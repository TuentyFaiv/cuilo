//! The install scripts download a release binary, check it against `SHA256SUMS` (and its signature
//! when minisign is there), and install it with the receipt `cuilo upgrade` looks for. Downloads
//! are served from a folder, not the network.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cuilo_core::brand;
use cuilo_core::model::Digest;

type TestResult = Result<(), Box<dyn Error>>;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn script(name: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install").join(name)
}

/// What a release folder serves besides the binary and its checksums.
#[derive(Clone, Default)]
struct Served {
	/// `SHA256SUMS` lists another digest for the binary.
	tamper: bool,
	/// There is no `latest` channel yet, only `latest-pre`.
	no_stable: bool,
	/// The trusted comment the binary's signature carries.
	signed_as: Option<String>,
}

/// A release folder holding this build of cuilo as `binary`, with its `SHA256SUMS` and channels.
fn release(dir: &Path, binary: &str, served: &Served) -> Result<PathBuf, Box<dyn Error>> {
	let release = dir.join("release");
	fs::create_dir_all(&release)?;
	let bytes = fs::read(assert_cmd::cargo::cargo_bin("cuilo"))?;
	fs::write(release.join(binary), &bytes)?;
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt as _;
		fs::set_permissions(release.join(binary), fs::Permissions::from_mode(0o755))?;
	}
	let digest = if served.tamper {
		Digest::of(b"other")
	} else {
		Digest::of(&bytes)
	};
	fs::write(
		release.join("SHA256SUMS"),
		format!(
			"{}  {binary}\n{}  cuilo-elsewhere\n",
			digest.hex(),
			Digest::of(b"x").hex()
		),
	)?;
	if !served.no_stable {
		fs::write(release.join("latest"), format!("{VERSION}\n"))?;
	}
	fs::write(release.join("latest-pre"), format!("{VERSION}\n"))?;
	if let Some(comment) = &served.signed_as {
		fs::write(release.join(format!("{binary}.minisig")), comment)?;
	}
	Ok(release)
}

#[cfg(unix)]
mod shell {
	use std::os::unix::fs::PermissionsExt as _;
	use std::process::Output;

	use super::*;

	/// The key the signature tests put into a copy of install.sh.
	const TEST_KEY: &str = "RWQtestkey";

	/// The binary install.sh picks on this machine.
	fn binary() -> String {
		let os = if cfg!(target_os = "macos") {
			"apple-darwin"
		} else {
			"unknown-linux-musl"
		};
		let arch = match std::env::consts::ARCH {
			"aarch64" => "aarch64",
			_ if cfg!(target_os = "macos") && rosetta() => "aarch64",
			other => other,
		};
		format!("cuilo-{arch}-{os}")
	}

	fn rosetta() -> bool {
		Command::new("sysctl")
			.args(["-n", "sysctl.proc_translated"])
			.output()
			.is_ok_and(|output| output.stdout.trim_ascii() == b"1")
	}

	fn executable(path: &Path, text: &str) -> Result<(), Box<dyn Error>> {
		fs::write(path, text)?;
		fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
		Ok(())
	}

	/// A folder with a `curl` that copies `<release>/<last path segment>` to `--output` and logs its
	/// arguments, and, when `minisign` is set, a `minisign` that accepts [`TEST_KEY`] and prints the
	/// signature file as its trusted comment.
	fn fakes(dir: &Path, release: &Path, minisign: bool) -> Result<PathBuf, Box<dyn Error>> {
		let bin = dir.join("fake-bin");
		fs::create_dir_all(&bin)?;
		executable(
			&bin.join("curl"),
			&format!(
				"#!/bin/sh\necho \"$@\" >> '{log}'\nout=''\nurl=''\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    --output) out=\"$2\"; shift ;;\n    --max-filesize) shift ;;\n    *) url=\"$1\" ;;\n  esac\n  shift\ndone\nfile='{release}'/\"${{url##*/}}\"\n[ -f \"$file\" ] || exit 22\ncp \"$file\" \"$out\"\n",
				log = dir.join("curl.log").display(),
				release = release.display(),
			),
		)?;
		if minisign {
			executable(
				&bin.join("minisign"),
				&format!(
					"#!/bin/sh\nkey=''\nfile=''\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -P) key=\"$2\"; shift ;;\n    -m) file=\"$2\"; shift ;;\n  esac\n  shift\ndone\n[ \"$key\" = '{TEST_KEY}' ] && [ -f \"$file.minisig\" ] || exit 1\ncat \"$file.minisig\"\n"
				),
			)?;
		}
		Ok(bin)
	}

	/// Runs `script` with only the fakes and the system folders on `PATH`.
	fn run(dir: &Path, script: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Output, Box<dyn Error>> {
		let path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", dir.join("fake-bin").display());
		let mut command = Command::new("sh");
		command
			.arg(script)
			.args(args)
			.env("PATH", path)
			.env("HOME", dir)
			.env("CUILO_INSTALL", dir.join("cuilo"))
			.env_remove("CUILO_VERSION")
			.env_remove("CUILO_REQUIRE_SIGNATURE");
		for (name, value) in env {
			command.env(name, value);
		}
		Ok(command.output()?)
	}

	fn install(dir: &Path, served: &Served, args: &[&str]) -> Result<Output, Box<dyn Error>> {
		let release = release(dir, &binary(), served)?;
		fakes(dir, &release, false)?;
		run(dir, &script("install.sh"), args, &[])
	}

	/// A copy of install.sh that carries [`TEST_KEY`], run with the fake minisign.
	fn install_signed(dir: &Path, served: &Served) -> Result<Output, Box<dyn Error>> {
		let release = release(dir, &binary(), served)?;
		fakes(dir, &release, true)?;
		let text = fs::read_to_string(script("install.sh"))?;
		let key_line = text
			.lines()
			.find(|line| line.starts_with("release_key="))
			.ok_or("install.sh has no release_key line")?;
		let copy = dir.join("install.sh");
		fs::write(
			&copy,
			text.replacen(key_line, &format!("release_key=\"{TEST_KEY}\""), 1),
		)?;
		run(dir, &copy, &[], &[("CUILO_REQUIRE_SIGNATURE", "1")])
	}

	fn text(output: &Output) -> String {
		format!(
			"{}{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		)
	}

	fn installed(dir: &Path) -> PathBuf {
		dir.join("cuilo/bin/cuilo")
	}

	#[test]
	fn install_sh_should_install_the_latest_checked_binary_with_a_receipt() -> TestResult {
		let dir = tempfile::tempdir()?;

		let output = install(dir.path(), &Served::default(), &[])?;

		let text = text(&output);
		assert!(output.status.success(), "{text}");
		assert!(text.contains(&format!("Installed cuilo {VERSION}")), "{text}");
		assert!(text.contains("SHA-256 checked; install minisign"), "{text}");
		assert_eq!(
			fs::read(installed(dir.path()))?,
			fs::read(assert_cmd::cargo::cargo_bin("cuilo"))?
		);
		let receipt = fs::read_to_string(dir.path().join("cuilo/bin").join(brand::INSTALL_RECEIPT))?;
		assert!(receipt.contains(&format!("\"version\": \"{VERSION}\"")), "{receipt}");
		let log = fs::read_to_string(dir.path().join("curl.log"))?;
		assert!(
			log
				.lines()
				.all(|line| line.starts_with("--proto =https --tlsv1.2 --fail")),
			"{log}"
		);
		assert!(!log.contains("--location"), "install.sh follows redirects: {log}");
		assert!(log.contains(&format!("{}/latest\n", brand::DOWNLOAD_URL)), "{log}");
		assert!(
			log.contains(&format!("{}/v{VERSION}/{}\n", brand::DOWNLOAD_URL, binary())),
			"{log}"
		);
		Ok(())
	}

	#[test]
	fn install_sh_should_say_it_installs_a_pre_release_while_there_is_no_stable_one() -> TestResult {
		let dir = tempfile::tempdir()?;

		let output = install(
			dir.path(),
			&Served {
				no_stable: true,
				..Served::default()
			},
			&[],
		)?;

		let text = text(&output);
		assert!(output.status.success(), "{text}");
		assert!(
			text.contains(&format!("No stable release yet; installing the pre-release {VERSION}")),
			"{text}"
		);
		Ok(())
	}

	#[test]
	fn install_sh_should_refuse_a_binary_that_does_not_match_its_checksum() -> TestResult {
		let dir = tempfile::tempdir()?;

		let output = install(
			dir.path(),
			&Served {
				tamper: true,
				..Served::default()
			},
			&[],
		)?;

		assert!(!output.status.success());
		assert!(text(&output).contains("does not match SHA256SUMS"), "{}", text(&output));
		assert!(!installed(dir.path()).exists());
		Ok(())
	}

	#[test]
	fn install_sh_should_refuse_a_version_that_is_not_one_before_downloading() -> TestResult {
		let dir = tempfile::tempdir()?;

		let output = install(dir.path(), &Served::default(), &["../../evil"])?;

		assert!(!output.status.success());
		assert!(text(&output).contains("is not a cuilo version"), "{}", text(&output));
		assert!(!dir.path().join("curl.log").exists(), "it downloaded something");
		Ok(())
	}

	#[test]
	fn install_sh_should_refuse_to_skip_a_required_signature() -> TestResult {
		let dir = tempfile::tempdir()?;
		let release = release(dir.path(), &binary(), &Served::default())?;
		fakes(dir.path(), &release, false)?;

		let output = run(
			dir.path(),
			&script("install.sh"),
			&[],
			&[("CUILO_REQUIRE_SIGNATURE", "1")],
		)?;

		assert!(!output.status.success());
		assert!(
			text(&output).contains("CUILO_REQUIRE_SIGNATURE=1 needs minisign"),
			"{}",
			text(&output)
		);
		assert!(!installed(dir.path()).exists());
		Ok(())
	}

	#[test]
	fn install_sh_should_check_the_signature_names_this_version_and_file() -> TestResult {
		let good = tempfile::tempdir()?;
		let output = install_signed(
			good.path(),
			&Served {
				signed_as: Some(format!("cuilo {VERSION} {}", binary())),
				..Served::default()
			},
		)?;
		assert!(output.status.success(), "{}", text(&output));
		assert!(
			text(&output).contains("signature and SHA-256 checked"),
			"{}",
			text(&output)
		);

		let old = tempfile::tempdir()?;
		let output = install_signed(
			old.path(),
			&Served {
				signed_as: Some("cuilo 0.0.1 cuilo-elsewhere".to_owned()),
				..Served::default()
			},
		)?;
		assert!(!output.status.success());
		assert!(text(&output).contains("is signed as `cuilo 0.0.1"), "{}", text(&output));
		assert!(!installed(old.path()).exists());

		let unsigned = tempfile::tempdir()?;
		let output = install_signed(unsigned.path(), &Served::default())?;
		assert!(!output.status.success());
		assert!(text(&output).contains("could not download"), "{}", text(&output));
		assert!(!installed(unsigned.path()).exists());
		Ok(())
	}
}

/// Runs install.ps1 on Windows, and elsewhere when `CUILO_TEST_PWSH` names a PowerShell.
#[test]
fn install_ps1_should_install_a_checked_binary_with_a_receipt() -> TestResult {
	let pwsh = match std::env::var("CUILO_TEST_PWSH") {
		Ok(pwsh) => pwsh,
		Err(_) if cfg!(windows) => "pwsh".to_owned(),
		Err(_) => return Ok(()),
	};
	let arch = if std::env::consts::ARCH == "aarch64" {
		"aarch64"
	} else {
		"x86_64"
	};
	let binary = format!("cuilo-{arch}-pc-windows-msvc.exe");
	for tamper in [false, true] {
		let dir = tempfile::tempdir()?;
		let release = release(
			dir.path(),
			&binary,
			&Served {
				tamper,
				..Served::default()
			},
		)?;
		let mock = "function Invoke-WebRequest { param([string]$Uri, [string]$OutFile, [switch]$UseBasicParsing, [int]$MaximumRedirection) \
			$file = Join-Path $env:CUILO_TEST_RELEASE ($Uri -split '/')[-1]; \
			if (-not (Test-Path $file)) { throw \"404 $Uri\" }; Copy-Item $file $OutFile }";
		let output = Command::new(&pwsh)
			.args(["-NoProfile", "-NonInteractive", "-Command"])
			.arg(format!("{mock}; & '{}'", script("install.ps1").display()))
			.env("CUILO_TEST_RELEASE", &release)
			.env("CUILO_INSTALL", dir.path().join("cuilo"))
			.env("CUILO_NO_MODIFY_PATH", "1")
			.env(
				"PROCESSOR_ARCHITECTURE",
				if arch == "aarch64" { "ARM64" } else { "AMD64" },
			)
			.env_remove("PROCESSOR_ARCHITEW6432")
			.env_remove("CUILO_VERSION")
			.env_remove("CUILO_REQUIRE_SIGNATURE")
			.output()?;
		let text = format!(
			"{}{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		let installed = dir.path().join("cuilo/bin");
		if tamper {
			assert!(!output.status.success(), "{text}");
			assert!(text.contains("does not match SHA256SUMS"), "{text}");
			assert!(!installed.join("cuilo.exe").exists());
		} else {
			assert!(output.status.success(), "{text}");
			assert!(text.contains(&format!("Installed cuilo {VERSION}")), "{text}");
			let receipt = fs::read_to_string(installed.join(brand::INSTALL_RECEIPT))?;
			assert!(receipt.contains(VERSION), "{receipt}");
		}
	}
	Ok(())
}
