//! Release files: the npm packages, the Homebrew formula, and checks that every channel agrees on
//! the version, names and key.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The workspace version, the one version of every channel.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The changelogs, English first, with the word each heading uses until its version is released.
const CHANGELOGS: [(&str, &str); 2] = [
	("apps/docs/src/content/docs/changelog.md", "unreleased"),
	("apps/docs/src/content/docs/es-mx/changelog.md", "sin publicar"),
];

/// The docs site that the changelog's root-relative links point into.
const SITE: &str = "https://cuilo.dev";

/// A release target and its npm package.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Platform {
	/// Rust target triple; the release binary is `cuilo-<target>[.exe]`.
	pub(crate) target: &'static str,
	/// Suffix of the npm package, e.g. `darwin-arm64` for `@cuilo/cli-darwin-arm64`.
	pub(crate) npm: &'static str,
	/// npm `os`.
	os: &'static str,
	/// npm `cpu`.
	cpu: &'static str,
	/// npm `libc`, on Linux.
	libc: Option<&'static str>,
}

impl Platform {
	/// The release binary's file name.
	pub(crate) fn binary(self) -> String {
		let suffix = if self.os == "win32" { ".exe" } else { "" };
		format!("cuilo-{}{suffix}", self.target)
	}
}

/// Every target the release workflow builds.
pub(crate) const PLATFORMS: [Platform; 8] = [
	Platform {
		target: "aarch64-apple-darwin",
		npm: "darwin-arm64",
		os: "darwin",
		cpu: "arm64",
		libc: None,
	},
	Platform {
		target: "x86_64-apple-darwin",
		npm: "darwin-x64",
		os: "darwin",
		cpu: "x64",
		libc: None,
	},
	Platform {
		target: "x86_64-unknown-linux-gnu",
		npm: "linux-x64",
		os: "linux",
		cpu: "x64",
		libc: Some("glibc"),
	},
	Platform {
		target: "aarch64-unknown-linux-gnu",
		npm: "linux-arm64",
		os: "linux",
		cpu: "arm64",
		libc: Some("glibc"),
	},
	Platform {
		target: "x86_64-unknown-linux-musl",
		npm: "linux-x64-musl",
		os: "linux",
		cpu: "x64",
		libc: Some("musl"),
	},
	Platform {
		target: "aarch64-unknown-linux-musl",
		npm: "linux-arm64-musl",
		os: "linux",
		cpu: "arm64",
		libc: Some("musl"),
	},
	Platform {
		target: "x86_64-pc-windows-msvc",
		npm: "win32-x64",
		os: "win32",
		cpu: "x64",
		libc: None,
	},
	Platform {
		target: "aarch64-pc-windows-msvc",
		npm: "win32-arm64",
		os: "win32",
		cpu: "arm64",
		libc: None,
	},
];

/// The repository root.
pub(crate) fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> Result<String, String> {
	fs::read_to_string(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<(), String> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|error| format!("could not create {}: {error}", parent.display()))?;
	}
	fs::write(path, bytes).map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// The string value of `pub const <name>` in `brand.rs`.
fn brand(root: &Path, name: &str) -> Result<String, String> {
	let text = read(&root.join("crates/cuilo-core/src/brand.rs"))?;
	let prefix = format!("pub const {name}: &str = \"");
	text
		.lines()
		.find_map(|line| line.trim().strip_prefix(prefix.as_str())?.strip_suffix("\";"))
		.map(ToOwned::to_owned)
		.ok_or_else(|| format!("brand.rs has no `{name}`"))
}

/// The download host every channel fetches release files from.
pub(crate) fn download_url() -> Result<String, String> {
	brand(&root(), "DOWNLOAD_URL")
}

/// The minisign public key that signs release files; empty until the owner adds it.
fn release_key(root: &Path) -> Result<String, String> {
	Ok(read(&root.join("crates/cuilo/release-key.pub"))?.trim().to_owned())
}

/// The wrapper's `package.json` as committed in `npm/cli/`.
fn wrapper_manifest(root: &Path) -> Result<serde_json::Map<String, Value>, String> {
	let path = root.join("npm/cli/package.json");
	match serde_json::from_str(&read(&path)?) {
		Ok(Value::Object(manifest)) => Ok(manifest),
		_ => Err(format!("{} is not a JSON object", path.display())),
	}
}

/// The npm package name, e.g. `@cuilo/cli`; the platform packages add `-<platform>`.
fn npm_name(manifest: &serde_json::Map<String, Value>) -> Result<String, String> {
	manifest
		.get("name")
		.and_then(Value::as_str)
		.map(ToOwned::to_owned)
		.ok_or_else(|| "npm/cli/package.json has no name".to_owned())
}

/// Writes the wrapper package and one package per platform into `out`, from the release binaries
/// in `binaries`; a platform whose binary is missing is an error.
pub(crate) fn npm(binaries: &Path, out: &Path) -> Result<Vec<PathBuf>, String> {
	let root = root();
	let mut wrapper = wrapper_manifest(&root)?;
	let name = npm_name(&wrapper)?;
	let license = read(&root.join("LICENSE"))?;
	let mut written = Vec::new();
	let mut optional = serde_json::Map::new();
	for platform in PLATFORMS {
		let binary = binaries.join(platform.binary());
		let bytes = fs::read(&binary).map_err(|error| format!("could not read {}: {error}", binary.display()))?;
		let package = format!("{name}-{}", platform.npm);
		let dir = out.join(format!("cli-{}", platform.npm));
		let mut manifest = json!({
			"name": package,
			"version": VERSION,
			"description": format!("The cuilo binary for {}.", platform.target),
			"license": wrapper.get("license").cloned().unwrap_or(Value::Null),
			"repository": wrapper.get("repository").cloned().unwrap_or(Value::Null),
			"os": [platform.os],
			"cpu": [platform.cpu],
			"files": ["bin"],
			"preferUnplugged": true,
		});
		if let Some(libc) = platform.libc {
			manifest["libc"] = json!([libc]);
		}
		let file = if platform.os == "win32" {
			"bin/cuilo.exe"
		} else {
			"bin/cuilo"
		};
		write(&dir.join(file), &bytes)?;
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt as _;
			fs::set_permissions(dir.join(file), fs::Permissions::from_mode(0o755))
				.map_err(|error| format!("could not make {file} executable: {error}"))?;
		}
		write(&dir.join("package.json"), pretty(&manifest)?)?;
		write(&dir.join("LICENSE"), &license)?;
		optional.insert(package, json!(VERSION));
		written.push(dir);
	}
	wrapper.insert("version".to_owned(), json!(VERSION));
	wrapper.insert("optionalDependencies".to_owned(), Value::Object(optional));
	wrapper.remove("private");
	let dir = out.join("cli");
	write(&dir.join("package.json"), pretty(&Value::Object(wrapper))?)?;
	for file in ["bin/cuilo.js", "README.md"] {
		write(&dir.join(file), read(&root.join("npm/cli").join(file))?)?;
	}
	write(&dir.join("LICENSE"), &license)?;
	written.push(dir);
	Ok(written)
}

fn pretty(value: &Value) -> Result<String, String> {
	serde_json::to_string_pretty(value)
		.map(|text| text + "\n")
		.map_err(|error| error.to_string())
}

/// The digests in a `SHA256SUMS` file, by file name.
fn sums(text: &str) -> BTreeMap<&str, &str> {
	text
		.lines()
		.filter_map(|line| line.split_once("  "))
		.map(|(digest, name)| (name.trim_start_matches('*'), digest))
		.collect()
}

/// The Homebrew formula for this version, from the release's `SHA256SUMS`.
pub(crate) fn formula(sums_text: &str) -> Result<String, String> {
	let digests = sums(sums_text);
	let download = download_url()?;
	let entry = |target: &str| -> Result<String, String> {
		let name = format!("cuilo-{target}");
		let digest = digests
			.get(name.as_str())
			.ok_or_else(|| format!("SHA256SUMS does not list {name}"))?;
		Ok(format!(
			"      url \"{download}/v#{{version}}/{name}\"\n      sha256 \"{digest}\""
		))
	};
	let mut formula = String::new();
	let _ = write!(
		formula,
		r##"# Generated by `cargo xtask formula` for each release; edit the generator, not this file.
class Cuilo < Formula
  desc "Generate projects and code from versioned, updatable packs"
  homepage "https://cuilo.dev"
  version "{VERSION}"
  license "MIT"

  on_macos do
    on_arm do
{}
    end
    on_intel do
{}
    end
  end

  on_linux do
    on_arm do
{}
    end
    on_intel do
{}
    end
  end

  def install
    bin.install Dir["cuilo-*"].first => "cuilo"
    generate_completions_from_executable(bin/"cuilo", "completions")
    (man1/"cuilo.1").write Utils.safe_popen_read(bin/"cuilo", "man")
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/cuilo --version")
  end
end
"##,
		entry("aarch64-apple-darwin")?,
		entry("x86_64-apple-darwin")?,
		entry("aarch64-unknown-linux-musl")?,
		entry("x86_64-unknown-linux-musl")?,
	);
	Ok(formula)
}

/// Problems where the channels disagree: names, the download host, the release key and the
/// target list.
pub(crate) fn check() -> Result<Vec<String>, String> {
	let root = root();
	let mut problems = Vec::new();
	let wrapper = wrapper_manifest(&root)?;
	let name = npm_name(&wrapper)?;
	if brand(&root, "NPM_PACKAGE")? != name {
		problems.push(format!(
			"brand::NPM_PACKAGE is not `{name}`, the name in npm/cli/package.json"
		));
	}
	if wrapper.get("version").and_then(Value::as_str) != Some(VERSION) {
		problems.push(format!("npm/cli/package.json's version is not {VERSION}"));
	}
	let download = brand(&root, "DOWNLOAD_URL")?;
	if !download.starts_with("https://") || download.ends_with('/') {
		problems.push(format!(
			"brand::DOWNLOAD_URL `{download}` is not an https:// URL without a trailing slash"
		));
	}
	let key = release_key(&root)?;
	let shell = read(&root.join("install/install.sh"))?;
	let powershell = read(&root.join("install/install.ps1"))?;
	let cargo = read(&root.join("crates/cuilo/Cargo.toml"))?;
	let expected = [
		("install/install.sh", &shell, format!("download_url=\"{download}\"")),
		("install/install.sh", &shell, format!("release_key=\"{key}\"")),
		(
			"install/install.ps1",
			&powershell,
			format!("$DownloadUrl = '{download}'"),
		),
		("install/install.ps1", &powershell, format!("$ReleaseKey = '{key}'")),
		(
			"crates/cuilo/Cargo.toml",
			&cargo,
			format!("pkg-url = \"{download}/v{{ version }}/"),
		),
		("crates/cuilo/Cargo.toml", &cargo, format!("pubkey = \"{key}\"")),
	];
	for (file, text, line) in expected {
		if !text.contains(&line) {
			problems.push(format!("{file} does not have `{line}`"));
		}
	}
	let workflow = read(&root.join(".github/workflows/release.yml"))?;
	for platform in PLATFORMS {
		if !workflow.contains(&format!("target: {}", platform.target)) {
			problems.push(format!("release.yml does not build {}", platform.target));
		}
	}
	Ok(problems)
}

/// What still stops a release, though the files agree: the release key and the download host.
pub(crate) fn blockers() -> Result<Vec<String>, String> {
	let root = root();
	let mut blockers = Vec::new();
	if release_key(&root)?.is_empty() {
		blockers.push("crates/cuilo/release-key.pub is empty; add the minisign public key (RELEASING.md)".to_owned());
	}
	let download = brand(&root, "DOWNLOAD_URL")?;
	if download.ends_with(".invalid") {
		blockers.push(format!(
			"brand::DOWNLOAD_URL is the placeholder `{download}`; set the Cloudflare download domain (RELEASING.md)"
		));
	}
	for (path, unreleased) in CHANGELOGS {
		match section(&read(&root.join(path))?, VERSION) {
			None => blockers.push(format!("{path} has no section for {VERSION}")),
			Some((heading, _)) if heading.contains(unreleased) => blockers.push(format!(
				"{path} still calls {VERSION} {unreleased}; put the release date in its heading"
			)),
			Some(_) => {}
		}
	}
	Ok(blockers)
}

/// This version's GitHub Release notes: its section of the English changelog.
pub(crate) fn notes() -> Result<String, String> {
	let (path, _) = CHANGELOGS[0];
	notes_in(&read(&root().join(path))?, VERSION).ok_or_else(|| format!("{path} has no section for {VERSION}"))
}

/// `version`'s section of `changelog` without its heading, with links into the docs site made absolute.
fn notes_in(changelog: &str, version: &str) -> Option<String> {
	let (_, body) = section(changelog, version)?;
	Some(format!("{}\n", body.trim().replace("](/", &format!("]({SITE}/"))))
}

/// `version`'s heading line in `changelog` and the text up to the next version's heading.
fn section<'a>(changelog: &'a str, version: &str) -> Option<(&'a str, &'a str)> {
	let heading = format!("## {version} ");
	let start = if changelog.starts_with(&heading) {
		0
	} else {
		changelog.find(&format!("\n{heading}"))? + 1
	};
	let (heading, body) = changelog[start..].split_once('\n').unwrap_or((&changelog[start..], ""));
	let end = body.find("\n## ").map_or(body.len(), |end| end + 1);
	Some((heading, &body[..end]))
}

/// The channel files this version moves forward, given the versions they name now (empty when a
/// channel has no release yet): `latest-pre` takes every newer version, `latest` only stable ones.
pub(crate) fn channels(latest: &str, latest_pre: &str) -> Result<Vec<&'static str>, String> {
	let version = semver::Version::parse(VERSION).map_err(|error| error.to_string())?;
	let behind = |current: &str| -> Result<bool, String> {
		let current = current.trim();
		if current.is_empty() {
			return Ok(true);
		}
		semver::Version::parse(current)
			.map(|current| current < version)
			.map_err(|error| format!("the channel names `{current}`, which is not a version: {error}"))
	};
	let mut moved = Vec::new();
	if version.pre.is_empty() && behind(latest)? {
		moved.push("latest");
	}
	if behind(latest_pre)? {
		moved.push("latest-pre");
	}
	Ok(moved)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn check_should_find_the_channels_in_agreement() -> Result<(), String> {
		assert_eq!(check()?, Vec::<String>::new());
		Ok(())
	}

	#[test]
	fn section_should_end_at_the_next_version() {
		let changelog = "---\ntitle: Changelog\n---\n\n## 2.0.1 (2027-01-02)\n\nFixed [a](/b/).\n\n### Bugs\n\n- c\n\n## 2.0.0 (2027-01-01)\n\nFirst.\n";

		assert_eq!(
			section(changelog, "2.0.1"),
			Some(("## 2.0.1 (2027-01-02)", "\nFixed [a](/b/).\n\n### Bugs\n\n- c\n\n"))
		);
		assert_eq!(section(changelog, "2.0.0").map(|(_, body)| body), Some("\nFirst.\n"));
		assert_eq!(section(changelog, "2.0"), None);
	}

	#[test]
	fn notes_should_point_docs_links_at_the_site() {
		let changelog = "## 2.0.1 (2027-01-02)\n\nSee [Install](/install/) and [Rust](https://rust-lang.org).\n";

		assert_eq!(
			notes_in(changelog, "2.0.1").as_deref(),
			Some("See [Install](https://cuilo.dev/install/) and [Rust](https://rust-lang.org).\n")
		);
	}

	#[test]
	fn notes_should_find_this_versions_section() -> Result<(), String> {
		assert_ne!(notes()?.trim(), "");
		Ok(())
	}

	#[test]
	fn channels_should_move_forward_only() -> Result<(), String> {
		let version = semver::Version::parse(VERSION).map_err(|error| error.to_string())?;
		let stable = version.pre.is_empty();

		assert_eq!(
			channels("", "")?,
			if stable {
				vec!["latest", "latest-pre"]
			} else {
				vec!["latest-pre"]
			}
		);
		assert_eq!(channels("999.0.0", "999.0.0\n")?, Vec::<&str>::new());
		assert_eq!(channels("999.0.0", "0.0.1")?, vec!["latest-pre"]);
		assert!(channels("", "<html>").is_err());
		Ok(())
	}

	#[test]
	fn formula_should_pin_each_binary_by_digest() -> Result<(), String> {
		let sums = PLATFORMS
			.iter()
			.enumerate()
			.fold(String::new(), |mut sums, (index, platform)| {
				let _ = writeln!(sums, "{index:064}  {}", platform.binary());
				sums
			});

		let formula = formula(&sums)?;

		assert!(formula.contains(&format!("version \"{VERSION}\"")));
		assert!(formula.contains(&format!(
			"url \"{}/v#{{version}}/cuilo-aarch64-apple-darwin\"\n      sha256 \"0000",
			download_url()?
		)));
		assert!(formula.contains(&format!("sha256 \"{:064}\"", 5)), "{formula}");
		assert!(super::formula("").is_err());
		Ok(())
	}

	#[test]
	fn npm_should_write_a_package_per_platform_and_the_wrapper() -> Result<(), String> {
		let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
		let binaries = dir.path().join("dist");
		for platform in PLATFORMS {
			write(&binaries.join(platform.binary()), platform.target)?;
		}

		let written = npm(&binaries, &dir.path().join("npm"))?;

		assert_eq!(written.len(), PLATFORMS.len() + 1);
		let musl: Value = serde_json::from_str(&read(&dir.path().join("npm/cli-linux-x64-musl/package.json"))?)
			.map_err(|error| error.to_string())?;
		assert_eq!(musl["libc"], json!(["musl"]));
		assert_eq!(musl["cpu"], json!(["x64"]));
		assert_eq!(
			read(&dir.path().join("npm/cli-win32-arm64/bin/cuilo.exe"))?,
			"aarch64-pc-windows-msvc"
		);
		let wrapper: Value =
			serde_json::from_str(&read(&dir.path().join("npm/cli/package.json"))?).map_err(|error| error.to_string())?;
		assert_eq!(wrapper["version"], json!(VERSION));
		assert_eq!(
			wrapper["optionalDependencies"].as_object().map(serde_json::Map::len),
			Some(8)
		);
		assert!(wrapper.get("private").is_none());
		Ok(())
	}
}
