//! `cuilo upgrade`: replaces a script-installed binary with a release whose signed checksums match.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail, ensure};
use cuilo_core::brand;
use cuilo_core::model::Digest;
use cuilo_packs::transport::Fetch;
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::Serialize;

/// The minisign public key that signs every release's `SHA256SUMS`; empty until the first release.
pub(crate) const RELEASE_KEY: &str = include_str!("../release-key.pub");

/// The target triple this binary was built for, e.g. `aarch64-apple-darwin`.
pub(crate) const TARGET: &str = env!("CUILO_TARGET");

/// How the running binary was installed, which decides who may replace it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Install {
	/// By the install script; `upgrade` replaces it.
	Script,
	/// By an npm-compatible package manager.
	Npm,
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
		let parts: Vec<String> = exe
			.components()
			.map(|part| part.as_os_str().to_string_lossy().to_ascii_lowercase())
			.collect();
		let has = |name: &str| parts.iter().any(|part| part == name);
		if has("node_modules") {
			Self::Npm
		} else if has("cellar") || has("homebrew") || has("linuxbrew") {
			Self::Homebrew
		} else if parts.len() >= 3 && parts[parts.len() - 3] == ".cargo" && parts[parts.len() - 2] == "bin" {
			Self::Cargo
		} else {
			Self::Unknown
		}
	}

	/// What to run instead when `upgrade` may not replace the binary.
	pub(crate) fn hint(&self) -> String {
		match self {
			Self::Script => String::new(),
			Self::Npm => format!(
				"it was installed with npm; run `npm install --global {}@latest`",
				brand::NPM_PACKAGE
			),
			Self::Homebrew => format!(
				"it was installed with Homebrew; run `brew upgrade {}`",
				brand::BINARY_NAME
			),
			Self::Cargo => format!(
				"it was installed with cargo; run `cargo binstall {0}` or `cargo install {0} --locked`",
				brand::BINARY_NAME
			),
			Self::Unknown => format!(
				"cuilo did not install this copy; reinstall with the install script at {}/install.sh",
				brand::DOWNLOAD_URL
			),
		}
	}
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

/// The newest release on the stable channel, or on the pre-release channel when `pre` is set. Only
/// a hint: the version installed is the one its signed `SHA256SUMS` names, and `run` never goes down.
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
	let staged = sibling(exe, "new");
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
	pub(crate) current: String,
	/// The version offered.
	pub(crate) available: String,
	/// How the binary was installed.
	pub(crate) install: Install,
	/// Whether the binary was replaced.
	pub(crate) upgraded: bool,
}

/// Runs `cuilo upgrade`.
pub(crate) fn run(fetch: &dyn Fetch, exe: &Path, check: bool, wanted: Option<&Version>) -> anyhow::Result<Outcome> {
	let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
	let install = Install::of(exe);
	if !check && install != Install::Script {
		bail!("this cuilo cannot upgrade itself: {}", install.hint());
	}
	let target = match wanted {
		Some(version) => version.clone(),
		None => latest(fetch, !current.pre.is_empty())?,
	};
	let mut outcome = Outcome {
		current: current.to_string(),
		available: target.to_string(),
		install,
		upgraded: false,
	};
	if check || (wanted.is_none() && target <= current) || wanted == Some(&current) {
		return Ok(outcome);
	}
	let release = release(fetch, RELEASE_KEY, &target)?;
	let bytes = download(fetch, &release)?;
	replace(exe, &bytes, &release.version)?;
	outcome.upgraded = true;
	Ok(outcome)
}

#[cfg(test)]
mod tests {
	use std::sync::Mutex;

	use cuilo_packs::transport::{FetchError, Header};

	use super::*;

	/// A throwaway key pair made for these tests; it signs nothing real.
	const KEY: &str = "RWRRuwa0l7u2Rh4gptzRQofBRURbNgjE8e/Ad6WYt+uvshv/s6OB9zVY";
	const OTHER_KEY: &str = "RWSSBRIxu9J58X+jmq/hTBpD4KnRr1oz2tt5M4n2tZ5+85grXHGK5v/C";
	const SIGNATURE: &str = "untrusted comment: signature from rsign secret key\nRURRuwa0l7u2RplKzWxjYG8787O3MPww0rSWlUrVhV/YTCGD6ioxkKbrtrKitElsYdvbogVtLDlrmNvK+RWHz8gBHVHgazDLmQQ=\ntrusted comment: cuilo 99.0.0\n89TAIyDABi/kDoaz0EP1P3SNtC8iEDhKnbYuM+TtNM2SLfWZCpza1RV4vZ/F3y5n6OBmC6FrCXn9BnZN/E3LCQ==\n";
	/// `SHA256SUMS` for version 99.0.0, listing [`BINARY`] for every release target.
	const SUMS: &str = "d323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-apple-darwin\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-apple-darwin\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-unknown-linux-gnu\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-unknown-linux-gnu\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-unknown-linux-musl\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-unknown-linux-musl\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-x86_64-pc-windows-msvc.exe\nd323d7e42e896bdc8f52482a9b8f8c91db37c37cc3c335b21ecc157568dcb32b  cuilo-aarch64-pc-windows-msvc.exe\n";
	const BINARY: &[u8] = b"new cuilo\n";

	#[test]
	fn release_key_should_be_a_minisign_public_key() {
		assert!(PublicKey::from_base64(RELEASE_KEY.trim()).is_ok(), "{RELEASE_KEY}");
	}

	fn version(text: &str) -> Version {
		Version::parse(text).unwrap_or_else(|_| Version::new(0, 0, 0))
	}

	/// Serves fixed bodies by URL suffix and records what was asked.
	#[derive(Debug, Default)]
	struct Served {
		bodies: Vec<(String, Vec<u8>)>,
		asked: Mutex<Vec<String>>,
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

	fn release_server(binary: &[u8]) -> Served {
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

	#[test]
	fn run_should_never_go_down_to_an_older_latest() -> anyhow::Result<()> {
		let fetch = Served {
			bodies: vec![
				("/latest".to_owned(), b"0.0.1\n".to_vec()),
				("/latest-pre".to_owned(), b"0.0.1\n".to_vec()),
			],
			asked: Mutex::default(),
		};
		let dir = tempfile::tempdir()?;
		let exe = dir.path().join("cuilo");
		fs::write(&exe, "current")?;
		fs::write(exe.with_file_name(brand::INSTALL_RECEIPT), "{}")?;

		let outcome = run(&fetch, &exe, false, None)?;

		assert!(!outcome.upgraded);
		assert_eq!(fs::read(&exe)?, b"current");
		assert!(
			fetch.asked.lock().is_ok_and(|asked| asked.len() == 1),
			"it downloaded more than the channel"
		);
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
		assert_eq!(fs::read_dir(dir.path())?.count(), 2, "a staged file was left behind");
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
	fn run_should_leave_other_installs_to_their_manager_without_downloading() {
		let fetch = Served::default();

		let error = run(&fetch, Path::new("/home/me/.cargo/bin/cuilo"), false, None)
			.err()
			.map(|error| error.to_string())
			.unwrap_or_default();

		assert!(error.contains("cargo binstall"), "{error}");
		assert!(fetch.asked.lock().is_ok_and(|asked| asked.is_empty()));
	}
}
