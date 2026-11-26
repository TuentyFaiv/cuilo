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
use serde::Serialize;

use crate::ui::Output;

/// The minisign public key that signs every release's `SHA256SUMS`; empty until the first release.
pub(crate) const RELEASE_KEY: &str = include_str!("../release-key.pub");

/// The target triple this binary was built for, e.g. `aarch64-apple-darwin`.
pub(crate) const TARGET: &str = env!("CUILO_TARGET");

/// The Homebrew formula, in its tap.
const HOMEBREW_FORMULA: &str = concat!("tuentyfaiv/tap/", cuilo_core::brand_name!());

/// The crate `cargo install` and `cargo binstall` install.
const CRATE: &str = env!("CARGO_PKG_NAME");

/// How the running binary was installed, which decides who may replace it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Install {
	/// By the install script; `upgrade` replaces it.
	Script,
	/// By npm.
	Npm,
	/// By pnpm.
	Pnpm,
	/// By Bun.
	Bun,
	/// By Yarn 1, the one with `yarn global`.
	Yarn,
	/// By Homebrew.
	Homebrew,
	/// By `cargo install` or `cargo binstall`.
	Cargo,
	/// Some other way, e.g. a binary copied by hand.
	Unknown,
}

impl Install {
	/// How the binary at `exe` was installed.
	pub(crate) fn of(exe: &Path) -> Self {
		if exe.with_file_name(brand::INSTALL_RECEIPT).is_file() {
			return Self::Script;
		}
		Self::from_path(&exe.to_string_lossy())
	}

	/// How a binary at `path` was installed, judged by its folder names; `/` and `\` both separate
	/// them, so one platform can judge another's paths.
	fn from_path(path: &str) -> Self {
		let parts: Vec<String> = path
			.split(['/', '\\'])
			.filter(|part| !part.is_empty())
			.map(str::to_ascii_lowercase)
			.collect();
		let has = |name: &str| parts.iter().any(|part| part == name);
		let position = |names: &[&str]| parts.iter().position(|part| names.contains(&part.as_str()));
		if has("node_modules") {
			let yarn = position(&["yarn", ".yarn", ".yarn-config"]);
			let global = parts.iter().rposition(|part| part == "global");
			if has(".bun")
				|| parts
					.windows(3)
					.any(|names| names == ["install", "global", "node_modules"])
			{
				Self::Bun
			} else if has("pnpm") || has(".pnpm") {
				Self::Pnpm
			} else if yarn.zip(global).is_some_and(|(yarn, global)| yarn < global) {
				Self::Yarn
			} else {
				Self::Npm
			}
		} else if has("cellar") || has("homebrew") || has("linuxbrew") {
			Self::Homebrew
		} else if parts.len() >= 3 && parts[parts.len() - 3] == ".cargo" && parts[parts.len() - 2] == "bin" {
			Self::Cargo
		} else {
			Self::Unknown
		}
	}

	/// The package manager's name, e.g. `npm`; `None` for script and unknown installs.
	pub(crate) fn manager(self) -> Option<&'static str> {
		match self {
			Self::Npm => Some("npm"),
			Self::Pnpm => Some("pnpm"),
			Self::Bun => Some("Bun"),
			Self::Yarn => Some("Yarn"),
			Self::Homebrew => Some("Homebrew"),
			Self::Cargo => Some("cargo"),
			Self::Script | Self::Unknown => None,
		}
	}

	/// The package-manager command that installs `version` over this copy, or `None` when no package
	/// manager installed it. Homebrew cannot pin a version, so it moves to its formula's.
	/// `binstall` says whether `cargo-binstall` is on `PATH`.
	pub(crate) fn command(self, version: &Version, binstall: bool) -> Option<Invocation> {
		let package = format!("{}@{version}", brand::NPM_PACKAGE);
		let version = version.to_string();
		let (program, args): (&str, Vec<&str>) = match self {
			Self::Npm => ("npm", vec!["install", "--global", &package]),
			Self::Pnpm => ("pnpm", vec!["add", "--global", &package]),
			Self::Bun => ("bun", vec!["add", "--global", &package]),
			Self::Yarn => ("yarn", vec!["global", "add", &package]),
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

	/// What to do to upgrade this copy, for messages.
	pub(crate) fn hint(self) -> String {
		if self == Self::Unknown {
			format!(
				"{} did not install this copy; reinstall with the install script at {}/install.sh",
				brand::NAME,
				brand::DOWNLOAD_URL
			)
		} else {
			format!("run `{} upgrade`", brand::BINARY_NAME)
		}
	}
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
	/// How the binary was installed.
	pub(crate) install: Install,
	/// Whether this copy was upgraded.
	pub(crate) upgraded: bool,
	/// The package-manager command that ran, or with `--check` the one that would run.
	pub(crate) command: Option<String>,
}

/// Runs `cuilo upgrade`: replaces a script install itself, and runs the package manager for others.
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
		bail!("this cuilo cannot upgrade itself: {}", install.hint());
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
	let command = install.command(&target, install == Install::Cargo && runner.finds("cargo-binstall"));
	outcome.command = command.as_ref().map(ToString::to_string);
	if check {
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
	} else if available > current {
		let how = match command {
			Some(command) => format!("{}, which runs `{command}`", install.hint()),
			None => install.hint(),
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

	#[test]
	fn install_should_follow_the_receipt_and_known_locations() -> anyhow::Result<()> {
		let dir = tempfile::tempdir()?;
		let exe = dir.path().join("bin/cuilo");
		fs::create_dir_all(dir.path().join("bin"))?;

		assert_eq!(Install::of(&exe), Install::Unknown);
		fs::write(exe.with_file_name(brand::INSTALL_RECEIPT), "{}")?;
		assert_eq!(Install::of(&exe), Install::Script);
		assert_eq!(
			Install::of(Path::new("/p/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo")),
			Install::Npm
		);
		assert_eq!(
			Install::of(Path::new("/opt/homebrew/Cellar/cuilo/2.0.0/bin/cuilo")),
			Install::Homebrew
		);
		assert_eq!(Install::of(Path::new("/home/me/.cargo/bin/cuilo")), Install::Cargo);
		Ok(())
	}

	#[test]
	fn install_should_tell_package_managers_apart_on_every_platform() {
		let cases = [
			(
				"/opt/homebrew/lib/node_modules/@cuilo/cli/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Install::Npm,
			),
			(
				"/home/me/.nvm/versions/node/v24.11.1/lib/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Install::Npm,
			),
			(
				r"C:\Users\me\AppData\Roaming\npm\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Install::Npm,
			),
			(
				r"\\?\C:\Program Files\nodejs\node_modules\@cuilo\cli\node_modules\@cuilo\cli-win32-arm64\bin\cuilo.exe",
				Install::Npm,
			),
			(
				"/Users/me/Library/pnpm/global/5/node_modules/.pnpm/@cuilo+cli-darwin-arm64@2.0.0/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Install::Pnpm,
			),
			(
				"/home/me/.local/share/pnpm/global/5/node_modules/.pnpm/@cuilo+cli-linux-x64-musl@2.0.0/node_modules/@cuilo/cli-linux-x64-musl/bin/cuilo",
				Install::Pnpm,
			),
			(
				r"C:\Users\me\AppData\Local\pnpm\global\5\node_modules\.pnpm\@cuilo+cli-win32-x64@2.0.0\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Install::Pnpm,
			),
			(
				"/opt/tools/global/5/node_modules/.pnpm/@cuilo+cli-linux-arm64@2.0.0/node_modules/@cuilo/cli-linux-arm64/bin/cuilo",
				Install::Pnpm,
			),
			(
				"/Users/me/.bun/install/global/node_modules/@cuilo/cli-darwin-arm64/bin/cuilo",
				Install::Bun,
			),
			(
				"/opt/bun/install/global/node_modules/@cuilo/cli-linux-x64/bin/cuilo",
				Install::Bun,
			),
			(
				r"C:\Users\me\.bun\install\global\node_modules\@cuilo\cli-win32-x64\bin\cuilo.exe",
				Install::Bun,
			),
			(
				"/Users/me/.config/yarn/global/node_modules/@cuilo/cli-darwin-x64/bin/cuilo",
				Install::Yarn,
			),
			(
				"/home/me/.config/yarn/global/node_modules/@cuilo/cli-linux-arm64-musl/bin/cuilo",
				Install::Yarn,
			),
			(
				r"C:\Users\me\AppData\Local\Yarn\Data\global\node_modules\@cuilo\cli-win32-arm64\bin\cuilo.exe",
				Install::Yarn,
			),
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
			assert_eq!(Install::from_path(path), install, "{path}");
		}
	}

	#[test]
	fn command_should_pin_the_version_for_every_manager_but_homebrew() {
		let target = version("2.1.0-beta.1");
		let command =
			|install: Install, binstall: bool| install.command(&target, binstall).map(|command| command.to_string());

		assert_eq!(
			command(Install::Npm, false).as_deref(),
			Some("npm install --global @cuilo/cli@2.1.0-beta.1")
		);
		assert_eq!(
			command(Install::Pnpm, false).as_deref(),
			Some("pnpm add --global @cuilo/cli@2.1.0-beta.1")
		);
		assert_eq!(
			command(Install::Bun, false).as_deref(),
			Some("bun add --global @cuilo/cli@2.1.0-beta.1")
		);
		assert_eq!(
			command(Install::Yarn, false).as_deref(),
			Some("yarn global add @cuilo/cli@2.1.0-beta.1")
		);
		assert_eq!(
			command(Install::Homebrew, false).as_deref(),
			Some("brew upgrade tuentyfaiv/tap/cuilo")
		);
		assert_eq!(
			command(Install::Cargo, true).as_deref(),
			Some("cargo binstall cuilo --version 2.1.0-beta.1 --no-confirm")
		);
		assert_eq!(
			command(Install::Cargo, false).as_deref(),
			Some("cargo install cuilo --locked --version 2.1.0-beta.1")
		);
		assert_eq!(command(Install::Script, true), None);
		assert_eq!(command(Install::Unknown, true), None);
		assert_eq!(
			Install::Npm.command(&target, false).map(|command| command.args),
			Some(vec![
				"install".to_owned(),
				"--global".to_owned(),
				"@cuilo/cli@2.1.0-beta.1".to_owned()
			]),
			"the package and version must stay one argument"
		);
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
		assert_eq!(json["install"], "bun");
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
