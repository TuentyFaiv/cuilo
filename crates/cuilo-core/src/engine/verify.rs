//! `--verify`: a plan is applied to a throwaway copy of the project, and the project's own checks run there.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value as Json;

use super::{Engine, EngineError};
use crate::brand;
use crate::model::{Fact, RelPath, Stack};
use crate::plan::{Action, Plan, Validated};
use crate::process;
use crate::workspace::user_cache_dir;

const TIMEOUT: Duration = Duration::from_secs(600);
const MAX_FILES: usize = 50_000;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const TAIL_LINES: usize = 40;
/// How long a copy that an interrupted verification left stays before the next verification removes it.
const STALE: Duration = Duration::from_hours(24);
/// Script names run as checks, in order; `build` only when none of them exists.
const SCRIPTS: [&str; 5] = ["typecheck", "type-check", "check", "lint", "test"];
/// Folders left out of the copy: version control, cuilo's state, build output and caches.
const SKIPPED: [&str; 16] = [
	".git",
	".cuilo",
	"target",
	"dist",
	"build",
	"out",
	"coverage",
	".next",
	".nuxt",
	".astro",
	".svelte-kit",
	".turbo",
	".cache",
	".output",
	".vercel",
	"__pycache__",
];
/// Installed dependencies, linked into the copy instead of copied.
const LINKED: [&str; 3] = ["node_modules", ".venv", "venv"];

/// A check `--verify` runs: one of the project's own commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Check {
	/// The program, e.g. `bun`.
	pub program: String,
	/// Its arguments, e.g. `["run", "typecheck"]`.
	pub args: Vec<String>,
}

impl std::fmt::Display for Check {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.program)?;
		for arg in &self.args {
			write!(f, " {arg}")?;
		}
		Ok(())
	}
}

/// What one check reported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CheckResult {
	/// The command, as typed.
	pub command: String,
	/// It passed with the plan applied.
	pub passed: bool,
	/// For a failed check, whether it passes without the plan.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub passes_without_plan: Option<bool>,
	/// The plan causes the failure: the check passes without it, or prints new problems with it.
	pub blocking: bool,
	/// How long it ran with the plan, in seconds.
	pub seconds: u64,
	/// For a blocking check, what it printed that it does not print without the plan, or the last
	/// lines of its output.
	#[serde(skip_serializing_if = "String::is_empty")]
	pub output: String,
}

/// The checks of one verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Verification {
	/// Every check, in the order they ran.
	pub checks: Vec<CheckResult>,
}

impl Verification {
	/// Whether no check fails because of the plan.
	#[must_use]
	pub fn passed(&self) -> bool {
		self.checks.iter().all(|check| !check.blocking)
	}
}

impl Engine {
	/// The checks `--verify` would run: the project config's `verify` list, else detected ones,
	/// such as `bun run typecheck`, `cargo check --all-targets` or `go vet ./...`.
	pub fn checks(&self) -> Result<Vec<Check>, EngineError> {
		if !self.config.verify.is_empty() {
			return Ok(
				self
					.config
					.verify
					.iter()
					.filter_map(|command| {
						let (program, args) = command.split_first()?;
						Some(Check {
							program: program.clone(),
							args: args.to_vec(),
						})
					})
					.collect(),
			);
		}
		let stack = self.project()?.stack;
		let mut checks = Vec::new();
		let package = self
			.workspace
			.read(&crate::model::RelPath::constant("package.json"))?
			.and_then(|bytes| serde_json::from_slice::<Json>(&bytes).ok());
		if let Some(scripts) = package.as_ref().and_then(|package| package.get("scripts")?.as_object()) {
			let runner = script_runner(&stack);
			let mut found: Vec<&str> = SCRIPTS.into_iter().filter(|name| scripts.contains_key(*name)).collect();
			if found.is_empty() && scripts.contains_key("build") {
				found.push("build");
			}
			checks.extend(found.into_iter().map(|script| Check {
				program: runner.0.to_owned(),
				args: vec![runner.1.to_owned(), script.to_owned()],
			}));
		}
		let check = |program: &str, args: &[&str]| Check {
			program: program.to_owned(),
			args: args.iter().map(|arg| (*arg).to_owned()).collect(),
		};
		if stack.has(Fact::Language, "rust") {
			checks.push(check("cargo", &["check", "--all-targets", "--quiet"]));
		}
		if stack.has(Fact::Language, "go") {
			checks.push(check("go", &["vet", "./..."]));
		}
		if stack.has(Fact::Language, "python") {
			let tool = |name: &str| {
				let local = Path::new(".venv/bin").join(name);
				let root = self.root.as_deref().unwrap_or(Path::new("."));
				if root.join(&local).is_file() {
					local.to_string_lossy().into_owned()
				} else {
					name.to_owned()
				}
			};
			if stack.has(Fact::Formatter, "ruff") {
				checks.push(check(&tool("ruff"), &["check", "."]));
			}
			if stack.has(Fact::Testing, "pytest") {
				if stack.has(Fact::PackageManager, "uv") {
					checks.push(check("uv", &["run", "pytest", "-q"]));
				} else {
					checks.push(check(&tool("pytest"), &["-q"]));
				}
			}
		}
		Ok(checks)
	}

	/// Applies `plan` to a private throwaway copy of the project and runs [`checks`](Self::checks) there,
	/// rerunning failed checks without the plan to tell whether it caused them.
	///
	/// The checks run the pack's files, such as a `build.rs`, as the user: call it only in a trusted
	/// project, once the user has agreed to the plan.
	pub fn verify(&self, plan: &Plan<Validated>) -> Result<Verification, EngineError> {
		let fail = |message: &str| EngineError::Verify(message.to_owned());
		let root = self
			.root
			.as_deref()
			.ok_or_else(|| fail("verification needs a project folder on disk"))?;
		if !self.checks_trusted {
			return Err(fail(
				"verification runs this project's own checks, so it needs a trusted project; run `cuilo trust`",
			));
		}
		let checks = self.checks()?;
		if checks.is_empty() {
			return Err(fail(
				"no checks found: add scripts like `typecheck`, `lint` or `test`, or a `verify` list in the project config",
			));
		}
		let staging = self
			.staging()
			.map_err(|error| fail(&format!("cannot keep the copy private: {error}")))?;
		prune(&staging);
		let copy = Copy::of(root, &staging).map_err(|error| fail(&format!("could not copy the project: {error}")))?;
		write_changes(plan, copy.path())
			.map_err(|error| fail(&format!("could not apply the plan to the copy: {error}")))?;
		let target = cargo_target(root, &staging);
		let mut results = Vec::new();
		let mut baseline: Option<Copy> = None;
		for check in &checks {
			let (passed, seconds, output) = run(check, copy.path(), &target)?;
			if passed {
				results.push(CheckResult {
					command: check.to_string(),
					passed,
					passes_without_plan: None,
					blocking: false,
					seconds,
					output: String::new(),
				});
				continue;
			}
			if baseline.is_none() {
				baseline =
					Some(Copy::of(root, &staging).map_err(|error| fail(&format!("could not copy the project: {error}")))?);
			}
			let before_dir = baseline.as_ref().map_or(root, Copy::path);
			let (passes_before, _, before) = run(check, before_dir, &target)?;
			let added = if passes_before {
				Vec::new()
			} else {
				new_lines(&normalize(&output, copy.path()), &normalize(&before, before_dir))
			};
			let blocking = passes_before || !added.is_empty();
			results.push(CheckResult {
				command: check.to_string(),
				passed,
				passes_without_plan: Some(passes_before),
				blocking,
				seconds,
				output: match (blocking, added.is_empty()) {
					(false, _) => String::new(),
					(true, true) => tail(&output),
					(true, false) => tail(&added.join("\n")),
				},
			});
		}
		Ok(Verification { checks: results })
	}

	/// The folder for copies and Cargo build folders, checked by [`private_dir`], without links.
	fn staging(&self) -> io::Result<PathBuf> {
		let dir = match &self.verify_dir {
			Some(dir) => dir.clone(),
			None => user_cache_dir()
				.ok_or_else(|| {
					io::Error::other(format!(
						"there is no user cache folder; set {}CACHE_DIR",
						brand::ENV_PREFIX
					))
				})?
				.join("verify"),
		};
		private_dir(&dir)?;
		fs::canonicalize(dir)
	}
}

/// Creates `dir` for this user alone, or checks that it is: a real folder closed to other users, under no
/// folder every user can write to, since tools like Cargo read config from the folders above them.
fn private_dir(dir: &Path) -> io::Result<()> {
	let mut builder = fs::DirBuilder::new();
	builder.recursive(true);
	#[cfg(unix)]
	std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
	builder.create(dir)?;
	if !fs::symlink_metadata(dir)?.is_dir() {
		return Err(io::Error::other(format!("`{}` is not a folder", dir.display())));
	}
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt as _;
		let real = fs::canonicalize(dir)?;
		if fs::metadata(&real)?.permissions().mode() & 0o077 != 0 {
			return Err(io::Error::other(format!(
				"other users can open `{}`; run `chmod 700` on it",
				dir.display()
			)));
		}
		if let Some(shared) = real
			.ancestors()
			.skip(1)
			.find(|parent| fs::metadata(parent).is_ok_and(|entry| entry.permissions().mode() & 0o002 != 0))
		{
			return Err(io::Error::other(format!(
				"`{}` is inside `{}`, which every user can write to; set {}CACHE_DIR to a folder of your own",
				dir.display(),
				shared.display(),
				brand::ENV_PREFIX
			)));
		}
	}
	Ok(())
}

/// Removes the copies older than [`STALE`] that interrupted verifications left in `staging`.
fn prune(staging: &Path) {
	let Ok(entries) = fs::read_dir(staging) else {
		return;
	};
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |since| since.as_nanos());
	for entry in entries.flatten() {
		let made = entry.file_name().to_str().and_then(made_at);
		if made.is_some_and(|made| now.saturating_sub(made) > STALE.as_nanos()) {
			let _ = fs::remove_dir_all(entry.path());
		}
	}
}

/// When the copy called `name` was made, in nanoseconds since the Unix epoch.
fn made_at(name: &str) -> Option<u128> {
	let mut parts = name.strip_prefix("copy-")?.split('-');
	parts.next()?;
	parts.next()?.parse().ok()
}

/// The command that runs a `package.json` script with the project's package manager.
fn script_runner(stack: &Stack) -> (&'static str, &'static str) {
	for (manager, runner) in [
		("bun", ("bun", "run")),
		("pnpm", ("pnpm", "run")),
		("yarn", ("yarn", "run")),
		("deno", ("deno", "task")),
	] {
		if stack.has(Fact::PackageManager, manager) {
			return runner;
		}
	}
	("npm", "run")
}

/// Runs `check` in `dir`; returns whether it passed, how long it took and the tail of its output.
fn run(check: &Check, dir: &Path, cargo_target: &Path) -> Result<(bool, u64, String), EngineError> {
	let started = Instant::now();
	let env: [(&str, &OsStr); 2] = [("CI", OsStr::new("1")), ("CARGO_TARGET_DIR", cargo_target.as_os_str())];
	let output = process::run_in(&check.program, &check.args, Vec::new(), TIMEOUT, Some(dir), &env);
	let seconds = started.elapsed().as_secs();
	Ok(match output {
		Ok(output) => {
			let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
			text.push_str(&String::from_utf8_lossy(&output.stderr));
			(output.status.success(), seconds, text)
		}
		Err(error) if error.not_found() => {
			return Err(EngineError::Verify(format!(
				"the check `{check}` needs `{}`, which is not installed",
				check.program
			)));
		}
		Err(error) => (false, seconds, error.to_string()),
	})
}

/// Each line of `text` with the copy's folder written as `.`, paired with a key where every run of
/// digits is `#`, so runs in different copies and at different speeds compare equal.
fn normalize(text: &str, dir: &Path) -> Vec<(String, String)> {
	let dir = dir.to_string_lossy();
	text
		.lines()
		.map(|line| {
			let line = line.replace(dir.as_ref(), ".").trim_end().to_owned();
			let mut key = String::with_capacity(line.len());
			for c in line.chars() {
				if !c.is_ascii_digit() {
					key.push(c);
				} else if !key.ends_with('#') {
					key.push('#');
				}
			}
			(line, key)
		})
		.filter(|(line, _)| !line.is_empty())
		.collect()
}

/// The lines of `with` whose key `without` does not have.
fn new_lines(with: &[(String, String)], without: &[(String, String)]) -> Vec<String> {
	let known: std::collections::HashSet<&String> = without.iter().map(|(_, key)| key).collect();
	with
		.iter()
		.filter(|(_, key)| !known.contains(key))
		.map(|(line, _)| line.clone())
		.collect()
}

fn tail(text: &str) -> String {
	let lines: Vec<&str> = text.lines().collect();
	lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}

/// A Cargo build folder in `staging`, one per project, kept between verifications so they stay fast.
fn cargo_target(root: &Path, staging: &Path) -> PathBuf {
	use std::hash::{Hash as _, Hasher as _};
	let mut hasher = std::collections::hash_map::DefaultHasher::new();
	root.hash(&mut hasher);
	staging.join(format!("target-{:016x}", hasher.finish()))
}

/// Writes every change of `plan` below `dir`, refusing paths that go through a link, like the linked
/// `node_modules`, which would reach the project itself.
fn write_changes(plan: &Plan<Validated>, dir: &Path) -> io::Result<()> {
	for change in plan.changes() {
		if change.action == Action::Unchanged {
			continue;
		}
		if let Some(link) = linked_part(dir, &change.path) {
			return Err(io::Error::other(format!(
				"`{}` is inside `{link}`, which the copy shares with the project",
				change.path
			)));
		}
		let path = change.path.under(dir);
		match change.action {
			Action::Unchanged => {}
			Action::Delete => match fs::remove_file(&path) {
				Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
				_ => {}
			},
			_ => {
				if let Some(parent) = path.parent() {
					fs::create_dir_all(parent)?;
				}
				fs::write(&path, &change.contents)?;
			}
		}
	}
	Ok(())
}

/// The first part of `path` below `dir` that is a link, as a path like `node_modules`.
fn linked_part(dir: &Path, path: &RelPath) -> Option<String> {
	let mut native = dir.to_path_buf();
	let mut part = String::new();
	for segment in path.as_str().split('/') {
		native.push(segment);
		if !part.is_empty() {
			part.push('/');
		}
		part.push_str(segment);
		if fs::symlink_metadata(&native).is_ok_and(|entry| entry.file_type().is_symlink()) {
			return Some(part);
		}
	}
	None
}

/// A copy of a project in the private `staging` folder, removed when dropped.
struct Copy {
	dir: PathBuf,
}

impl Copy {
	/// Copies `root` into a new folder of `staging` that only this user can open.
	fn of(root: &Path, staging: &Path) -> io::Result<Self> {
		static COUNTER: AtomicU64 = AtomicU64::new(0);
		let nanos = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map_or(0, |elapsed| elapsed.as_nanos());
		let name = format!(
			"copy-{}-{nanos}-{}",
			std::process::id(),
			COUNTER.fetch_add(1, Ordering::Relaxed)
		);
		let dir = staging.join(name);
		#[cfg_attr(not(unix), expect(unused_mut, reason = "only Unix sets a mode"))]
		let mut builder = fs::DirBuilder::new();
		#[cfg(unix)]
		std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
		builder.create(&dir)?;
		let copy = Self { dir };
		let mut budget = (MAX_FILES, MAX_BYTES);
		copy_dir(root, &copy.dir, &mut budget, staging)?;
		Ok(copy)
	}

	fn path(&self) -> &Path {
		&self.dir
	}
}

impl Drop for Copy {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.dir);
	}
}

/// Copies `from` into `to`, leaving out `staging` should the project hold it, such as a project in the
/// home folder.
fn copy_dir(from: &Path, to: &Path, budget: &mut (usize, u64), staging: &Path) -> io::Result<()> {
	for entry in fs::read_dir(from)? {
		let entry = entry?;
		let name = entry.file_name();
		let source = entry.path();
		if source == staging {
			continue;
		}
		let target = to.join(&name);
		let kind = entry.file_type()?;
		let name = name.to_string_lossy();
		if kind.is_dir() && LINKED.contains(&name.as_ref()) {
			link(&source, &target)?;
		} else if kind.is_dir() {
			if SKIPPED.contains(&name.as_ref()) {
				continue;
			}
			fs::create_dir(&target)?;
			copy_dir(&source, &target, budget, staging)?;
		} else if kind.is_symlink() {
			link_like(&source, &target)?;
		} else {
			let size = entry.metadata()?.len();
			if budget.0 == 0 || budget.1 < size {
				return Err(io::Error::other(format!(
					"the project has more than {MAX_FILES} files or {} MB outside build and dependency folders",
					MAX_BYTES / 1024 / 1024
				)));
			}
			budget.0 -= 1;
			budget.1 -= size;
			fs::copy(&source, &target)?;
		}
	}
	Ok(())
}

/// Links the folder `target` to `source`.
fn link(source: &Path, target: &Path) -> io::Result<()> {
	#[cfg(unix)]
	return std::os::unix::fs::symlink(source, target);
	#[cfg(windows)]
	return std::os::windows::fs::symlink_dir(source, target);
	#[cfg(not(any(unix, windows)))]
	return Err(io::Error::other("verification needs symbolic links"));
}

/// Recreates the symlink `source` at `target`, pointing where it points.
fn link_like(source: &Path, target: &Path) -> io::Result<()> {
	let destination = fs::read_link(source)?;
	#[cfg(unix)]
	return std::os::unix::fs::symlink(destination, target);
	#[cfg(windows)]
	return if source.is_dir() {
		std::os::windows::fs::symlink_dir(destination, target)
	} else {
		std::os::windows::fs::symlink_file(destination, target)
	};
	#[cfg(not(any(unix, windows)))]
	return Err(io::Error::other("verification needs symbolic links"));
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn copy_should_skip_build_output_and_link_dependencies() {
		let root = tempfile::tempdir().unwrap();
		for (path, text) in [
			("src/a.ts", "a"),
			("dist/a.js", "x"),
			(".git/HEAD", "x"),
			("node_modules/x/index.js", "x"),
		] {
			let path = root.path().join(path);
			fs::create_dir_all(path.parent().unwrap()).unwrap();
			fs::write(path, text).unwrap();
		}

		let staging = tempfile::tempdir().unwrap();
		let copy = Copy::of(root.path(), staging.path()).unwrap();
		let dir = copy.path().to_path_buf();

		assert_eq!(fs::read_to_string(dir.join("src/a.ts")).unwrap(), "a");
		assert!(!dir.join("dist").exists() && !dir.join(".git").exists());
		assert!(
			fs::symlink_metadata(dir.join("node_modules"))
				.unwrap()
				.file_type()
				.is_symlink()
		);
		drop(copy);
		assert!(!dir.exists());
	}

	#[cfg(unix)]
	#[test]
	fn copies_should_open_only_for_their_user() {
		use std::os::unix::fs::PermissionsExt as _;
		let (root, staging) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
		fs::write(root.path().join("a.ts"), "a").unwrap();

		let copy = Copy::of(root.path(), staging.path()).unwrap();

		assert_eq!(fs::metadata(copy.path()).unwrap().permissions().mode() & 0o777, 0o700);
	}

	#[test]
	fn a_copy_should_leave_out_a_staging_folder_inside_the_project() {
		let root = tempfile::tempdir().unwrap();
		let staging = root.path().join(".home/cache/verify");
		fs::create_dir_all(&staging).unwrap();
		fs::write(root.path().join("a.ts"), "a").unwrap();
		let (root, staging) = (root.path().canonicalize().unwrap(), staging.canonicalize().unwrap());

		let copy = Copy::of(&root, &staging).unwrap();

		assert!(copy.path().join("a.ts").exists());
		assert!(copy.path().join(".home/cache").exists() && !copy.path().join(".home/cache/verify").exists());
	}

	#[test]
	fn write_changes_should_refuse_paths_through_a_link() {
		let root = tempfile::tempdir().unwrap();
		fs::create_dir_all(root.path().join("node_modules/dep")).unwrap();
		let staging = tempfile::tempdir().unwrap();
		let copy = Copy::of(root.path(), staging.path()).unwrap();
		let (inside, outside) = (
			RelPath::new("src/a.ts").unwrap(),
			RelPath::new("node_modules/x/index.js").unwrap(),
		);

		assert_eq!(linked_part(copy.path(), &inside), None);
		assert_eq!(linked_part(copy.path(), &outside).as_deref(), Some("node_modules"));
	}

	#[cfg(unix)]
	#[test]
	fn private_dir_should_refuse_a_folder_other_users_can_reach() {
		use std::os::unix::fs::PermissionsExt as _;
		let base = tempfile::tempdir().unwrap();
		let open = base.path().join("open");
		fs::create_dir(&open).unwrap();
		fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
		let shared = base.path().join("shared");
		fs::create_dir(&shared).unwrap();
		fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();

		assert!(private_dir(&open).unwrap_err().to_string().contains("chmod 700"));
		assert!(
			private_dir(&shared.join("verify"))
				.unwrap_err()
				.to_string()
				.contains("every user can write")
		);
	}

	#[test]
	fn prune_should_remove_only_stale_copies() {
		let staging = tempfile::tempdir().unwrap();
		let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
		let (stale, fresh) = (
			format!("copy-1-{}-0", now - 2 * STALE.as_nanos()),
			format!("copy-1-{now}-1"),
		);
		for name in [&stale, &fresh, &"target-0123456789abcdef".to_owned()] {
			fs::create_dir(staging.path().join(name)).unwrap();
		}

		prune(staging.path());

		assert!(!staging.path().join(&stale).exists());
		assert!(staging.path().join(&fresh).exists() && staging.path().join("target-0123456789abcdef").exists());
	}

	#[test]
	fn script_runner_should_follow_the_package_manager() {
		let mut stack = Stack::default();
		assert_eq!(script_runner(&stack), ("npm", "run"));
		stack.insert(Fact::PackageManager, "bun");
		assert_eq!(script_runner(&stack), ("bun", "run"));
	}

	#[test]
	fn new_lines_should_ignore_paths_timings_and_known_problems() {
		let with = normalize(
			"/tmp/a/src/x.ts(3,1): error TS1\n/tmp/a/src/y.ts(9,2): error TS2322: Type 'string'\nDone in 12ms\n",
			Path::new("/tmp/a"),
		);
		let without = normalize("/tmp/b/src/x.ts(4,1): error TS1\nDone in 340ms\n", Path::new("/tmp/b"));

		assert_eq!(
			new_lines(&with, &without),
			["./src/y.ts(9,2): error TS2322: Type 'string'"]
		);
	}

	#[test]
	fn tail_should_keep_the_last_lines() {
		let text = (0..100).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");

		assert_eq!(tail(&text).lines().next(), Some("60"));
	}
}
