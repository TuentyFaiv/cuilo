//! `cargo xtask verify-packs`: adds every official piece, in each of its variants, to a real project of that stack with
//! the cuilo binary, then runs the ecosystem's own checks and the tests the piece wrote.

mod declared;
mod follow;
mod gradle;
mod stacks;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::docs::{build_cuilo, described_example, is_required};
use crate::release::root;
use follow::Added;
use stacks::{Check, Ecosystem, GRADLE_SETUP, Probe, STACKS, Stack, UNCOVERED};

pub(crate) const USAGE: &str = "\
Usage: cargo xtask verify-packs [options]

Adds each official piece, in each variant, to a copy of its stack's fixture in fixtures/stacks/, then runs that
ecosystem's checks and tests. Stops at the first failure.

Options:
  --list                 List the combinations and their commands instead of running them
  --stack <name>         Only this stack, e.g. `svelte` or `hono-prisma`
  --group <group>        Only this toolchain: typescript, rust, go, python, flutter, kotlin or swift
  --pack <pack>          Only this pack, e.g. `backend`
  --piece <piece>        Only this piece, e.g. `ui/atom` or `atom`
  --latest               Update every dependency to its newest release before adding the piece
  --require-toolchains   Fail instead of skipping a combination whose toolchain is missing
  --keep                 Keep each combination's folder in target/verify-packs after it passes

Filters can be repeated; a combination runs when it matches every kind of filter given.";

/// The fake database URL every command sees; nothing connects to it.
const DATABASE_URL: &str = "postgresql://verify:verify@127.0.0.1:5432/verify";
/// Folders a fixture copy leaves out: dependencies and build output from running a fixture in place.
const SKIPPED: [&str; 17] = [
	"node_modules",
	"target",
	".venv",
	".svelte-kit",
	".angular",
	".astro",
	"dist",
	"__pycache__",
	".mypy_cache",
	".pytest_cache",
	".dart_tool",
	".gradle",
	".kotlin",
	"build",
	".build",
	".swiftpm",
	"DerivedData",
];

/// What `verify-packs` was asked to do.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools, reason = "independent command-line flags")]
struct Options {
	list: bool,
	latest: bool,
	require_toolchains: bool,
	keep: bool,
	stacks: Vec<String>,
	groups: Vec<String>,
	packs: Vec<String>,
	pieces: Vec<String>,
}

impl Options {
	fn parse(args: &[String]) -> Result<Self, String> {
		let mut options = Self::default();
		let mut args = args.iter();
		while let Some(arg) = args.next() {
			let (flag, inline) = match arg.split_once('=') {
				Some((flag, value)) => (flag, Some(value.to_owned())),
				None => (arg.as_str(), None),
			};
			let mut value = || {
				inline
					.clone()
					.or_else(|| args.next().cloned())
					.ok_or_else(|| format!("`{flag}` needs a value\n\n{USAGE}"))
			};
			match flag {
				"--list" => options.list = true,
				"--latest" => options.latest = true,
				"--require-toolchains" => options.require_toolchains = true,
				"--keep" => options.keep = true,
				"--stack" => options.stacks.push(value()?),
				"--group" => options.groups.push(value()?),
				"--pack" => options.packs.push(value()?),
				"--piece" => options.pieces.push(value()?),
				"-h" | "--help" => return Err(USAGE.to_owned()),
				other => return Err(format!("unknown option `{other}`\n\n{USAGE}")),
			}
		}
		Ok(options)
	}

	/// Fails on a filter that matches nothing it could.
	fn validate(&self, packs: &[Pack]) -> Result<(), String> {
		let known = |kind: &str, given: &[String], valid: &[String]| -> Result<(), String> {
			match given.iter().find(|value| !valid.contains(value)) {
				Some(value) => Err(format!("no {kind} `{value}`; known: {}", valid.join(", "))),
				None => Ok(()),
			}
		};
		let stacks: Vec<String> = STACKS.iter().map(|stack| stack.name.to_owned()).collect();
		let mut groups: Vec<String> = Ecosystem::ALL
			.iter()
			.map(|ecosystem| ecosystem.group().to_owned())
			.collect();
		groups.dedup();
		let pack_names: Vec<String> = packs.iter().map(|pack| pack.name.clone()).collect();
		let pieces: Vec<String> = packs
			.iter()
			.flat_map(|pack| {
				pack
					.pieces
					.iter()
					.flat_map(|piece| [piece.name.clone(), format!("{}/{}", pack.name, piece.name)])
			})
			.collect();
		known("stack", &self.stacks, &stacks)?;
		known("group", &self.groups, &groups)?;
		known("pack", &self.packs, &pack_names)?;
		known("piece", &self.pieces, &pieces)
	}

	fn selects(&self, combination: &Combination<'_>) -> bool {
		let matches =
			|filters: &[String], value: &dyn Fn(&str) -> bool| filters.is_empty() || filters.iter().any(|f| value(f));
		matches(&self.stacks, &|name| name == combination.stack.name)
			&& matches(&self.groups, &|group| group == combination.stack.ecosystem.group())
			&& matches(&self.packs, &|pack| pack == combination.pack)
			&& matches(&self.pieces, &|piece| {
				piece == combination.piece.name || piece == format!("{}/{}", combination.pack, combination.piece.name)
			})
	}
}

/// A pack as `cuilo pack show` describes it.
#[derive(Debug)]
struct Pack {
	name: String,
	pieces: Vec<Piece>,
}

/// One piece of a pack.
#[derive(Debug)]
struct Piece {
	name: String,
	target: String,
	inputs: Map<String, Value>,
	/// Variant names with the folder each writes to, the first of its alternatives when it lists several.
	variants: Vec<(String, String)>,
}

impl Piece {
	fn from_json(value: &Value) -> Self {
		let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
		let target = text(&value["target"]);
		let first = |target: &Value| target.as_str().or_else(|| target[0].as_str()).map(str::to_owned);
		let variants = value["variants"]
			.as_array()
			.into_iter()
			.flatten()
			.map(|variant| {
				let folder = first(&variant["target"]).unwrap_or_else(|| target.clone());
				(text(&variant["name"]), folder)
			})
			.collect();
		Self {
			name: text(&value["name"]),
			target,
			inputs: value["inputs"].as_object().cloned().unwrap_or_default(),
			variants,
		}
	}

	fn target(&self, variant: &str) -> &str {
		self
			.variants
			.iter()
			.find(|(name, _)| name == variant)
			.map_or(self.target.as_str(), |(_, target)| target.as_str())
	}

	fn has(&self, variant: &str) -> bool {
		self.variants.iter().any(|(name, _)| name == variant)
	}
}

/// One piece in one variant, added to a copy of one stack's fixture, after the pieces it builds on.
struct Combination<'a> {
	stack: &'static Stack,
	pack: &'a str,
	piece: &'a Piece,
	variant: &'static str,
	first: Vec<(&'a Piece, &'static str)>,
}

impl Combination<'_> {
	fn label(&self) -> String {
		format!(
			"{} · {}/{} ({})",
			self.stack.name, self.pack, self.piece.name, self.variant
		)
	}

	/// Whether a piece it adds writes tests, which the pieces with a `tests` input do.
	fn writes_tests(&self) -> bool {
		self
			.first
			.iter()
			.map(|(piece, _)| *piece)
			.chain([self.piece])
			.any(|piece| piece.inputs.contains_key("tests"))
	}

	/// The arguments that select this combination alone.
	fn filter(&self) -> String {
		format!("--stack {} --piece {}/{}", self.stack.name, self.pack, self.piece.name)
	}
}

/// Every combination the stacks cover, checking that each variant of each piece is covered or listed as not yet.
fn matrix(packs: &[Pack]) -> Result<Vec<Combination<'_>>, String> {
	let mut combinations = Vec::new();
	for stack in STACKS {
		for cover in stack.covers {
			let pack = packs.iter().find(|pack| pack.name == cover.pack).ok_or_else(|| {
				format!(
					"stack `{}` covers the pack `{}`, which does not exist",
					stack.name, cover.pack
				)
			})?;
			let find = |name: &str| pack.pieces.iter().find(|piece| piece.name == name);
			let mut first = Vec::new();
			for (name, variant) in cover.first {
				let piece = find(name).filter(|piece| piece.has(variant)).ok_or_else(|| {
					format!(
						"stack `{}` adds {}/{name} ({variant}) first, which does not exist",
						stack.name, pack.name
					)
				})?;
				first.push((piece, *variant));
			}
			let before = combinations.len();
			for piece in pack.pieces.iter().filter(|piece| piece.has(cover.variant)) {
				if cover.pieces.is_empty() || cover.pieces.contains(&piece.name.as_str()) {
					combinations.push(Combination {
						stack,
						pack: &pack.name,
						piece,
						variant: cover.variant,
						first: first.clone(),
					});
				}
			}
			if combinations.len() == before {
				return Err(format!(
					"stack `{}` covers no piece of `{}` with the variant `{}`",
					stack.name, cover.pack, cover.variant
				));
			}
		}
	}
	let problems = coverage_problems(packs, &combinations, UNCOVERED);
	if problems.is_empty() {
		Ok(combinations)
	} else {
		Err(format!(
			"{}\nfix crates/xtask/src/verify/stacks.rs: cover each variant with a stack, or list it in UNCOVERED until one does",
			problems.join("\n")
		))
	}
}

/// Variants of `packs` that no combination covers and `uncovered` does not list, and entries of `uncovered` that
/// are covered or name no variant.
fn coverage_problems(
	packs: &[Pack],
	combinations: &[Combination<'_>],
	uncovered: &[(&str, &[&str], &str)],
) -> Vec<String> {
	let covered = |pack: &str, piece: &str, variant: &str| {
		combinations
			.iter()
			.any(|combination| combination.pack == pack && combination.piece.name == piece && combination.variant == variant)
	};
	let mut problems = Vec::new();
	for pack in packs {
		for piece in &pack.pieces {
			for (variant, _) in &piece.variants {
				let listed = uncovered
					.iter()
					.any(|(name, variants, _)| *name == pack.name && variants.contains(&variant.as_str()));
				match (covered(&pack.name, &piece.name, variant), listed) {
					(false, false) => problems.push(format!("no stack verifies {}/{} ({variant})", pack.name, piece.name)),
					(true, true) => problems.push(format!(
						"{}/{} ({variant}) is verified but still listed as not covered",
						pack.name, piece.name
					)),
					_ => {}
				}
			}
		}
	}
	for (name, variants, _) in uncovered {
		for variant in *variants {
			let exists = packs
				.iter()
				.filter(|pack| pack.name == *name)
				.flat_map(|pack| &pack.pieces)
				.any(|piece| piece.has(variant));
			if !exists {
				problems.push(format!("UNCOVERED lists {name} ({variant}), which no piece has"));
			}
		}
	}
	problems
}

/// Runs `verify-packs` with the arguments after the task name.
pub(crate) fn run(args: &[String]) -> Result<(), String> {
	let options = Options::parse(args)?;
	let root = root();
	let root = root.canonicalize().unwrap_or(root);
	let scratch = root.join("target/verify-packs");
	eprintln!("→ building cuilo");
	let cuilo = build_cuilo(&root)?;
	let context = Context::new(&root, scratch, cuilo, options.latest)?;
	let packs = context.packs()?;
	options.validate(&packs)?;
	let all = matrix(&packs)?;
	let selected: Vec<&Combination<'_>> = all.iter().filter(|combination| options.selects(combination)).collect();
	if selected.is_empty() {
		return Err("no combination matches these filters".to_owned());
	}
	let mut toolchains = Toolchains::default();
	if options.list {
		print!("{}", listing(&selected, &mut toolchains, options.latest));
		return Ok(());
	}
	eprintln!(
		"→ {} combinations{}; commands run with {}",
		selected.len(),
		if options.latest {
			", dependencies at their newest releases"
		} else {
			""
		},
		context.shown_env()
	);
	let mut outcomes = Vec::new();
	let result = run_all(&selected, &context, &options, &mut toolchains, &mut outcomes);
	report(&outcomes);
	result
}

fn run_all(
	selected: &[&Combination<'_>],
	context: &Context,
	options: &Options,
	toolchains: &mut Toolchains,
	outcomes: &mut Vec<Outcome>,
) -> Result<(), String> {
	for (index, combination) in selected.iter().enumerate() {
		let label = combination.label();
		if let Err(reason) = toolchains.check(combination.stack.ecosystem, options.latest) {
			if options.require_toolchains {
				outcomes.push(Outcome::failed(label, Duration::ZERO));
				return Err(format!("{reason}, and --require-toolchains is set"));
			}
			eprintln!("\n[{}/{}] {label}: skipped, {reason}", index + 1, selected.len());
			outcomes.push(Outcome {
				label,
				result: format!("skip ({reason})"),
				time: Duration::ZERO,
			});
			continue;
		}
		eprintln!("\n[{}/{}] {label}", index + 1, selected.len());
		let started = Instant::now();
		match verify(combination, context) {
			Ok(dir) => {
				let time = started.elapsed();
				eprintln!("  ✓ passed in {}", seconds(time));
				outcomes.push(Outcome {
					label,
					result: "pass".to_owned(),
					time,
				});
				if !options.keep {
					fs::remove_dir_all(&dir).map_err(|error| format!("could not remove {}: {error}", dir.display()))?;
				}
			}
			Err(error) => {
				outcomes.push(Outcome::failed(label, started.elapsed()));
				return Err(format!(
					"{error}\nReproduce it with `cargo xtask verify-packs {}{}`",
					combination.filter(),
					if options.latest { " --latest" } else { "" }
				));
			}
		}
	}
	Ok(())
}

/// The cuilo binary, the scratch folders and the environment every command runs with.
struct Context {
	root: PathBuf,
	scratch: PathBuf,
	cuilo: PathBuf,
	latest: bool,
	env: Vec<(String, String)>,
}

impl Context {
	fn new(root: &Path, scratch: PathBuf, cuilo: PathBuf, latest: bool) -> Result<Self, String> {
		let home = scratch.join(".cuilo");
		for dir in ["config", "cache", "project"] {
			fs::create_dir_all(home.join(dir)).map_err(|error| format!("could not create {}: {error}", home.display()))?;
		}
		let path = |dir: &str| home.join(dir).display().to_string();
		let env = [
			("CUILO_CONFIG_DIR", path("config")),
			("CUILO_CACHE_DIR", path("cache")),
			("CUILO_UPDATES", "off".to_owned()),
			("DATABASE_URL", DATABASE_URL.to_owned()),
			("NO_COLOR", "1".to_owned()),
			("GOTOOLCHAIN", "local".to_owned()),
			("NG_CLI_ANALYTICS", "false".to_owned()),
			("ASTRO_TELEMETRY_DISABLED", "1".to_owned()),
			("CHECKPOINT_DISABLE", "1".to_owned()),
			("FLUTTER_SUPPRESS_ANALYTICS", "true".to_owned()),
		]
		.into_iter()
		.map(|(key, value)| (key.to_owned(), value))
		.collect();
		Ok(Self {
			root: root.to_path_buf(),
			scratch,
			cuilo,
			latest,
			env,
		})
	}

	fn shown_env(&self) -> String {
		self
			.env
			.iter()
			.map(|(key, value)| format!("{key}={}", quote(value)))
			.collect::<Vec<_>>()
			.join(" ")
	}

	/// The official packs, as the cuilo binary loads them.
	fn packs(&self) -> Result<Vec<Pack>, String> {
		let mut neutral = Project::new(self.scratch.join(".cuilo/project"), self.env.clone());
		neutral.echo = false;
		let listed = neutral.json(&[self.cuilo_arg(), "--json", "pack", "list"])?;
		let mut packs = Vec::new();
		for pack in listed.as_array().into_iter().flatten() {
			let name = pack["name"].as_str().unwrap_or_default().to_owned();
			let shown = neutral.json(&[self.cuilo_arg(), "--json", "pack", "show", &name])?;
			let pieces = shown["pieces"]
				.as_array()
				.into_iter()
				.flatten()
				.map(Piece::from_json)
				.collect();
			packs.push(Pack { name, pieces });
		}
		Ok(packs)
	}

	fn cuilo_arg(&self) -> &str {
		self.cuilo.to_str().unwrap_or("cuilo")
	}

	/// The environment of `stack`'s commands: the shared one, plus a Cargo target folder per stack so builds are
	/// incremental across its combinations.
	fn env_for(&self, stack: &Stack) -> Vec<(String, String)> {
		let mut env = self.env.clone();
		if stack.ecosystem == Ecosystem::Cargo {
			let target = self.scratch.join(".cargo-target").join(stack.name);
			env.push(("CARGO_TARGET_DIR".to_owned(), target.display().to_string()));
		}
		env
	}
}

/// Adds `combination` to a fresh copy of its fixture and runs the checks; returns the copy's folder.
fn verify(combination: &Combination<'_>, context: &Context) -> Result<PathBuf, String> {
	let stack = combination.stack;
	let dir = context
		.scratch
		.join(stack.name)
		.join(format!("{}-{}", combination.pack, combination.piece.name));
	if dir.exists() {
		fs::remove_dir_all(&dir).map_err(|error| format!("could not remove {}: {error}", dir.display()))?;
	}
	let fixture = context.root.join("fixtures/stacks").join(stack.name);
	copy_dir(&fixture, &dir)?;
	eprintln!("  in {}", dir.display());
	let mut project = Project::new(dir.clone(), context.env_for(stack));
	install(stack, &mut project, context.latest)?;
	detect(stack, &project, context)?;
	let pieces = combination
		.first
		.iter()
		.copied()
		.chain([(combination.piece, combination.variant)]);
	for (piece, variant) in pieces {
		add(combination.pack, piece, variant, stack, &mut project, context)?;
	}
	if context.latest && stack.follow.pins_versions() {
		gradle::update(&mut project, stack.hold)?;
	}
	for cmd in stack.ecosystem.settle() {
		project.run(cmd)?;
	}
	for check in stack.checks {
		project.check(check)?;
	}
	if combination.writes_tests() {
		for check in stack.tests {
			project.check(check)?;
		}
	} else {
		eprintln!(
			"  (no tests to run: {}/{} writes none)",
			combination.pack, combination.piece.name
		);
	}
	Ok(dir)
}

/// Installs the fixture's dependencies, or with `latest` updates them to their newest releases, then prepares the
/// stack's tools.
fn install(stack: &Stack, project: &mut Project, latest: bool) -> Result<(), String> {
	if latest {
		unpin(stack, project)?;
		for cmd in latest_updates(stack, &project.dir)? {
			project.run(&cmd.iter().map(String::as_str).collect::<Vec<_>>())?;
		}
	} else {
		project.run(stack.ecosystem.install())?;
	}
	if stack.browser {
		let mut playwright = vec!["bun", "run", "playwright", "install", "--only-shell", "chromium"];
		if cfg!(target_os = "linux") && std::env::var_os("CI").is_some() {
			playwright.push("--with-deps");
		}
		project.run(&playwright)?;
	}
	for cmd in stack.prepare {
		project.run(cmd)?;
	}
	Ok(())
}

/// The commands that move every dependency of the fixture at `dir` to its newest release, except those `stack` holds.
fn latest_updates(stack: &Stack, dir: &Path) -> Result<Vec<Vec<String>>, String> {
	let owned = |cmd: &[&str]| cmd.iter().map(|arg| (*arg).to_owned()).collect::<Vec<String>>();
	Ok(match stack.ecosystem {
		Ecosystem::Bun => {
			let manifest = read_json(&dir.join("package.json"))?;
			let mut cmd = owned(&["bun", "update", "--latest"]);
			for section in ["dependencies", "devDependencies"] {
				let names = manifest[section].as_object().into_iter().flat_map(Map::keys);
				cmd.extend(names.filter(|name| !stack.hold.contains(&name.as_str())).cloned());
			}
			vec![cmd]
		}
		Ecosystem::Cargo => {
			let mut upgrade = owned(&["cargo", "upgrade", "--incompatible", "allow"]);
			for held in stack.hold {
				upgrade.extend(["--exclude".to_owned(), (*held).to_owned()]);
			}
			vec![upgrade, owned(&["cargo", "update"])]
		}
		Ecosystem::Go => vec![
			owned(&["go", "get", "-u", "-t", "./..."]),
			owned(&["go", "mod", "tidy"]),
		],
		Ecosystem::Uv => vec![owned(&["uv", "lock", "--upgrade"]), owned(&["uv", "sync"])],
		Ecosystem::Flutter => {
			let mut upgrade = owned(&["flutter", "pub", "upgrade", "--major-versions"]);
			if !stack.hold.is_empty() {
				let pubspec = dir.join("pubspec.yaml");
				let text =
					fs::read_to_string(&pubspec).map_err(|error| format!("could not read {}: {error}", pubspec.display()))?;
				upgrade.extend(
					pubspec_packages(&text)
						.into_iter()
						.filter(|name| !stack.hold.contains(&name.as_str())),
				);
			}
			vec![upgrade]
		}
		Ecosystem::Gradle | Ecosystem::Android => vec![owned(GRADLE_SETUP)],
		Ecosystem::Xcode => vec![owned(Ecosystem::Xcode.install())],
		Ecosystem::SwiftPm => vec![owned(&["swift", "package", "update"])],
	})
}

/// Edits the fixture so the commands of [`latest_updates`] reach the newest releases: Gradle's version catalog and
/// wrapper, which no Gradle command updates, and the package versions an Xcode project resolved.
fn unpin(stack: &Stack, project: &mut Project) -> Result<(), String> {
	match stack.ecosystem {
		Ecosystem::Gradle | Ecosystem::Android => gradle::update(project, stack.hold),
		Ecosystem::Xcode | Ecosystem::SwiftPm if !stack.hold.is_empty() => Err(format!(
			"`--latest` cannot hold packages of the Swift stack `{}`; empty its `hold`",
			stack.name
		)),
		Ecosystem::Xcode => {
			let resolved: Vec<String> = fs::read_dir(&project.dir)
				.map_err(|error| format!("could not list {}: {error}", project.dir.display()))?
				.filter_map(Result::ok)
				.filter_map(|entry| entry.file_name().into_string().ok())
				.filter(|name| name.ends_with(".xcodeproj"))
				.map(|name| format!("{name}/project.xcworkspace/xcshareddata/swiftpm/Package.resolved"))
				.filter(|path| project.dir.join(path).is_file())
				.collect();
			for path in resolved {
				eprintln!("  ✎ {path}: remove, so packages resolve to their newest allowed releases");
				let file = project.dir.join(&path);
				fs::remove_file(&file).map_err(|error| format!("could not remove {}: {error}", file.display()))?;
			}
			Ok(())
		}
		Ecosystem::Bun | Ecosystem::Cargo | Ecosystem::Go | Ecosystem::Uv | Ecosystem::Flutter | Ecosystem::SwiftPm => {
			Ok(())
		}
	}
}

/// The hosted packages `pubspec.yaml` lists under `dependencies` and `dev_dependencies`, leaving out the SDK's.
fn pubspec_packages(text: &str) -> Vec<String> {
	let mut names = Vec::new();
	let mut in_dependencies = false;
	let mut pending: Option<String> = None;
	for line in text.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		let indent = line.len() - line.trim_start().len();
		if indent == 0 || (indent == 2 && in_dependencies) {
			names.extend(pending.take());
		}
		if indent == 0 {
			in_dependencies = matches!(trimmed, "dependencies:" | "dev_dependencies:");
		} else if in_dependencies && indent == 2 {
			pending = trimmed.split_once(':').map(|(name, _)| name.trim().to_owned());
		} else if trimmed.starts_with("sdk:") {
			pending = None;
		}
	}
	names.extend(pending);
	names
}

/// Fails unless `cuilo detect` reports the facts `stack` expects of its fixture.
fn detect(stack: &Stack, project: &Project, context: &Context) -> Result<(), String> {
	let detected = project.json(&[context.cuilo_arg(), "--json", "detect"])?;
	let missing: Vec<String> = stack
		.detects
		.iter()
		.filter(|(fact, value)| {
			!detected["stack"][*fact]
				.as_array()
				.is_some_and(|values| values.iter().any(|found| found == value))
		})
		.map(|(fact, value)| format!("{fact}={value}"))
		.collect();
	if missing.is_empty() {
		Ok(())
	} else {
		Err(format!(
			"`cuilo detect` does not see {} in fixtures/stacks/{}; it reports {}",
			missing.join(", "),
			stack.name,
			detected["stack"]
		))
	}
}

/// Runs `cuilo add` for `piece` in `variant`, installing what it lists, then follows its next steps.
fn add(
	pack: &str,
	piece: &Piece,
	variant: &str,
	stack: &Stack,
	project: &mut Project,
	context: &Context,
) -> Result<(), String> {
	let inputs = inputs(piece, stack, &project.dir)?;
	let mut cmd: Vec<String> = [context.cuilo_arg(), "--json", "--no-input", "add"]
		.into_iter()
		.map(str::to_owned)
		.collect();
	cmd.push(format!("{pack}/{}", piece.name));
	if let Some(name) = inputs.get("name") {
		cmd.push(name.clone());
	}
	for (key, value) in inputs.iter().filter(|(key, _)| *key != "name") {
		cmd.extend(["-i".to_owned(), format!("{key}={value}")]);
	}
	cmd.extend(
		["--variant", variant, "--wire", "--install", "--yes"]
			.into_iter()
			.map(str::to_owned),
	);
	let result = project.json(&cmd.iter().map(String::as_str).collect::<Vec<_>>())?;
	in_sources(stack, &result)?;
	let added = Added {
		piece: &piece.name,
		target: piece.target(variant),
		name: inputs.get("name").map(String::as_str),
		plural: inputs.get("plural").map(String::as_str),
	};
	stack.follow.apply(&added, project)
}

/// Fails when the `cuilo add` that printed `result` created a file outside the folders `stack` builds.
fn in_sources(stack: &Stack, result: &Value) -> Result<(), String> {
	if stack.sources.is_empty() {
		return Ok(());
	}
	let outside: Vec<&str> = result["plan"]["changes"]
		.as_array()
		.into_iter()
		.flatten()
		.filter(|change| change["action"] == "create")
		.filter_map(|change| change["path"].as_str())
		.filter(|path| !stack.sources.iter().any(|source| path.starts_with(source)))
		.collect();
	if outside.is_empty() {
		Ok(())
	} else {
		Err(format!(
			"`cuilo add` created {}, outside {}, the folders fixtures/stacks/{} builds, so the checks would not see it",
			outside.join(", "),
			stack.sources.join(" and "),
			stack.name
		))
	}
}

/// The inputs `cuilo add` gets: `tests=true`, the values the fixture declares, like its Go module, and each other
/// required input's example.
fn inputs(piece: &Piece, stack: &Stack, dir: &Path) -> Result<BTreeMap<String, String>, String> {
	let mut values = BTreeMap::new();
	for (name, input) in &piece.inputs {
		if name == "tests" {
			values.insert(name.clone(), "true".to_owned());
		} else if let Some(value) = declared::value(name, stack, dir)? {
			values.insert(name.clone(), value);
		} else if is_required(input) {
			let example = described_example(input["description"].as_str().unwrap_or_default())
				.or_else(|| input["example"].as_str())
				.ok_or_else(|| format!("`{}` has no example for its required input `{name}`", piece.name))?;
			values.insert(name.clone(), example.to_owned());
		}
	}
	Ok(values)
}

/// A copy of a fixture that commands run in, printing each before it runs.
pub(crate) struct Project {
	dir: PathBuf,
	env: Vec<(String, String)>,
	/// Whether commands are printed before they run.
	echo: bool,
}

impl Project {
	fn new(dir: PathBuf, env: Vec<(String, String)>) -> Self {
		Self { dir, env, echo: true }
	}

	/// Runs `cmd` in the project and fails unless it succeeds.
	pub(crate) fn run(&mut self, cmd: &[&str]) -> Result<(), String> {
		self.output(cmd, false).map(|_| ())
	}

	fn check(&mut self, check: &Check) -> Result<(), String> {
		self.output(check.cmd, check.quiet).map(|_| ())
	}

	fn json(&self, cmd: &[&str]) -> Result<Value, String> {
		let stdout = self.output(cmd, false)?;
		serde_json::from_str(&stdout).map_err(|error| format!("`{}` printed no JSON: {error}", shell(cmd)))
	}

	/// Runs `cmd`, returning its standard output; with `quiet`, printing anything is a failure too.
	fn output(&self, cmd: &[&str], quiet: bool) -> Result<String, String> {
		let Some((program, args)) = cmd.split_first() else {
			return Err("an empty command".to_owned());
		};
		if self.echo {
			eprintln!("  $ {}", shell(cmd));
		}
		fs::create_dir_all(&self.dir).map_err(|error| format!("could not create {}: {error}", self.dir.display()))?;
		let output = Command::new(program)
			.args(args)
			.current_dir(&self.dir)
			.envs(self.env.iter().map(|(key, value)| (key, value)))
			.stdin(Stdio::null())
			.output()
			.map_err(|error| self.failure(cmd, &format!("could not start `{program}`: {error}"), ""))?;
		let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
		let printed = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
		if !output.status.success() {
			return Err(self.failure(cmd, &format!("failed ({})", output.status), &printed));
		}
		if quiet && !printed.trim().is_empty() {
			return Err(self.failure(cmd, "printed something, which means it found a problem", &printed));
		}
		Ok(stdout)
	}

	fn failure(&self, cmd: &[&str], what: &str, printed: &str) -> String {
		let env: Vec<String> = self
			.env
			.iter()
			.map(|(key, value)| format!("{key}={}", quote(value)))
			.collect();
		format!(
			"`{}` {what}\n{}\nRun it again with:\n  cd {} && env {} {}",
			shell(cmd),
			printed.trim_end(),
			quote(&self.dir.display().to_string()),
			env.join(" "),
			shell(cmd)
		)
	}

	/// Changes the project file at `path`, which may not exist yet, describing the change as `what`.
	pub(crate) fn edit(
		&mut self,
		path: &str,
		what: &str,
		change: impl FnOnce(String) -> Result<String, String>,
	) -> Result<(), String> {
		eprintln!("  ✎ {path}: {what}");
		let file = self.dir.join(path);
		let text = match fs::read_to_string(&file) {
			Ok(text) => text,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
			Err(error) => return Err(format!("could not read {}: {error}", file.display())),
		};
		let changed = change(text).map_err(|error| format!("could not {what} in {path}: {error}"))?;
		if let Some(parent) = file.parent() {
			fs::create_dir_all(parent).map_err(|error| format!("could not create {}: {error}", parent.display()))?;
		}
		fs::write(&file, changed).map_err(|error| format!("could not write {}: {error}", file.display()))
	}
}

/// Whether each ecosystem's toolchain is installed, probed once.
#[derive(Default)]
struct Toolchains {
	probed: BTreeMap<Ecosystem, Result<(), String>>,
}

impl Toolchains {
	/// `Ok` when `ecosystem`'s tools run here, else why not.
	fn check(&mut self, ecosystem: Ecosystem, latest: bool) -> Result<(), String> {
		self
			.probed
			.entry(ecosystem)
			.or_insert_with(|| ecosystem.probes(latest).into_iter().try_for_each(probe))
			.clone()
	}
}

fn probe(probe: Probe) -> Result<(), String> {
	let cmd = match probe {
		Probe::Runs(cmd) => cmd,
		Probe::Folder(vars) => {
			let set = vars
				.iter()
				.any(|var| std::env::var_os(var).is_some_and(|dir| Path::new(&dir).is_dir()));
			return if set {
				Ok(())
			} else {
				Err(format!("{} is not set to a folder", vars.join(" or ")))
			};
		}
	};
	let Some((program, args)) = cmd.split_first() else {
		return Ok(());
	};
	match Command::new(program)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.status()
	{
		Ok(status) if status.success() => Ok(()),
		Ok(_) => Err(format!("`{}` failed", shell(cmd))),
		Err(_) => Err(format!("`{program}` is not installed")),
	}
}

/// One combination's result for the summary.
struct Outcome {
	label: String,
	result: String,
	time: Duration,
}

impl Outcome {
	fn failed(label: String, time: Duration) -> Self {
		Self {
			label,
			result: "FAIL".to_owned(),
			time,
		}
	}
}

/// Prints the summary table, and adds it to the GitHub Actions job summary when there is one.
fn report(outcomes: &[Outcome]) {
	if outcomes.is_empty() {
		return;
	}
	let width = outcomes
		.iter()
		.map(|outcome| outcome.label.chars().count())
		.max()
		.unwrap_or(0);
	eprintln!("\nSummary:");
	let mut markdown = String::from("| Combination | Result | Time |\n| --- | --- | --- |\n");
	for outcome in outcomes {
		let time = if outcome.time.is_zero() {
			"-".to_owned()
		} else {
			seconds(outcome.time)
		};
		eprintln!("  {:width$}  {:>7}  {}", outcome.label, time, outcome.result);
		let _ = writeln!(markdown, "| {} | {} | {time} |", outcome.label, outcome.result);
	}
	let total: Duration = outcomes.iter().map(|outcome| outcome.time).sum();
	eprintln!("  {} combinations in {}", outcomes.len(), seconds(total));
	if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
		let appended = fs::OpenOptions::new()
			.append(true)
			.create(true)
			.open(summary)
			.and_then(|mut file| std::io::Write::write_all(&mut file, format!("{markdown}\n").as_bytes()));
		if let Err(error) = appended {
			eprintln!("could not write the job summary: {error}");
		}
	}
}

/// The `--list` text: each stack's fixture, detection, commands and combinations, then what is not covered yet.
fn listing(selected: &[&Combination<'_>], toolchains: &mut Toolchains, latest: bool) -> String {
	let commands = |checks: &[Check]| {
		checks
			.iter()
			.map(|check| shell(check.cmd))
			.collect::<Vec<_>>()
			.join("; ")
	};
	let mut out = String::new();
	let mut current = "";
	for combination in selected {
		let stack = combination.stack;
		if stack.name != current {
			current = stack.name;
			let status = match toolchains.check(stack.ecosystem, latest) {
				Ok(()) => String::new(),
				Err(reason) => format!(", skipped here: {reason}"),
			};
			let detects: Vec<String> = stack
				.detects
				.iter()
				.map(|(fact, value)| format!("{fact}={value}"))
				.collect();
			let _ = writeln!(
				out,
				"\n{} ({}{status}) fixtures/stacks/{}",
				stack.name,
				stack.ecosystem.group(),
				stack.name
			);
			let _ = writeln!(out, "  detects  {}", detects.join(" "));
			let _ = writeln!(out, "  checks   {}", commands(stack.checks));
			let _ = writeln!(out, "  tests    {}", commands(stack.tests));
		}
		let first: Vec<String> = combination
			.first
			.iter()
			.map(|(piece, variant)| format!("{}/{} ({variant})", combination.pack, piece.name))
			.collect();
		let after = if first.is_empty() {
			String::new()
		} else {
			format!(", after {}", first.join(", "))
		};
		let _ = writeln!(
			out,
			"  {}/{} ({}{after})",
			combination.pack, combination.piece.name, combination.variant
		);
	}
	let _ = writeln!(
		out,
		"\nNot yet covered:{}",
		if UNCOVERED.is_empty() { " nothing" } else { "" }
	);
	for (pack, variants, reason) in UNCOVERED {
		let _ = writeln!(out, "  {pack}: {} ({reason})", variants.join(", "));
	}
	let _ = writeln!(out, "\n{} combinations", selected.len());
	out
}

/// Copies the fixture at `from` to `to`, leaving out dependency and build folders.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
	fs::create_dir_all(to).map_err(|error| format!("could not create {}: {error}", to.display()))?;
	let entries = fs::read_dir(from).map_err(|error| format!("could not list {}: {error}", from.display()))?;
	for entry in entries {
		let entry = entry.map_err(|error| format!("could not list {}: {error}", from.display()))?;
		let name = entry.file_name();
		if SKIPPED.iter().any(|skipped| name == *skipped) {
			continue;
		}
		let source = entry.path();
		let target = to.join(&name);
		let kind = entry
			.file_type()
			.map_err(|error| format!("could not read {}: {error}", source.display()))?;
		if kind.is_dir() {
			copy_dir(&source, &target)?;
		} else if kind.is_file() {
			fs::copy(&source, &target).map_err(|error| format!("could not copy {}: {error}", source.display()))?;
		} else {
			return Err(format!("{} is neither a file nor a folder", source.display()));
		}
	}
	Ok(())
}

fn read_json(path: &Path) -> Result<Value, String> {
	let text = fs::read_to_string(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
	serde_json::from_str(&text).map_err(|error| format!("{} is not JSON: {error}", path.display()))
}

/// `cmd` as a POSIX shell would take it.
fn shell(cmd: &[&str]) -> String {
	cmd.iter().map(|arg| quote(arg)).collect::<Vec<_>>().join(" ")
}

fn quote(arg: &str) -> String {
	let plain = !arg.is_empty()
		&& arg
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || "@^~.=/:_+,-".contains(c));
	if plain {
		arg.to_owned()
	} else {
		format!("'{}'", arg.replace('\'', r"'\''"))
	}
}

fn seconds(time: Duration) -> String {
	format!("{:.1}s", time.as_secs_f64())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn piece(name: &str, variants: &[&str]) -> Piece {
		Piece {
			name: name.to_owned(),
			target: "src".to_owned(),
			inputs: Map::new(),
			variants: variants
				.iter()
				.map(|variant| ((*variant).to_owned(), "src".to_owned()))
				.collect(),
		}
	}

	#[test]
	fn options_should_take_values_after_or_joined_to_their_flag() -> Result<(), String> {
		let args: Vec<String> = ["--stack", "svelte", "--piece=ui/atom", "--latest"]
			.into_iter()
			.map(str::to_owned)
			.collect();

		let options = Options::parse(&args)?;

		assert_eq!(options.stacks, ["svelte"]);
		assert_eq!(options.pieces, ["ui/atom"]);
		assert!(options.latest);
		Ok(())
	}

	#[test]
	fn options_should_reject_a_flag_without_its_value() {
		assert!(Options::parse(&["--stack".to_owned()]).is_err());
	}

	#[test]
	fn coverage_problems_should_name_variants_no_stack_covers_and_stale_waivers() {
		let packs = [Pack {
			name: "ui".to_owned(),
			pieces: vec![piece("atom", &["flutter", "brand-new"])],
		}];
		let uncovered: &[(&str, &[&str], &str)] = &[("ui", &["flutter", "swiftui"], "no fixture yet")];

		assert_eq!(
			coverage_problems(&packs, &[], uncovered),
			[
				"no stack verifies ui/atom (brand-new)",
				"UNCOVERED lists ui (swiftui), which no piece has"
			]
		);
	}

	#[test]
	fn pubspec_packages_should_leave_out_sdk_packages() {
		let pubspec = "name: app\ndependencies:\n  flutter:\n    sdk: flutter\n  material_ui: ^1.1.0\n\ndev_dependencies:\n  flutter_test:\n    sdk: flutter\n  flutter_lints: ^6.0.0\n\nflutter:\n  uses-material-design: true\n";

		assert_eq!(pubspec_packages(pubspec), ["material_ui", "flutter_lints"]);
	}

	#[test]
	fn quote_should_leave_plain_arguments_alone() {
		assert_eq!(shell(&["bun", "run", "vitest", "run"]), "bun run vitest run");
		assert_eq!(quote("--input=a b"), "'--input=a b'");
	}
}
