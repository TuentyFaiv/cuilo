//! `cuilo upgrade`: replaces a script-installed binary with a release whose signed checksums match,
//! or runs the package manager that installed cuilo.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::{env, fmt, fs, io};

use anyhow::{Context as _, anyhow, bail, ensure};
use cuilo_core::brand;
use cuilo_core::model::Digest;
use cuilo_packs::transport::Fetch;
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::ser::SerializeMap as _;
use serde::{Serialize, Serializer};

use crate::ui::{self, Output};

/// The minisign public key that signs every release's `SHA256SUMS`; empty until the first release.
pub(crate) const RELEASE_KEY: &str = include_str!("../release-key.pub");

/// The target triple this binary was built for, e.g. `aarch64-apple-darwin`.
pub(crate) const TARGET: &str = env!("CUILO_TARGET");

/// The Homebrew formula, in its tap.
const HOMEBREW_FORMULA: &str = concat!("tuentyfaiv/tap/", cuilo_core::brand_name!());

/// The crate `cargo install` and `cargo binstall` install.
const CRATE: &str = env!("CARGO_PKG_NAME");

/// How the running binary was installed, which decides who may replace it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Install {
	/// By the install script; `upgrade` replaces it.
	Script,
	/// Globally, by a JavaScript package manager; `upgrade` runs it.
	Global(Manager),
	/// In the cache of a package manager's one-off runner, such as npx; `upgrade` leaves it alone.
	Ephemeral(Manager),
	/// In a project's `node_modules`; `upgrade` leaves it to the project.
	Project {
		/// The package manager the project's lockfile names.
		manager: Manager,
		/// The project folder.
		root: String,
	},
	/// By Homebrew.
	Homebrew,
	/// By `cargo install` or `cargo binstall`.
	Cargo,
	/// Some other way, e.g. a binary copied by hand.
	Unknown,
}

/// A JavaScript package manager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Manager {
	/// npm, with npx.
	Npm,
	/// pnpm, with `pnpm dlx`.
	Pnpm,
	/// Bun, with bunx.
	Bun,
	/// Yarn: `yarn global` is Yarn 1's, `yarn dlx` Yarn 2 and later's.
	Yarn,
}

impl Manager {
	/// Its name in messages, e.g. `Bun`.
	fn name(self) -> &'static str {
		match self {
			Self::Npm => "npm",
			Self::Pnpm => "pnpm",
			Self::Bun => "Bun",
			Self::Yarn => "Yarn",
		}
	}

	/// Its one-off runner, e.g. `pnpm dlx`.
	fn runner(self) -> &'static str {
		match self {
			Self::Npm => "npx",
			Self::Pnpm => "pnpm dlx",
			Self::Bun => "bunx",
			Self::Yarn => "yarn dlx",
		}
	}
}

/// Who owns a copy of cuilo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
	/// The user, through a global install that `upgrade` upgrades.
	Global,
	/// A one-off runner, such as npx, that keeps it in its cache.
	Ephemeral,
	/// A project, as one of its dependencies.
	Project,
}

/// The lockfiles that name a project's package manager, in the order they are looked for.
const LOCKFILES: [(&str, Manager); 6] = [
	("bun.lock", Manager::Bun),
	("bun.lockb", Manager::Bun),
	("pnpm-lock.yaml", Manager::Pnpm),
	("yarn.lock", Manager::Yarn),
	("package-lock.json", Manager::Npm),
	("npm-shrinkwrap.json", Manager::Npm),
];

impl Install {
	/// How the binary at `exe` was installed.
	pub(crate) fn of(exe: &Path) -> Self {
		if exe.with_file_name(brand::INSTALL_RECEIPT).is_file() {
			return Self::Script;
		}
		let search: Vec<String> = env::var_os("PATH")
			.map(|path| {
				env::split_paths(&path)
					.map(|dir| dir.to_string_lossy().into_owned())
					.collect()
			})
			.unwrap_or_default();
		Self::judge(&exe.to_string_lossy(), &search, &|file| Path::new(file).is_file())
	}

	/// How a binary at `path` was installed, judged by its folder names, the files `exists` finds
	/// beside them, and the `PATH` folders in `search`. A copy in `node_modules` is ephemeral in a
	/// one-off runner's cache, global in a package manager's global folder or in an npm prefix, which
	/// has no `package.json`, and otherwise the dependency of a project.
	fn judge(path: &str, search: &[String], exists: &dyn Fn(&str) -> bool) -> Self {
		let lower = path.to_ascii_lowercase();
		let folders = Folders::new(path, &lower);
		if !folders.has("node_modules") {
			return if folders.has("cellar") || folders.has("homebrew") || folders.has("linuxbrew") {
				Self::Homebrew
			} else if matches!(folders.names.as_slice(), [.., ".cargo", "bin", _]) {
				Self::Cargo
			} else {
				Self::Unknown
			};
		}
		if let Some(manager) = folders.runner() {
			return Self::Ephemeral(manager);
		}
		if folders.in_pnpm_store() {
			// pnpm 11 keeps the copies of `pnpm add --global` and `pnpm dlx` in the same store, so only
			// the `PATH` folder `pnpm dlx` adds tells them apart.
			let dlx = search
				.iter()
				.any(|dir| Folders::new(dir, &dir.to_ascii_lowercase()).is_pnpm_dlx_bin());
			return if dlx {
				Self::Ephemeral(Manager::Pnpm)
			} else {
				Self::Global(Manager::Pnpm)
			};
		}
		if let Some(manager) = folders.global() {
			return Self::Global(manager);
		}
		let Some((root, separator)) = folders.project() else {
			return Self::Unknown;
		};
		let file = |name: &str| format!("{root}{separator}{name}");
		if !exists(&file("package.json")) {
			return Self::Global(Manager::Npm);
		}
		let manager = LOCKFILES
			.iter()
			.find(|(name, _)| exists(&file(name)))
			.map(|&(_, manager)| manager)
			.or_else(|| folders.layout())
			.unwrap_or(Manager::Npm);
		Self::Project {
			manager,
			root: shown(root).to_owned(),
		}
	}

	/// Who owns this copy; `None` when unknown.
	pub(crate) fn kind(&self) -> Option<Kind> {
		match self {
			Self::Script | Self::Global(_) | Self::Homebrew | Self::Cargo => Some(Kind::Global),
			Self::Ephemeral(_) => Some(Kind::Ephemeral),
			Self::Project { .. } => Some(Kind::Project),
			Self::Unknown => None,
		}
	}

	/// Its name in `--json` output, e.g. `pnpm` or `script`.
	fn id(&self) -> &'static str {
		match self {
			Self::Script => "script",
			Self::Global(manager) | Self::Ephemeral(manager) | Self::Project { manager, .. } => match manager {
				Manager::Npm => "npm",
				Manager::Pnpm => "pnpm",
				Manager::Bun => "bun",
				Manager::Yarn => "yarn",
			},
			Self::Homebrew => "homebrew",
			Self::Cargo => "cargo",
			Self::Unknown => "unknown",
		}
	}

	/// The name of the package manager that `upgrade` runs, e.g. `npm`; `None` when it runs none.
	pub(crate) fn manager(&self) -> Option<&'static str> {
		match self {
			Self::Global(manager) => Some(manager.name()),
			Self::Homebrew => Some("Homebrew"),
			Self::Cargo => Some("cargo"),
			Self::Script | Self::Ephemeral(_) | Self::Project { .. } | Self::Unknown => None,
		}
	}

	/// The command that puts `version` in place of this copy, or `None` for script and unknown
	/// installs: for a global install the one `upgrade` runs, otherwise the one to run yourself.
	/// Homebrew cannot pin a version, so it moves to its formula's. A one-off runner gets the dist-tag
	/// when `version` is the `newest` release. `binstall` says whether `cargo-binstall` is on `PATH`.
	pub(crate) fn command(&self, version: &Version, newest: bool, binstall: bool) -> Option<Invocation> {
		let package = format!("{}@{version}", brand::NPM_PACKAGE);
		let tag = if version.pre.is_empty() { "latest" } else { "next" };
		let run = if newest {
			format!("{}@{tag}", brand::NPM_PACKAGE)
		} else {
			package.clone()
		};
		let version = version.to_string();
		let (program, args): (&str, Vec<&str>) = match self {
			Self::Global(Manager::Npm) => ("npm", vec!["install", "--global", &package]),
			Self::Global(Manager::Pnpm) => ("pnpm", vec!["add", "--global", &package]),
			Self::Global(Manager::Bun) => ("bun", vec!["add", "--global", &package]),
			Self::Global(Manager::Yarn) => ("yarn", vec!["global", "add", &package]),
			Self::Ephemeral(Manager::Npm) => ("npx", vec![&run]),
			Self::Ephemeral(Manager::Pnpm) => ("pnpm", vec!["dlx", &run]),
			Self::Ephemeral(Manager::Bun) => ("bunx", vec![&run]),
			Self::Ephemeral(Manager::Yarn) => ("yarn", vec!["dlx", &run]),
			Self::Project { manager, .. } => match manager {
				Manager::Npm => ("npm", vec!["install", "--save-dev", &package]),
				Manager::Pnpm => ("pnpm", vec!["add", "--save-dev", &package]),
				Manager::Bun => ("bun", vec!["add", "--dev", &package]),
				Manager::Yarn => ("yarn", vec!["add", "--dev", &package]),
			},
			Self::Homebrew => ("brew", vec!["upgrade", HOMEBREW_FORMULA]),
			Self::Cargo if binstall => ("cargo", vec!["binstall", CRATE, "--version", &version, "--no-confirm"]),
			Self::Cargo => ("cargo", vec!["install", CRATE, "--locked", "--version", &version]),
			Self::Script | Self::Unknown => return None,
		};
		Some(Invocation {
			program: program.to_owned(),
			args: args.into_iter().map(str::to_owned).collect(),
		})
	}

	/// What to do to move this copy to `version`, for messages; `newest` says whether it is the newest
	/// release.
	pub(crate) fn hint(&self, version: &Version, newest: bool) -> String {
		match (self, self.command(version, newest, false)) {
			(Self::Unknown, _) => format!(
				"{} did not install this copy; reinstall with the install script at {}/install.sh",
				brand::NAME,
				brand::DOWNLOAD_URL
			),
			(Self::Ephemeral(manager), Some(command)) => {
				format!("this copy runs through {}; run `{command}`", manager.runner())
			}
			(Self::Project { root, .. }, Some(command)) => format!(
				"this copy is a dependency of the project in {}; run `{command}` there",
				ui::printable_line(root)
			),
			_ => format!("run `{} upgrade`", brand::BINARY_NAME),
		}
	}
}

/// `install`, `kind` and `project` in `--json` output.
impl Serialize for Install {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		let project = match self {
			Self::Project { root, .. } => Some(root),
			_ => None,
		};
		let mut map = serializer.serialize_map(Some(3))?;
		map.serialize_entry("install", self.id())?;
		map.serialize_entry("kind", &self.kind())?;
		map.serialize_entry("project", &project)?;
		map.end()
	}
}

/// A binary's path split into folder names, which `/` and `\` both separate, so one platform can
/// judge another's paths.
#[derive(Debug)]
struct Folders<'a> {
	/// The path as given.
	path: &'a str,
	/// The names, lowercased.
	names: Vec<&'a str>,
	/// The byte offset in `path` where each name starts.
	starts: Vec<usize>,
}

impl<'a> Folders<'a> {
	/// The folders of `path`, whose lowercase copy is `lower`.
	fn new(path: &'a str, lower: &'a str) -> Self {
		let mut names = Vec::new();
		let mut starts = Vec::new();
		let mut start = 0;
		for (index, c) in lower.char_indices().chain([(lower.len(), '/')]) {
			if matches!(c, '/' | '\\') {
				if let Some(name) = lower.get(start..index).filter(|name| !name.is_empty()) {
					names.push(name);
					starts.push(start);
				}
				start = index + 1;
			}
		}
		Self { path, names, starts }
	}

	/// Whether a folder is called `name`.
	fn has(&self, name: &str) -> bool {
		self.names.contains(&name)
	}

	/// Whether `size` names in a row pass `test`.
	fn any(&self, size: usize, test: impl Fn(&[&str]) -> bool) -> bool {
		self.names.windows(size).any(test)
	}

	/// The package manager whose one-off runner keeps this copy in its cache: npx's `_npx/<hash>`,
	/// bunx's `bunx-<uid>-<package>` or, since Bun 1.4 outside Windows, `.bunx-<uid>/<package>`,
	/// pnpm's `dlx/<key>/<dir>/node_modules`, or Yarn's `xfs-<id>/dlx-<pid>`.
	fn runner(&self) -> Option<Manager> {
		if self.has("_npx") {
			Some(Manager::Npm)
		} else if self.names.iter().any(|name| bunx(name)) {
			Some(Manager::Bun)
		} else if self.any(4, |names| matches!(names, ["dlx", _, _, "node_modules"])) {
			Some(Manager::Pnpm)
		} else if self.any(
			2,
			|names| matches!(names, [temp, dlx] if temp.starts_with("xfs-") && dlx.starts_with("dlx-")),
		) {
			Some(Manager::Yarn)
		} else {
			None
		}
	}

	/// Whether this copy is in pnpm's global virtual store,
	/// `<store>/v11/links/@<scope>/<name>/<version>/<hash>/node_modules`.
	fn in_pnpm_store(&self) -> bool {
		self.any(7, |names| {
			matches!(names, [version, "links", scope, _, _, _, "node_modules"] if store_version(version) && scope.starts_with('@'))
		})
	}

	/// Whether this is the `node_modules/.bin` folder `pnpm dlx` adds to `PATH`,
	/// `<cache>/dlx/<key>/<dir>/node_modules/.bin`.
	fn is_pnpm_dlx_bin(&self) -> bool {
		matches!(self.names.as_slice(), [.., "dlx", _, _, "node_modules", ".bin"])
	}

	/// The package manager whose global folder holds this copy: Bun's `install/global`, pnpm's
	/// `global/5` or, since pnpm 11, `global/v11/<hash>`, or Yarn 1's `yarn/global` or
	/// `Yarn/Data/global`.
	fn global(&self) -> Option<Manager> {
		if self.any(3, |names| names == ["install", "global", "node_modules"]) {
			Some(Manager::Bun)
		} else if self.any(4, |names| {
			matches!(names, ["global", layout, "node_modules", _] | ["global", layout, _, "node_modules"] if pnpm_layout(layout))
		}) {
			Some(Manager::Pnpm)
		} else if self.any(3, |names| names == ["yarn", "global", "node_modules"])
			|| self.any(4, |names| names == ["yarn", "data", "global", "node_modules"])
		{
			Some(Manager::Yarn)
		} else {
			None
		}
	}

	/// The project folder holding this copy, and the separator after it: the folder above `.yarn`
	/// for a package Yarn unplugged, else the one holding the first `node_modules`.
	fn project(&self) -> Option<(&'a str, char)> {
		let index = self
			.names
			.windows(2)
			.position(|names| names == [".yarn", "unplugged"])
			.or_else(|| self.names.iter().position(|name| *name == "node_modules"))?;
		let before = self.path.get(..*self.starts.get(index)?)?;
		let separator = before.chars().next_back()?;
		Some((before.strip_suffix(separator)?, separator))
	}

	/// The package manager a project's `node_modules` layout points to: pnpm's `.pnpm`, Bun's
	/// `.bun`, Yarn's `.yarn`.
	fn layout(&self) -> Option<Manager> {
		if self.has(".pnpm") {
			Some(Manager::Pnpm)
		} else if self.has(".bun") {
			Some(Manager::Bun)
		} else if self.has(".yarn") {
			Some(Manager::Yarn)
		} else {
			None
		}
	}
}

/// Whether `name` is a folder bunx installs into: `bunx-<uid>-<package>`, or `.bunx-<uid>`.
fn bunx(name: &str) -> bool {
	name
		.strip_prefix('.')
		.unwrap_or(name)
		.strip_prefix("bunx-")
		.is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// Whether `name` is the layout folder of pnpm's global folder: `5` before pnpm 11, `v11` since.
fn pnpm_layout(name: &str) -> bool {
	let digits = name.strip_prefix('v').unwrap_or(name);
	!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether `name` is a pnpm store's version folder, e.g. `v11`.
fn store_version(name: &str) -> bool {
	name.starts_with('v') && pnpm_layout(name)
}

/// `path` without the `\\?\` that a resolved Windows path starts with, when a drive letter follows.
fn shown(path: &str) -> &str {
	path
		.strip_prefix(r"\\?\")
		.filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
		.unwrap_or(path)
}

/// A package-manager command: a program and fixed arguments, never a shell string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Invocation {
	/// The program as typed in a shell, e.g. `npm`.
	pub(crate) program: String,
	/// Its arguments.
	pub(crate) args: Vec<String>,
}

impl fmt::Display for Invocation {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.program)?;
		self.args.iter().try_for_each(|arg| write!(f, " {arg}"))
	}
}

/// Runs package-manager commands; a trait so tests can record them instead.
pub(crate) trait Runner {
	/// Whether `program` is on `PATH`.
	fn finds(&self, program: &str) -> bool;
	/// Runs `invocation` to the end; an error unless it exits with 0.
	fn run(&self, invocation: &Invocation) -> anyhow::Result<()>;
}

/// Runs commands in this terminal, printing each first; with `json`, their stdout goes to stderr.
#[derive(Debug)]
pub(crate) struct System {
	/// Whether stdout carries JSON.
	pub(crate) json: bool,
}

impl Runner for System {
	fn finds(&self, program: &str) -> bool {
		let extensions = extensions(cfg!(windows), env::var_os("PATHEXT").as_deref());
		find(program, env::var_os("PATH").as_deref(), &extensions).is_some()
	}

	fn run(&self, invocation: &Invocation) -> anyhow::Result<()> {
		eprintln!("$ {invocation}");
		let program = if cfg!(windows) {
			let extensions = extensions(true, env::var_os("PATHEXT").as_deref());
			find(&invocation.program, env::var_os("PATH").as_deref(), &extensions)
				.unwrap_or_else(|| PathBuf::from(&invocation.program))
		} else {
			PathBuf::from(&invocation.program)
		};
		let mut command = std::process::Command::new(program);
		command.args(&invocation.args);
		if self.json {
			command.stdout(io::stderr());
		}
		let status = command
			.status()
			.with_context(|| format!("could not run `{invocation}`"))?;
		ensure!(status.success(), "`{invocation}` failed ({status})");
		Ok(())
	}
}

/// The file extensions a program may have: on Windows the runnable ones `PATHEXT` lists, in its
/// order, so `npm` finds the `npm.cmd` shim; elsewhere none.
fn extensions(windows: bool, pathext: Option<&OsStr>) -> Vec<String> {
	const RUNNABLE: [&str; 4] = [".com", ".exe", ".bat", ".cmd"];
	if !windows {
		return vec![String::new()];
	}
	let listed: Vec<String> = pathext
		.and_then(OsStr::to_str)
		.unwrap_or_default()
		.split(';')
		.map(|extension| extension.trim().to_ascii_lowercase())
		.filter(|extension| RUNNABLE.contains(&extension.as_str()))
		.collect();
	if listed.is_empty() {
		RUNNABLE.map(str::to_owned).to_vec()
	} else {
		listed
	}
}

/// The first file named `program` plus one of `extensions` in the absolute folders of `path`.
fn find(program: &str, path: Option<&OsStr>, extensions: &[String]) -> Option<PathBuf> {
	env::split_paths(path?).filter(|dir| dir.is_absolute()).find_map(|dir| {
		extensions
			.iter()
			.map(|extension| dir.join(format!("{program}{extension}")))
			.find(|file| file.is_file())
	})
}

/// The running binary, with symbolic links resolved.
pub(crate) fn current_exe() -> io::Result<PathBuf> {
	let exe = env::current_exe()?;
	Ok(exe.canonicalize().unwrap_or(exe))
}

/// A release whose `SHA256SUMS` carries a valid signature by the release key.
#[derive(Debug)]
pub(crate) struct Release {
	/// Its version, taken from the signed trusted comment.
	pub(crate) version: Version,
	/// SHA-256 hex digest of each file.
	sums: BTreeMap<String, String>,
}

impl Release {
	/// Checks `signature` over `sums` with `key`, and that it names `expected` when given.
	pub(crate) fn verify(key: &str, sums: &[u8], signature: &str, expected: Option<&Version>) -> anyhow::Result<Self> {
		let key = key.trim();
		ensure!(
			!key.is_empty(),
			"this build has no release key, so it cannot verify an upgrade; reinstall with the install script at {}/install.sh",
			brand::DOWNLOAD_URL
		);
		let public =
			PublicKey::from_base64(key).map_err(|_| anyhow!("the release key in this build is not a minisign key"))?;
		let decoded =
			Signature::decode(signature).map_err(|_| anyhow!("`SHA256SUMS.minisig` is not a minisign signature"))?;
		public
			.verify(sums, &decoded, false)
			.map_err(|_| anyhow!("`SHA256SUMS` is not signed by the cuilo release key"))?;
		let comment = decoded.trusted_comment();
		let version = comment
			.strip_prefix(concat!(cuilo_core::brand_name!(), " "))
			.and_then(|version| Version::parse(version.trim()).ok())
			.ok_or_else(|| anyhow!("the signed comment `{comment}` does not name a cuilo version"))?;
		if let Some(expected) = expected {
			ensure!(
				&version == expected,
				"the release for {expected} is signed as {version}"
			);
		}
		let text = std::str::from_utf8(sums).context("`SHA256SUMS` is not UTF-8")?;
		let sums = text
			.lines()
			.filter(|line| !line.trim().is_empty())
			.map(|line| {
				let (digest, name) = line
					.split_once("  ")
					.ok_or_else(|| anyhow!("`SHA256SUMS` has a malformed line: {line}"))?;
				Ok((name.trim_start_matches('*').to_owned(), digest.to_ascii_lowercase()))
			})
			.collect::<anyhow::Result<_>>()?;
		Ok(Self { version, sums })
	}

	/// `bytes` when their digest is the one listed for `name`.
	pub(crate) fn check<'a>(&self, name: &str, bytes: &'a [u8]) -> anyhow::Result<&'a [u8]> {
		let expected = self
			.sums
			.get(name)
			.ok_or_else(|| anyhow!("release {} has no `{name}`", self.version))?;
		let actual = Digest::of(bytes).hex();
		ensure!(
			&actual == expected,
			"`{name}` does not match its signed checksum (expected {expected}, got {actual})"
		);
		Ok(bytes)
	}
}

/// The release binary for this target, e.g. `cuilo-aarch64-apple-darwin`.
pub(crate) fn binary_name() -> String {
	format!("{}-{TARGET}{}", brand::BINARY_NAME, std::env::consts::EXE_SUFFIX)
}

/// The newest release on the stable channel, or the pre-release one with `pre`; only a hint, since
/// the signed `SHA256SUMS` names the version installed.
pub(crate) fn latest(fetch: &dyn Fetch, pre: bool) -> anyhow::Result<Version> {
	let name = if pre { "latest-pre" } else { "latest" };
	let body = fetch.get(&format!("{}/{name}", brand::DOWNLOAD_URL), &[])?;
	channel(&body).ok_or_else(|| anyhow!("`{}/{name}` does not name a version", brand::DOWNLOAD_URL))
}

/// The version a channel file names on its first line.
fn channel(body: &[u8]) -> Option<Version> {
	let text = std::str::from_utf8(body.get(..body.len().min(256))?).ok()?;
	Version::parse(text.lines().next()?.trim()).ok()
}

/// The folder holding every file of `version`.
fn release_url(version: &Version) -> String {
	format!("{}/v{version}", brand::DOWNLOAD_URL)
}

/// Downloads `version`'s `SHA256SUMS` and checks its signature with `key`.
pub(crate) fn release(fetch: &dyn Fetch, key: &str, version: &Version) -> anyhow::Result<Release> {
	let base = release_url(version);
	let sums = fetch.get(&format!("{base}/SHA256SUMS"), &[])?;
	let signature = fetch.get(&format!("{base}/SHA256SUMS.minisig"), &[])?;
	let signature = String::from_utf8(signature).context("`SHA256SUMS.minisig` is not UTF-8")?;
	Release::verify(key, &sums, &signature, Some(version))
}

/// Downloads this target's binary of `release`, checked against its signed digest.
pub(crate) fn download(fetch: &dyn Fetch, release: &Release) -> anyhow::Result<Vec<u8>> {
	let name = binary_name();
	let bytes = fetch.get(&format!("{}/{name}", release_url(&release.version)), &[])?;
	release.check(&name, &bytes)?;
	Ok(bytes)
}

/// Replaces the binary at `exe` with `bytes`, keeping the old one if anything fails.
pub(crate) fn replace(exe: &Path, bytes: &[u8], version: &Version) -> anyhow::Result<()> {
	let staged = sibling(exe, &format!("new-{}", std::process::id()));
	fs::write(&staged, bytes).with_context(|| format!("could not write {}", staged.display()))?;
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt as _;
		fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
	}
	if let Err(error) = swap(exe, &staged) {
		let _ = fs::remove_file(&staged);
		return Err(error).with_context(|| format!("could not replace {}", exe.display()));
	}
	let receipt = exe.with_file_name(brand::INSTALL_RECEIPT);
	let record = serde_json::json!({ "installer": "upgrade", "version": version.to_string(), "target": TARGET });
	fs::write(&receipt, format!("{record:#}\n")).with_context(|| format!("could not write {}", receipt.display()))?;
	Ok(())
}

/// A file beside `exe`, e.g. `.cuilo.new`.
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
	let name = exe
		.file_name()
		.map_or_else(Default::default, |name| name.to_string_lossy().into_owned());
	exe.with_file_name(format!(".{name}.{suffix}"))
}

#[cfg(not(windows))]
fn swap(exe: &Path, staged: &Path) -> std::io::Result<()> {
	fs::rename(staged, exe)
}

/// Windows cannot overwrite a running executable, but it can rename it out of the way.
#[cfg(windows)]
fn swap(exe: &Path, staged: &Path) -> std::io::Result<()> {
	let old = sibling(exe, "old");
	let _ = fs::remove_file(&old);
	fs::rename(exe, &old)?;
	if let Err(error) = fs::rename(staged, exe) {
		let _ = fs::rename(&old, exe);
		return Err(error);
	}
	Ok(())
}

/// What `upgrade` found or did, for `--json`.
#[derive(Debug, Serialize)]
pub(crate) struct Outcome {
	/// The running version.
	pub(crate) current: Version,
	/// The version offered.
	pub(crate) available: Version,
	/// How the binary was installed: `install` names the installer, e.g. `npm`, `kind` who owns the
	/// copy (`global`, `ephemeral` or `project`, or `null` when unknown), and `project` the project
	/// folder of a project's copy.
	#[serde(flatten)]
	pub(crate) install: Install,
	/// Whether this copy was upgraded.
	pub(crate) upgraded: bool,
	/// The command that moves this copy to `available`: for a global install the package-manager
	/// command that ran, or with `--check` the one that would run; for other copies the one to run
	/// yourself.
	pub(crate) command: Option<String>,
}

/// Runs `cuilo upgrade`: replaces a script install itself, runs the package manager for other global
/// installs, and only reports the command for a one-off runner's or a project's copy.
pub(crate) fn run(
	fetch: &dyn Fetch,
	exe: &Path,
	check: bool,
	wanted: Option<&Version>,
	runner: &dyn Runner,
) -> anyhow::Result<Outcome> {
	let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
	let install = Install::of(exe);
	if !check && install == Install::Unknown {
		bail!(
			"this cuilo cannot upgrade itself: {}",
			install.hint(&current, wanted.is_none())
		);
	}
	if !check && install == Install::Homebrew && wanted.is_some() {
		bail!(
			"Homebrew cannot install a chosen version, so `--version` does not work for this copy; run `{} upgrade` without it",
			brand::BINARY_NAME
		);
	}
	let target = match wanted {
		Some(version) => version.clone(),
		None => latest(fetch, !current.pre.is_empty())?,
	};
	let mut outcome = Outcome {
		current: current.clone(),
		available: target.clone(),
		install,
		upgraded: false,
		command: None,
	};
	if (wanted.is_none() && target <= current) || wanted == Some(&current) {
		return Ok(outcome);
	}
	let binstall = outcome.install == Install::Cargo && runner.finds("cargo-binstall");
	let command = outcome.install.command(&target, wanted.is_none(), binstall);
	outcome.command = command.as_ref().map(ToString::to_string);
	if check || outcome.install.kind() != Some(Kind::Global) {
		return Ok(outcome);
	}
	if let Some(command) = command {
		runner.run(&command)?;
	} else {
		let release = release(fetch, RELEASE_KEY, &target)?;
		let bytes = download(fetch, &release)?;
		replace(exe, &bytes, &release.version)?;
	}
	outcome.upgraded = true;
	Ok(outcome)
}

/// `cuilo upgrade [--check] [--version <v>]`, printing its outcome as text or, with `json`, as JSON.
pub(crate) fn command(json: bool, check: bool, wanted: Option<&Version>) -> anyhow::Result<()> {
	let fetch = cuilo_packs::transport::HttpsFetch::default();
	let outcome = run(&fetch, &current_exe()?, check, wanted, &System { json })?;
	if json {
		Output::json(&outcome);
		return Ok(());
	}
	let name = brand::NAME;
	let Outcome {
		current,
		available,
		install,
		upgraded,
		command,
	} = &outcome;
	if *upgraded {
		match install.manager() {
			Some(_) if *install == Install::Homebrew => println!("Upgraded {name} with Homebrew."),
			Some(manager) => println!("Upgraded {name} {current} → {available} with {manager}."),
			None => println!("Upgraded {name} {current} → {available}."),
		}
	} else if available == current {
		println!("{name} {current} is the newest release.");
	} else if available > current || command.is_some() {
		let hint = install.hint(available, wanted.is_none());
		let how = match command {
			Some(command) if install.kind() == Some(Kind::Global) => format!("{hint}, which runs `{command}`"),
			_ => hint,
		};
		println!("{name} {available} is available (you have {current}); {how}.");
	} else {
		println!("{name} {current} is newer than the newest release, {available}.");
	}
	Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
	use std::cell::RefCell;
	use std::sync::Mutex;

	use cuilo_packs::transport::{FetchError, Header};

	use super::*;

	/// A throwaway key pair made for these tests; it signs nothing real.
	pub(crate) const KEY: &str = "RWRRuwa0l7u2Rh4gptzRQofBRURbNgjE8e/Ad6WYt+uvshv/s6OB9zVY";
	const OTHER_KEY: &str = "RWSSBRIxu9J58X+jmq/hTBpD4KnRr1oz2tt5M4n2tZ5+85grXHGK5v/C";
	const SIGNATURE: &str = "untrusted comment: signature from rsign secret key\nRURRuwa0l7u2RplKzWxjYG8787O3MPww0rSWlUrVhV/YTCGD6ioxkKbrtrKitElsYdvbogVtLDlrmNvK+RWHz8gBHVHgazDLmQQ=\ntrusted comment: cuilo 99.0.0\n89TAIyDABi/kDoaz0EP1P3SNtC8iEDhKnbYuM+TtNM2SLfWZCpza1RV4vZ/F3y5n6OBmC6FrCXn9BnZN/E3LCQ==\n";
	/// `SHA256SUMS` for version 99.0.0, listing [`BINARY`] for every release target.
	const SUMS: &str = "d323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-apple-darwin\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-apple-darwin\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-unknown-linux-gnu\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-unknown-linux-gnu\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-unknown-linux-musl\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-unknown-linux-musl\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-pc-windows-msvc.exe\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-pc-windows-msvc.exe\n";
	pub(crate) const BINARY: &[u8] = b"new cuilo\n";

	#[test]
	fn release_key_should_be_a_minisign_public_key() {
		assert!(PublicKey::from_base64(RELEASE_KEY.trim()).is_ok(), "{RELEASE_KEY}");
	}

	pub(crate) fn version(text: &str) -> Version {
		Version::parse(text).unwrap_or_else(|_| Version::new(0, 0, 0))
	}

	/// Serves fixed bodies by URL suffix and records what was asked.
	#[derive(Debug, Default)]
	pub(crate) struct Served {
		pub(crate) bodies: Vec<(String, Vec<u8>)>,
		pub(crate) asked: Mutex<Vec<String>>,
	}

	impl Fetch for Served {
		fn get(&self, url: &str, _headers: &[Header]) -> Result<Vec<u8>, FetchError> {
			if let Ok(mut asked) = self.asked.lock() {
				asked.push(url.to_owned());
			}
			self
				.bodies
				.iter()
				.find(|(suffix, _)| url.ends_with(suffix))
				.map(|(_, body)| body.clone())
				.ok_or_else(|| FetchError {
					url: url.to_owned(),
					reason: "the server answered HTTP 404".to_owned(),
				})
		}
	}

	/// Serves release 99.0.0, signed by [`KEY`], with `binary` as this target's binary.
	pub(crate) fn release_server(binary: &[u8]) -> Served {
		Served {
			bodies: vec![
				("/v99.0.0/SHA256SUMS".to_owned(), SUMS.as_bytes().to_vec()),
				("/v99.0.0/SHA256SUMS.minisig".to_owned(), SIGNATURE.as_bytes().to_vec()),
				(format!("/v99.0.0/{}", binary_name()), binary.to_vec()),
			],
			asked: Mutex::default(),
		}
	}

	#[test]
	fn verify_should_read_the_version_from_the_signed_comment() -> anyhow::Result<()> {
		let release = Release::verify(KEY, SUMS.as_bytes(), SIGNATURE, None)?;

		assert_eq!(release.version, version("99.0.0"));
		assert!(release.check(&binary_name(), BINARY).is_ok());
		Ok(())
	}

	#[test]
	fn verify_should_refuse_other_keys_tampered_sums_and_a_missing_key() {
		let tampered = SUMS.replacen('6', "7", 1);

		let errors = [
			Release::verify(OTHER_KEY, SUMS.as_bytes(), SIGNATURE, None),
			Release::verify(KEY, tampered.as_bytes(), SIGNATURE, None),
			Release::verify("", SUMS.as_bytes(), SIGNATURE, None),
			Release::verify(KEY, SUMS.as_bytes(), SIGNATURE, Some(&version("98.0.0"))),
		]
		.map(|result| result.err().map(|error| error.to_string()).unwrap_or_default());

		assert!(
			errors[0].contains("not signed by the cuilo release key"),
			"{}",
			errors[0]
		);
		assert!(
			errors[1].contains("not signed by the cuilo release key"),
			"{}",
			errors[1]
		);
		assert!(errors[2].contains("no release key"), "{}", errors[2]);
		assert!(errors[3].contains("signed as 99.0.0"), "{}", errors[3]);
	}

	#[test]
	fn check_should_refuse_a_binary_that_does_not_match() -> anyhow::Result<()> {
		let release = Release::verify(KEY, SUMS.as_bytes(), SIGNATURE, None)?;

		let error = release
			.check(&binary_name(), b"evil")
			.err()
			.map(|error| error.to_string());

		assert!(error.is_some_and(|error| error.contains("does not match its signed checksum")));
		Ok(())
	}

	#[test]
	fn channel_should_read_one_version_and_refuse_anything_else() {
		assert_eq!(channel(b"2.1.0-beta.1\n"), Some(version("2.1.0-beta.1")));
		assert_eq!(channel(b" 2.0.0 \r\n"), Some(version("2.0.0")));
		assert_eq!(channel(b"<html>not found</html>"), None);
		assert_eq!(channel(b""), None);
		assert_eq!(channel(&[0xff, 0xfe]), None);
	}

	/// Records the commands it is asked to run, finding only `found` on `PATH`.
	#[derive(Debug, Default)]
	struct Recorder {
		found: Vec<&'static str>,
		ran: RefCell<Vec<String>>,
		fails: bool,
	}

	impl Runner for Recorder {
		fn finds(&self, program: &str) -> bool {
			self.found.contains(&program)
		}

		fn run(&self, invocation: &Invocation) -> anyhow::Result<()> {
			self.ran.borrow_mut().push(invocation.to_string());
			ensure!(!self.fails, "`{invocation}` failed (exit status: 1)");
			Ok(())
		}
	}

	fn channels(version: &str) -> Served {
		Served {
			bodies: vec![
				("/latest".to_owned(), format!("{version}\n").into_bytes()),
				("/latest-pre".to_owned(), format!("{version}\n").into_bytes()),
			],
			asked: Mutex::default(),
		}
	}

	fn asked(fetch: &Served) -> usize {
		fetch.asked.lock().map_or(usize::MAX, |asked| asked.len())
	}

	#[test]
	fn run_should_never_go_down_to_an_older_latest() -> anyhow::Result<()> {
		let fetch = channels("0.0.1");
		let dir = tempfile::tempdir()?;
		let exe = dir.path().join("cuilo");
		fs::write(&exe, "current")?;
		fs::write(exe.with_file_name(brand::INSTALL_RECEIPT), "{}")?;

		let outcome = run(&fetch, &exe, false, None, &Recorder::default())?;

		assert!(!outcome.upgraded);
		assert_eq!(fs::read(&exe)?, b"current");
		assert_eq!(asked(&fetch), 1, "it downloaded more than the channel");
		Ok(())
	}

	/// How a copy at `path` was installed, where the files in `files` exist and `PATH` is empty.
	fn judged(path: &str, files: &[&str]) -> Install {
		Install::judge(path, &[], &|file| files.contains(&file))
	}

	#[test]
	fn install_should_follow_the_receipt_the_project_files_and_known_locations() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let exe = dir.path().join("bin/cuilo");
		fs::create_dir_all(dir.path().join("bin"))?;
		let local = dir.path().join("node_modules/@cuilo/cli-darwin-arm64/bin/cuilo");

		assert_eq!(Install::of(&exe), Install::Unknown);
		fs::write(exe.with_file_name(brand::INSTALL_RECEIPT), "{}")?;
		assert_eq!(Install::of(&exe), Install::Script);
		assert_eq!(
			Install::of(&local),
			Install::Global(Manager::Npm),
			"a folder without package.json is an npm prefix"
		);
		fs::write(dir.path().join("package.json"), "{}")?;
		fs::write(dir.path().join("pnpm-lock.yaml"), "")?;
		assert_eq!(
			Install::of(&local),
			Install::Project {
				manager: Manager::Pnpm,
				root: dir.path().to_string_lossy().into_owned(),
			}
		);
		assert_eq!(
			Install::of(Path::new("/opt/homebrew/Cellar/cuilo/2.0.0/bin/cuilo")),
			Install::Homebrew
		);
		assert_eq!(Install::of(Path::new("/home/me/.cargo/bin/cuilo")), Install::Cargo);
		Ok(())
	}

	#[test]
	fn install_should_find_npm_prefixes_on_every_platform() {
		let prefixes = [
			"/opt/homebrew/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
			"/usr/local/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-darwin-x64/bin/cuilo",
			"/Users/me/.nvm/versions/node/v24.11.1/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
			"/Users/me/.local/share/fnm/node-versions/v26.13.0/installation/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
			"/Users/me/.volta/tools/image/packages/@cuilo/cli/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-darwin-x64/bin/cuilo",
			"/home/me/.nvm/versions/node/v24.11.1/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
			"/home/me/.local/share/mise/installs/node/24.11.1/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-linux-x64-musl/bin/cuilo",
			"/usr/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-linux-arm64/bin/cuilo",
			r"C:\Users\me\AppData\Roaming\npm\node_modules\@cuilo\cli\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
			r"C:\Users\me\AppData\Local\nvm\v24.11.1\node_modules\@cuilo\cli\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
			r"C:\Users\me\AppData\Roaming\fnm\node-versions\v24.11.1\installation\node_modules\@cuilo\cli\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
			r"C:\Users\me\AppData\Local\Volta\tools\image\packages\@cuilo\cli\node_modules\@cuilo\cli\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
			r"\\?\C:\Program Files\nodejs\node_modules\@cuilo\cli\node_modules\@cuilo\cli-win32-arm64\bin\cuilo.exe",
		];

		for path in prefixes {
			assert_eq!(judged(path, &[]), Install::Global(Manager::Npm), "{path}");
		}
	}

	#[test]
	fn install_should_find_the_global_folders_of_bun_pnpm_and_yarn_on_every_platform() {
		let cases = [
			(
				"/Users/me/.bun/install/global/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Bun,
			),
			(
				"/home/me/.bun/install/global/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Manager::Bun,
			),
			(
				"/opt/bun/install/global/node_modules/@cuilo/cli-linux-arm64/bin/cuilo",
				Manager::Bun,
			),
			(
				r"C:\Users\me\.bun\install\global\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Manager::Bun,
			),
			(
				"/Users/me/Library/pnpm/global/5/node_modules/.pnpm/@cuilo+cli-darwin-arm64@2.0.0/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Pnpm,
			),
			(
				"/home/me/.local/share/pnpm/global/5/node_modules/.pnpm/@cuilo+cli-linux-x64-musl@2.0.0/node_modules/@cuilo/cli-linux-x64-musl/bin/cuilo",
				Manager::Pnpm,
			),
			(
				r"C:\Users\me\AppData\Local\pnpm\global\5\node_modules\.pnpm\@cuilo+cli-win32-x64@2.0.0\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Manager::Pnpm,
			),
			(
				"/opt/tools/global/5/node_modules/.pnpm/@cuilo+cli-linux-arm64@2.0.0/node_modules/@cuilo/cli-linux-arm64/bin/cuilo",
				Manager::Pnpm,
			),
			(
				"/Users/me/Library/pnpm/global/v11/1f3c9a7e2b-target/node_modules/.pnpm/@cuilo+cli-darwin-arm64@2.0.0/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Pnpm,
			),
			(
				"/Users/me/Library/pnpm/store/v11/links/@cuilo/cli-darwin-arm64/2.0.0/4f1c0d6a9b2e/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Pnpm,
			),
			(
				"/home/me/.local/share/pnpm/store/v11/links/@cuilo/cli-linux-x64/2.0.0/4f1c0d6a9b2e/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Manager::Pnpm,
			),
			(
				r"C:\Users\me\AppData\Local\pnpm\store\v11\links\@cuilo\cli-win32-x64\2.0.0\4f1c0d6a9b2e\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Manager::Pnpm,
			),
			(
				"/Users/me/.config/yarn/global/node_modules/@cuilo/cli-darwin-x64/bin/cuilo",
				Manager::Yarn,
			),
			(
				"/home/me/.local/share/yarn/global/node_modules/@cuilo/cli-linux-arm64-musl/bin/cuilo",
				Manager::Yarn,
			),
			(
				r"C:\Users\me\AppData\Local\Yarn\Data\global\node_modules\@cuilo\cli-win32-arm64\bin\cuilo.exe",
				Manager::Yarn,
			),
		];

		for (path, manager) in cases {
			let everything = Install::judge(path, &[], &|_| true);
			assert_eq!(everything, Install::Global(manager), "{path}");
		}
	}

	#[test]
	fn install_should_find_the_caches_of_one_off_runners_on_every_platform() {
		let cases = [
			(
				"/Users/me/.npm/_npx/0e5ad6a08e1e9350/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Npm,
			),
			(
				"/home/me/.npm/_npx/1a4eb60c8f6b0f89/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Manager::Npm,
			),
			(
				r"C:\Users\me\AppData\Local\npm-cache\_npx\1d6e82a4126006c4\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Manager::Npm,
			),
			(
				"/Users/me/.bun/install/cache/.bunx-501/@cuilo/cli@latest/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Bun,
			),
			(
				"/private/var/folders/fb/b3fb4y6n5q51w_ln9y1lr1jh0000gn/T/bunx-501-@cuilo/cli@latest/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Bun,
			),
			(
				"/home/me/.bun/install/cache/.bunx-1000/@cuilo/cli@latest/node_modules/@cuilo/cli-linux-x64-musl/bin/cuilo",
				Manager::Bun,
			),
			(
				"/tmp/.bunx-1000/@cuilo/cli@latest/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Manager::Bun,
			),
			(
				"/tmp/bunx-1000-@cuilo/cli@latest/node_modules/@cuilo/cli-linux-arm64/bin/cuilo",
				Manager::Bun,
			),
			(
				r"C:\Users\me\AppData\Local\Temp\bunx-3164710133-@cuilo\cli@latest\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Manager::Bun,
			),
			(
				"/Users/me/Library/Caches/pnpm/dlx/abfd2c4e6d5e2fd2dbf26b1d3e/19a0b7f1c2e-6f3a/node_modules/.pnpm/@cuilo+cli-darwin-arm64@2.0.0/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Pnpm,
			),
			(
				"/home/me/.cache/pnpm/dlx/abfd2c4e6d5e2fd2dbf26b1d3e/19a0b7f1c2e-6f3a/node_modules/.pnpm/@cuilo+cli-linux-x64@2.0.0/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Manager::Pnpm,
			),
			(
				r"C:\Users\me\AppData\Local\pnpm-cache\dlx\abfd2c4e6d5e2fd2dbf26b1d3e\19a0b7f1c2e-6f3a\node_modules\.pnpm\@cuilo+cli-win32-x64@2.0.0\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Manager::Pnpm,
			),
			(
				"/private/var/folders/fb/b3fb4y6n5q51w_ln9y1lr1jh0000gn/T/xfs-3f9c1a2b/dlx-48213/.yarn/unplugged/@cuilo-cli-darwin-arm64-npm-2.0.0-8d2c1f0a3b/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Manager::Yarn,
			),
			(
				"/tmp/xfs-1b2c3d4e/dlx-9120/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Manager::Yarn,
			),
			(
				r"C:\Users\me\AppData\Local\Temp\xfs-5e6f7a8b\dlx-7344\.yarn\unplugged\@cuilo-cli-win32-x64-npm-2.0.0-8d2c1f0a3b\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Manager::Yarn,
			),
		];

		for (path, manager) in cases {
			let everything = Install::judge(path, &[], &|_| true);
			assert_eq!(everything, Install::Ephemeral(manager), "{path}");
		}
	}

	#[test]
	fn install_should_tell_pnpm_dlx_from_a_global_install_in_the_shared_store_by_path() {
		let cases = [
			(
				"/Users/me/Library/pnpm/store/v11/links/@cuilo/cli-darwin-arm64/2.0.0/4f1c0d6a9b2e/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				"/Users/me/Library/Caches/pnpm/dlx/abfd2c4e6d5e/19a0b7f1c2e-6f3a/node_modules/.bin",
			),
			(
				"/home/me/.local/share/pnpm/store/v11/links/@cuilo/cli-linux-x64/2.0.0/4f1c0d6a9b2e/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				"/home/me/.cache/pnpm/dlx/abfd2c4e6d5e/19a0b7f1c2e-6f3a/node_modules/.bin",
			),
			(
				r"C:\Users\me\AppData\Local\pnpm\store\v11\links\@cuilo\cli-win32-x64\2.0.0\4f1c0d6a9b2e\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				r"C:\Users\me\AppData\Local\pnpm-cache\dlx\abfd2c4e6d5e\19a0b7f1c2e-6f3a\node_modules\.bin",
			),
		];

		for (path, dlx) in cases {
			let search = [dlx.to_owned(), "/usr/bin".to_owned()];
			assert_eq!(
				Install::judge(path, &search, &|_| false),
				Install::Ephemeral(Manager::Pnpm),
				"{path}"
			);
			assert_eq!(
				Install::judge(path, &search[1..], &|_| false),
				Install::Global(Manager::Pnpm),
				"{path}"
			);
		}
	}

	#[test]
	fn install_should_take_a_project_copy_s_manager_from_its_lockfile_on_every_platform() {
		let project = |manager: Manager, root: &str| Install::Project {
			manager,
			root: root.to_owned(),
		};
		let cases = [
			(
				"/Users/me/code/app/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				&[
					"/Users/me/code/app/package.json",
					"/Users/me/code/app/package-lock.json",
				][..],
				project(Manager::Npm, "/Users/me/code/app"),
			),
			(
				"/Users/me/code/app/node_modules/.bun/@cuilo+cli-darwin-arm64@2.0.0/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				&["/Users/me/code/app/package.json", "/Users/me/code/app/bun.lock"],
				project(Manager::Bun, "/Users/me/code/app"),
			),
			(
				"/home/me/code/app/node_modules/.pnpm/@cuilo+cli-linux-x64@2.0.0/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				&["/home/me/code/app/package.json", "/home/me/code/app/pnpm-lock.yaml"],
				project(Manager::Pnpm, "/home/me/code/app"),
			),
			(
				"/home/me/code/app/.yarn/unplugged/@cuilo-cli-linux-x64-npm-2.0.0-8d2c1f0a3b/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				&["/home/me/code/app/package.json", "/home/me/code/app/yarn.lock"],
				project(Manager::Yarn, "/home/me/code/app"),
			),
			(
				"/home/me/code/tools/packages/lib/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				&["/home/me/code/tools/packages/lib/package.json"],
				project(Manager::Npm, "/home/me/code/tools/packages/lib"),
			),
			(
				r"C:\Users\me\code\app\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				&[r"C:\Users\me\code\app\package.json", r"C:\Users\me\code\app\bun.lockb"],
				project(Manager::Bun, r"C:\Users\me\code\app"),
			),
			(
				r"\\?\C:\Users\me\code\app\node_modules\@cuilo\cli-win32-arm64\bin\cuilo.exe",
				&[
					r"\\?\C:\Users\me\code\app\package.json",
					r"\\?\C:\Users\me\code\app\yarn.lock",
				],
				project(Manager::Yarn, r"C:\Users\me\code\app"),
			),
			(
				r"D:\work\app\node_modules\.pnpm\@cuilo+cli-win32-x64@2.0.0\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				&[r"D:\work\app\package.json"],
				project(Manager::Pnpm, r"D:\work\app"),
			),
		];

		for (path, files, install) in cases {
			assert_eq!(judged(path, files), install, "{path}");
		}
	}

	#[test]
	fn install_should_leave_paths_outside_node_modules_to_homebrew_cargo_or_nobody() {
		let cases = [
			("/opt/homebrew/Cellar/cuilo/2.0.0/bin/cuilo", Install::Homebrew),
			("/usr/local/Cellar/cuilo/2.0.0/bin/cuilo", Install::Homebrew),
			(
				"/home/linuxbrew/.linuxbrew/Cellar/cuilo/2.0.0/bin/cuilo",
				Install::Homebrew,
			),
			(r"C:\Users\me\.cargo\bin\cuilo.exe", Install::Cargo),
			("/usr/local/bin/cuilo", Install::Unknown),
			(r"C:\tools\cuilo.exe", Install::Unknown),
		];

		for (path, install) in cases {
			assert_eq!(judged(path, &[]), install, "{path}");
		}
	}

	#[test]
	fn command_should_pin_the_version_for_every_manager_but_homebrew_and_one_off_runners() {
		let target = version("2.1.0-beta.1");
		let project = |manager| Install::Project {
			manager,
			root: "/app".to_owned(),
		};
		let command = |install: Install, newest: bool, binstall: bool| {
			install
				.command(&target, newest, binstall)
				.map(|command| command.to_string())
		};

		let cases = [
			(
				Install::Global(Manager::Npm),
				"npm install --global @cuilo/cli@2.1.0-beta.1",
			),
			(
				Install::Global(Manager::Pnpm),
				"pnpm add --global @cuilo/cli@2.1.0-beta.1",
			),
			(
				Install::Global(Manager::Bun),
				"bun add --global @cuilo/cli@2.1.0-beta.1",
			),
			(
				Install::Global(Manager::Yarn),
				"yarn global add @cuilo/cli@2.1.0-beta.1",
			),
			(Install::Ephemeral(Manager::Npm), "npx @cuilo/cli@next"),
			(Install::Ephemeral(Manager::Pnpm), "pnpm dlx @cuilo/cli@next"),
			(Install::Ephemeral(Manager::Bun), "bunx @cuilo/cli@next"),
			(Install::Ephemeral(Manager::Yarn), "yarn dlx @cuilo/cli@next"),
			(project(Manager::Npm), "npm install --save-dev @cuilo/cli@2.1.0-beta.1"),
			(project(Manager::Pnpm), "pnpm add --save-dev @cuilo/cli@2.1.0-beta.1"),
			(project(Manager::Bun), "bun add --dev @cuilo/cli@2.1.0-beta.1"),
			(project(Manager::Yarn), "yarn add --dev @cuilo/cli@2.1.0-beta.1"),
			(Install::Homebrew, "brew upgrade tuentyfaiv/tap/cuilo"),
			(Install::Cargo, "cargo install cuilo --locked --version 2.1.0-beta.1"),
		];
		for (install, expected) in cases {
			assert_eq!(command(install, true, false).as_deref(), Some(expected));
		}

		assert_eq!(
			command(Install::Cargo, true, true).as_deref(),
			Some("cargo binstall cuilo --version 2.1.0-beta.1 --no-confirm")
		);
		assert_eq!(
			command(Install::Ephemeral(Manager::Npm), false, false).as_deref(),
			Some("npx @cuilo/cli@2.1.0-beta.1"),
			"a chosen version is named, not the dist-tag"
		);
		assert_eq!(
			Install::Ephemeral(Manager::Npm)
				.command(&version("2.1.0"), true, false)
				.map(|command| command.to_string())
				.as_deref(),
			Some("npx @cuilo/cli@latest")
		);
		assert_eq!(command(Install::Script, true, true), None);
		assert_eq!(command(Install::Unknown, true, true), None);
		assert_eq!(
			Install::Global(Manager::Npm)
				.command(&target, true, false)
				.map(|command| command.args),
			Some(vec![
				"install".to_owned(),
				"--global".to_owned(),
				"@cuilo/cli@2.1.0-beta.1".to_owned()
			]),
			"the package and version must stay one argument"
		);
	}

	#[test]
	fn hint_should_say_who_updates_each_copy() {
		let target = version("2.1.0");
		let hint = |install: Install| install.hint(&target, true);

		assert_eq!(hint(Install::Script), "run `cuilo upgrade`");
		assert_eq!(hint(Install::Global(Manager::Pnpm)), "run `cuilo upgrade`");
		assert_eq!(hint(Install::Homebrew), "run `cuilo upgrade`");
		assert_eq!(
			hint(Install::Ephemeral(Manager::Npm)),
			"this copy runs through npx; run `npx @cuilo/cli@latest`"
		);
		assert_eq!(
			hint(Install::Ephemeral(Manager::Bun)),
			"this copy runs through bunx; run `bunx @cuilo/cli@latest`"
		);
		assert_eq!(
			hint(Install::Ephemeral(Manager::Pnpm)),
			"this copy runs through pnpm dlx; run `pnpm dlx @cuilo/cli@latest`"
		);
		assert_eq!(
			hint(Install::Ephemeral(Manager::Yarn)),
			"this copy runs through yarn dlx; run `yarn dlx @cuilo/cli@latest`"
		);
		assert_eq!(
			hint(Install::Project {
				manager: Manager::Bun,
				root: "/Users/me/code/app".to_owned(),
			}),
			"this copy is a dependency of the project in /Users/me/code/app; run `bun add --dev @cuilo/cli@2.1.0` there"
		);
		assert_eq!(
			hint(Install::Project {
				manager: Manager::Npm,
				root: "/tmp/\u{1b}[2Kevil".to_owned(),
			}),
			r"this copy is a dependency of the project in /tmp/\u{1b}[2Kevil; run `npm install --save-dev @cuilo/cli@2.1.0` there"
		);
		assert!(hint(Install::Unknown).contains("install script"));
	}

	#[test]
	fn outcome_should_report_the_installer_kind_and_project_in_json() -> anyhow::Result<()> {
		let outcome = |install| Outcome {
			current: version("2.0.0"),
			available: version("2.1.0"),
			install,
			upgraded: false,
			command: None,
		};

		let project = serde_json::to_value(outcome(Install::Project {
			manager: Manager::Pnpm,
			root: "/app".to_owned(),
		}))?;
		let npx = serde_json::to_value(outcome(Install::Ephemeral(Manager::Npm)))?;
		let script = serde_json::to_value(outcome(Install::Script))?;
		let unknown = serde_json::to_value(outcome(Install::Unknown))?;

		assert_eq!(
			(&project["install"], &project["kind"], &project["project"]),
			(&"pnpm".into(), &"project".into(), &"/app".into())
		);
		assert_eq!(
			(&npx["install"], &npx["kind"], &npx["project"]),
			(&"npm".into(), &"ephemeral".into(), &serde_json::Value::Null)
		);
		assert_eq!(
			(&script["install"], &script["kind"]),
			(&"script".into(), &"global".into())
		);
		assert_eq!(
			(&unknown["install"], &unknown["kind"]),
			(&"unknown".into(), &serde_json::Value::Null)
		);
		Ok(())
	}

	#[test]
	fn extensions_should_keep_the_runnable_ones_pathext_lists_on_windows_only() {
		let listed = extensions(true, Some(OsStr::new(".COM;.EXE;.BAT;.CMD;.VBS;.JS;.PY")));

		assert_eq!(listed, [".com", ".exe", ".bat", ".cmd"]);
		assert_eq!(extensions(true, Some(OsStr::new(".CMD;.EXE"))), [".cmd", ".exe"]);
		assert_eq!(extensions(true, None), [".com", ".exe", ".bat", ".cmd"]);
		assert_eq!(extensions(false, Some(OsStr::new(".EXE"))), [""]);
	}

	#[test]
	fn find_should_take_the_windows_shim_and_the_unix_script_from_path() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let empty = tempfile::tempdir()?;
		for file in ["npm", "npm.cmd", "npm.ps1", "bun.exe", "cargo-binstall"] {
			fs::write(dir.path().join(file), "")?;
		}
		let path = env::join_paths([Path::new("relative"), empty.path(), dir.path()])?;
		let windows = extensions(true, None);
		let unix = extensions(false, None);

		assert_eq!(find("npm", Some(&path), &windows), Some(dir.path().join("npm.cmd")));
		assert_eq!(find("bun", Some(&path), &windows), Some(dir.path().join("bun.exe")));
		assert_eq!(find("npm", Some(&path), &unix), Some(dir.path().join("npm")));
		assert_eq!(
			find("cargo-binstall", Some(&path), &unix),
			Some(dir.path().join("cargo-binstall"))
		);
		assert_eq!(find("cargo-binstall", Some(&path), &windows), None);
		assert_eq!(find("yarn", Some(&path), &unix), None);
		assert_eq!(find("npm", None, &unix), None);
		Ok(())
	}

	#[test]
	fn replace_should_swap_the_binary_and_write_the_receipt() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let exe = dir.path().join("cuilo");
		fs::write(&exe, "old")?;
		let fetch = release_server(BINARY);
		let release = release(&fetch, KEY, &version("99.0.0"))?;

		replace(&exe, &download(&fetch, &release)?, &release.version)?;

		assert_eq!(fs::read(&exe)?, BINARY);
		let receipt = fs::read_to_string(exe.with_file_name(brand::INSTALL_RECEIPT))?;
		assert!(receipt.contains("\"version\": \"99.0.0\""), "{receipt}");
		assert!(
			!sibling(&exe, &format!("new-{}", std::process::id())).exists(),
			"the staged file was left behind"
		);
		Ok(())
	}

	#[test]
	fn download_should_refuse_a_tampered_binary() -> anyhow::Result<()> {
		let fetch = release_server(b"evil");
		let release = release(&fetch, KEY, &version("99.0.0"))?;

		assert!(download(&fetch, &release).is_err());
		Ok(())
	}

	#[test]
	fn run_should_keep_unknown_installs_to_the_install_script_without_downloading() {
		let fetch = Served::default();
		let runner = Recorder::default();

		let error = run(&fetch, Path::new("/usr/local/bin/cuilo"), false, None, &runner)
			.err()
			.map(|error| error.to_string())
			.unwrap_or_default();

		assert!(
			error.contains("cannot upgrade itself") && error.contains("install script"),
			"{error}"
		);
		assert_eq!(asked(&fetch), 0);
		assert!(runner.ran.borrow().is_empty());
	}

	#[test]
	fn run_should_install_the_newest_release_with_the_package_manager() -> anyhow::Result<()> {
		let fetch = channels("99.0.0");
		let runner = Recorder::default();
		let exe = Path::new("/Users/me/.bun/install/global/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo");

		let outcome = run(&fetch, exe, false, None, &runner)?;

		assert!(outcome.upgraded);
		assert_eq!(*runner.ran.borrow(), ["bun add --global @cuilo/cli@99.0.0"]);
		assert_eq!(
			asked(&fetch),
			1,
			"only the channel is read; the package manager downloads"
		);
		let json = serde_json::to_value(&outcome)?;
		assert_eq!((&json["install"], &json["kind"]), (&"bun".into(), &"global".into()));
		assert_eq!(json["available"], "99.0.0");
		assert_eq!(json["command"], "bun add --global @cuilo/cli@99.0.0");
		Ok(())
	}

	#[test]
	fn run_should_run_nothing_when_the_wanted_version_is_running() -> anyhow::Result<()> {
		let runner = Recorder::default();
		let running = version(env!("CARGO_PKG_VERSION"));
		let exe = Path::new("/home/me/.config/yarn/global/node_modules/@cuilo/cli-linux-x64/bin/cuilo");

		let outcome = run(&Served::default(), exe, false, Some(&running), &runner)?;

		assert!(!outcome.upgraded);
		assert_eq!(outcome.command, None);
		assert!(runner.ran.borrow().is_empty());
		Ok(())
	}

	#[test]
	fn run_should_only_report_the_command_with_check() -> anyhow::Result<()> {
		let fetch = channels("99.0.0");
		let runner = Recorder::default();
		let exe = Path::new(r"C:\Users\me\AppData\Roaming\npm\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe");

		let outcome = run(&fetch, exe, true, None, &runner)?;

		assert!(!outcome.upgraded);
		assert_eq!(
			outcome.command.as_deref(),
			Some("npm install --global @cuilo/cli@99.0.0")
		);
		assert!(runner.ran.borrow().is_empty());
		Ok(())
	}

	#[test]
	fn run_should_only_point_a_one_off_runner_s_copy_back_to_its_runner() -> anyhow::Result<()> {
		let fetch = channels("99.0.0");
		let runner = Recorder::default();
		let exe = Path::new("/Users/me/.npm/_npx/0e5ad6a08e1e9350/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo");

		let newest = run(&fetch, exe, false, None, &runner)?;
		let chosen = run(&Served::default(), exe, false, Some(&version("99.0.0")), &runner)?;

		assert!(!newest.upgraded);
		assert_eq!(newest.command.as_deref(), Some("npx @cuilo/cli@latest"));
		assert_eq!(chosen.command.as_deref(), Some("npx @cuilo/cli@99.0.0"));
		assert!(runner.ran.borrow().is_empty());
		assert_eq!(asked(&fetch), 1, "only the channel is read");
		assert_eq!(serde_json::to_value(&newest)?["kind"], "ephemeral");
		Ok(())
	}

	#[test]
	fn run_should_leave_a_project_s_copy_to_the_project_s_package_manager() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		fs::write(dir.path().join("package.json"), "{}")?;
		fs::write(dir.path().join("bun.lock"), "")?;
		let exe = dir.path().join("node_modules/@cuilo/cli-linux-x64/bin/cuilo");
		let runner = Recorder::default();

		let outcome = run(&Served::default(), &exe, false, Some(&version("99.0.0")), &runner)?;

		assert!(!outcome.upgraded);
		assert_eq!(outcome.command.as_deref(), Some("bun add --dev @cuilo/cli@99.0.0"));
		assert!(runner.ran.borrow().is_empty());
		let json = serde_json::to_value(&outcome)?;
		assert_eq!((&json["install"], &json["kind"]), (&"bun".into(), &"project".into()));
		assert_eq!(json["project"], dir.path().to_string_lossy().as_ref());
		Ok(())
	}

	#[test]
	fn run_should_refuse_a_version_for_homebrew_before_any_download() -> anyhow::Result<()> {
		let fetch = channels("99.0.0");
		let runner = Recorder::default();
		let exe = Path::new("/opt/homebrew/Cellar/cuilo/2.0.0/bin/cuilo");

		let error = run(&fetch, exe, false, Some(&version("99.0.0")), &runner)
			.err()
			.map(|error| error.to_string())
			.unwrap_or_default();
		assert!(error.contains("Homebrew") && error.contains("--version"), "{error}");
		assert_eq!(asked(&fetch), 0);
		assert!(runner.ran.borrow().is_empty());

		let outcome = run(&fetch, exe, false, None, &runner)?;
		assert!(outcome.upgraded);
		assert_eq!(*runner.ran.borrow(), ["brew upgrade tuentyfaiv/tap/cuilo"]);
		Ok(())
	}

	#[test]
	fn run_should_prefer_cargo_binstall_when_it_is_on_path() -> anyhow::Result<()> {
		let exe = Path::new("/home/me/.cargo/bin/cuilo");
		let target = version("99.0.0");
		let with = Recorder {
			found: vec!["cargo-binstall"],
			..Recorder::default()
		};
		let without = Recorder::default();

		run(&Served::default(), exe, false, Some(&target), &with)?;
		run(&Served::default(), exe, false, Some(&target), &without)?;

		assert_eq!(
			*with.ran.borrow(),
			["cargo binstall cuilo --version 99.0.0 --no-confirm"]
		);
		assert_eq!(*without.ran.borrow(), ["cargo install cuilo --locked --version 99.0.0"]);
		Ok(())
	}

	#[test]
	fn run_should_fail_naming_the_command_when_the_package_manager_fails() {
		let runner = Recorder {
			fails: true,
			..Recorder::default()
		};
		let exe = Path::new("/usr/local/lib/node_modules/@cuilo/cli-linux-x64/bin/cuilo");

		let error = run(&Served::default(), exe, false, Some(&version("99.0.0")), &runner)
			.err()
			.map(|error| error.to_string())
			.unwrap_or_default();

		assert!(
			error.contains("`npm install --global @cuilo/cli@99.0.0` failed"),
			"{error}"
		);
	}
}
