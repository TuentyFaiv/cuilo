//! Black-box tests for the `cuilo` binary.

use std::error::Error;

use assert_cmd::Command;
use cuilo_core::brand;
use predicates::prelude::PredicateBooleanExt as _;
use predicates::str::{contains, starts_with};

type TestResult = Result<(), Box<dyn Error>>;

fn cuilo() -> Result<Command, Box<dyn Error>> {
	Ok(Command::cargo_bin(brand::BINARY_NAME)?)
}

#[test]
fn version_flag_should_print_name_and_package_version() -> TestResult {
	let expected = format!("{} {}", brand::BINARY_NAME, env!("CARGO_PKG_VERSION"));

	cuilo()?
		.arg("--version")
		.assert()
		.success()
		.stdout(starts_with(expected));

	Ok(())
}

#[test]
fn no_arguments_should_print_usage() -> TestResult {
	cuilo()?.assert().success().stdout(contains("Usage:"));

	Ok(())
}

#[test]
fn unknown_argument_should_fail_with_usage_error() -> TestResult {
	cuilo()?.arg("--definitely-not-a-flag").assert().code(2);

	Ok(())
}

#[test]
fn rpc_should_answer_json_rpc_on_stdio() -> TestResult {
	let dir = tempfile::tempdir()?;
	let request = r#"{"jsonrpc":"2.0","id":1,"method":"history.list"}"#;

	cuilo()?
		.arg("rpc")
		.current_dir(dir.path())
		.write_stdin(format!("{request}\n"))
		.assert()
		.success()
		.stdout(contains(r#""id":1"#).and(contains(r#""result":[]"#)));
	Ok(())
}

#[test]
fn rpc_should_stay_out_of_help() -> TestResult {
	cuilo()?.arg("--help").assert().success().stdout(contains("rpc").not());
	Ok(())
}

/// A React project with an isolated user config and pack cache.
fn react_project() -> Result<tempfile::TempDir, Box<dyn Error>> {
	let dir = tempfile::tempdir()?;
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"dependencies": {"react": "19"}, "devDependencies": {"typescript": "6"}}"#,
	)?;
	std::fs::create_dir(dir.path().join(".home"))?;
	Ok(dir)
}

/// A pack cache for `--verify` below the build folder, since one in the shared temp folder is refused.
#[cfg(unix)]
fn private_cache() -> Result<tempfile::TempDir, Box<dyn Error>> {
	Ok(tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR"))?)
}

/// Writes a folder pack at `name` in `dir` from `(path, text)` pairs.
fn local_pack(dir: &tempfile::TempDir, name: &str, files: &[(&str, &str)]) -> TestResult {
	for (path, text) in files {
		let file = dir.path().join(name).join(path);
		std::fs::create_dir_all(file.parent().ok_or("no parent folder")?)?;
		std::fs::write(file, text)?;
	}
	Ok(())
}

fn in_project(dir: &tempfile::TempDir) -> Result<Command, Box<dyn Error>> {
	let mut command = cuilo()?;
	command
		.current_dir(dir.path())
		.env("CUILO_CONFIG_DIR", dir.path().join(".home/config"))
		.env("CUILO_CACHE_DIR", dir.path().join(".home/cache"))
		.env("NO_COLOR", "1");
	Ok(command)
}

#[test]
fn add_should_plan_then_apply_only_with_yes() -> TestResult {
	let dir = react_project()?;

	in_project(&dir)?
		.args(["add", "ui/atom", "Button", "--dry-run"])
		.assert()
		.success()
		.stdout(contains("create  src/ui/atoms/button/button.tsx"));
	in_project(&dir)?
		.args(["add", "ui/atom", "Button"])
		.assert()
		.code(2)
		.stderr(contains("--yes"));
	assert!(!dir.path().join("src").exists());
	in_project(&dir)?
		.args(["add", "atom", "Button", "--yes"])
		.assert()
		.success()
		.stdout(contains("Applied transaction 1"));

	assert!(dir.path().join("src/ui/atoms/button/button.tsx").is_file());
	Ok(())
}

#[test]
fn detect_should_show_the_import_style_in_text_and_json_and_take_the_configured_one() -> TestResult {
	let dir = react_project()?;
	std::fs::write(
		dir.path().join("tsconfig.json"),
		"{\n\t// Node.js\n\t\"compilerOptions\": { \"module\": \"NodeNext\" },\n}\n",
	)?;
	let style = |args: &[&str]| -> Result<serde_json::Value, Box<dyn Error>> {
		let output = in_project(&dir)?.args(args).output()?;
		assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
		let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
		Ok(value["stack"]["import_style"].clone())
	};

	assert_eq!(style(&["detect"])?, serde_json::json!(["imports-js"]));
	assert_eq!(style(&["--json", "detect"])?, serde_json::json!(["imports-js"]));
	std::fs::write(
		dir.path().join("cuilo.json"),
		r#"{"stack": {"import_style": ["imports-extensionless"]}}"#,
	)?;
	assert_eq!(style(&["detect"])?, serde_json::json!(["imports-extensionless"]));
	std::fs::write(dir.path().join("cuilo.json"), r#"{"stack": {"import_style": ["js"]}}"#)?;
	in_project(&dir)?.arg("detect").assert().failure().stderr(contains(
		"must be `imports-extensionless`, `imports-js` or `imports-ts`",
	));
	Ok(())
}

#[test]
fn json_output_should_describe_the_plan_and_the_transaction() -> TestResult {
	let dir = react_project()?;

	let output = in_project(&dir)?
		.args(["--json", "add", "ui/atom", "Chip", "--yes"])
		.output()?;
	let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;

	assert!(output.status.success());
	assert_eq!(value["plan"]["summary"], "add ui/atom");
	assert_eq!(value["applied"]["id"], 1);
	Ok(())
}

#[test]
fn status_why_and_undo_should_follow_the_edits() -> TestResult {
	let dir = react_project()?;
	in_project(&dir)?
		.args(["add", "ui/atom", "Badge", "--yes"])
		.assert()
		.success();
	let file = dir.path().join("src/ui/atoms/badge/badge.tsx");
	std::fs::write(&file, format!("{}// mine\n", std::fs::read_to_string(&file)?))?;
	let last = std::fs::read_to_string(&file)?.lines().count();

	in_project(&dir)?
		.arg("status")
		.assert()
		.success()
		.stdout(contains("modified  src/ui/atoms/badge/badge.tsx"));
	in_project(&dir)?
		.args(["why", &format!("src/ui/atoms/badge/badge.tsx:{last}")])
		.assert()
		.success()
		.stdout(contains("is yours"));
	in_project(&dir)?
		.arg("undo")
		.assert()
		.code(1)
		.stderr(contains("changed after transaction 1"));
	in_project(&dir)?.args(["undo", "--force"]).assert().success();

	assert!(!file.exists());
	Ok(())
}

#[test]
fn a_rejected_plan_should_exit_with_three() -> TestResult {
	let dir = react_project()?;
	std::fs::create_dir_all(dir.path().join("src/ui/atoms/button"))?;
	std::fs::write(dir.path().join("src/ui/atoms/button/button.tsx"), "mine\n")?;

	in_project(&dir)?
		.args(["add", "ui/atom", "Button", "--yes"])
		.assert()
		.code(3)
		.stderr(contains("already exists"));
	Ok(())
}

#[test]
fn capture_should_make_a_piece_usable_with_from() -> TestResult {
	let dir = react_project()?;
	std::fs::create_dir_all(dir.path().join("src/widgets"))?;
	std::fs::write(
		dir.path().join("src/widgets/price-tag.ts"),
		"export const PriceTag = 'price-tag'\n",
	)?;

	in_project(&dir)?
		.args([
			"pack",
			"capture",
			"src/widgets/price-tag.ts",
			"--piece",
			"widget",
			"--name",
			"PriceTag",
			"--yes",
		])
		.assert()
		.success();
	in_project(&dir)?
		.args(["add", "local/widget", "SaleBadge", "--from", "./packs/local", "--yes"])
		.assert()
		.success();

	assert_eq!(
		std::fs::read_to_string(dir.path().join("src/widgets/sale-badge.ts"))?,
		"export const SaleBadge = 'sale-badge'\n"
	);
	Ok(())
}

#[test]
fn pack_list_should_include_the_official_packs() -> TestResult {
	let dir = react_project()?;

	in_project(&dir)?
		.args(["pack", "list"])
		.assert()
		.success()
		.stdout(contains("ui 0.2.0").and(contains("backend 0.2.0")));
	Ok(())
}

#[cfg(unix)]
#[test]
fn trust_should_let_the_project_formatter_shape_generated_files() -> TestResult {
	use std::os::unix::fs::PermissionsExt as _;

	let dir = react_project()?;
	std::fs::write(dir.path().join("biome.json"), "{}\n")?;
	let bin = dir.path().join("node_modules/.bin");
	std::fs::create_dir_all(&bin)?;
	std::fs::write(
		bin.join("biome"),
		"#!/bin/sh\ncat >/dev/null\nprintf '// formatted\\n'\n",
	)?;
	std::fs::set_permissions(bin.join("biome"), std::fs::Permissions::from_mode(0o755))?;

	in_project(&dir)?
		.args(["add", "ui/atom", "Tag", "--dry-run"])
		.assert()
		.success()
		.stdout(contains("not formatted with Biome").and(contains("cuilo trust")));
	in_project(&dir)?
		.arg("trust")
		.assert()
		.success()
		.stdout(contains("cuilo will run Biome"));
	in_project(&dir)?
		.args(["add", "ui/atom", "Tag", "--yes"])
		.assert()
		.success();

	let index = std::fs::read_to_string(dir.path().join("src/ui/atoms/tag/index.ts"))?;
	assert_eq!(index, "// formatted\n");
	in_project(&dir)?
		.args(["--json", "trust", "--revoke"])
		.assert()
		.success()
		.stdout(contains(r#""trusted": false"#));
	Ok(())
}

#[test]
fn completions_should_print_a_script_for_each_shell() -> TestResult {
	for (shell, marker) in [
		("bash", "_cuilo()"),
		("zsh", "#compdef cuilo"),
		("fish", "complete -c cuilo"),
	] {
		cuilo()?
			.args(["completions", shell])
			.assert()
			.success()
			.stdout(contains(marker));
	}
	cuilo()?.arg("man").assert().success().stdout(contains(".TH cuilo 1"));
	Ok(())
}

#[test]
fn upgrade_should_leave_a_copy_it_did_not_install_alone_without_downloading() -> TestResult {
	cuilo()?
		.args(["upgrade", "--json"])
		.assert()
		.code(1)
		.stdout(contains("cannot upgrade itself").and(contains("install script")));
	Ok(())
}

#[test]
fn upgrade_help_should_hide_the_background_flag() -> TestResult {
	cuilo()?
		.args(["upgrade", "--help"])
		.assert()
		.success()
		.stdout(contains("--check").and(contains("--background").not()));
	Ok(())
}

#[test]
fn upgrade_background_should_stay_silent_and_record_nothing_in_ci() -> TestResult {
	let dir = tempfile::tempdir()?;

	cuilo()?
		.args(["upgrade", "--background"])
		.env("CI", "true")
		.env("CUILO_CACHE_DIR", dir.path())
		.assert()
		.success()
		.stdout("")
		.stderr("");
	assert_eq!(std::fs::read_dir(dir.path())?.count(), 0);
	Ok(())
}

/// A copy of this build in the folder `bin`.
#[cfg(unix)]
fn copy_in(bin: &std::path::Path) -> Result<std::path::PathBuf, Box<dyn Error>> {
	std::fs::create_dir_all(bin)?;
	let exe = bin.join(brand::BINARY_NAME);
	std::fs::copy(assert_cmd::cargo::cargo_bin(brand::BINARY_NAME), &exe)?;
	Ok(exe)
}

/// A copy of this build where a global npm install puts it, and a `PATH` whose fake `npm` prints
/// `npm says hi`, saves its arguments in `npm-args` and exits with `status`.
#[cfg(unix)]
fn npm_install(dir: &std::path::Path, status: u8) -> Result<(std::path::PathBuf, String), Box<dyn Error>> {
	use std::os::unix::fs::PermissionsExt as _;

	let exe = copy_in(&dir.join("lib/node_modules/@cuilo/cli-test/bin"))?;
	let fakes = dir.join("fake-bin");
	std::fs::create_dir_all(&fakes)?;
	std::fs::write(
		fakes.join("npm"),
		format!(
			"#!/bin/sh\necho \"$@\" > '{}'\necho npm says hi\nexit {status}\n",
			dir.join("npm-args").display()
		),
	)?;
	std::fs::set_permissions(fakes.join("npm"), std::fs::Permissions::from_mode(0o755))?;
	Ok((exe, format!("{}:/usr/bin:/bin", fakes.display())))
}

#[cfg(unix)]
#[test]
fn upgrade_should_run_npm_for_an_npm_install_and_keep_stdout_json() -> TestResult {
	let dir = tempfile::tempdir()?;
	let (exe, path) = npm_install(dir.path(), 0)?;

	let output = Command::new(&exe)
		.args(["--json", "upgrade", "--version", "99.0.0"])
		.env("PATH", &path)
		.output()?;

	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(output.status.success(), "{stderr}");
	let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
	assert_eq!(value["install"], "npm");
	assert_eq!(value["upgraded"], true);
	assert_eq!(value["command"], "npm install --global @cuilo/cli@99.0.0");
	assert!(
		stderr.contains("$ npm install --global @cuilo/cli@99.0.0") && stderr.contains("npm says hi"),
		"{stderr}"
	);
	assert_eq!(
		std::fs::read_to_string(dir.path().join("npm-args"))?,
		"install --global @cuilo/cli@99.0.0\n"
	);
	Ok(())
}

#[cfg(unix)]
#[test]
fn upgrade_should_fail_naming_the_npm_command_that_failed() -> TestResult {
	let dir = tempfile::tempdir()?;
	let (exe, path) = npm_install(dir.path(), 7)?;

	Command::new(&exe)
		.args(["upgrade", "--version", "99.0.0"])
		.env("PATH", &path)
		.assert()
		.code(1)
		.stdout(contains("npm says hi"))
		.stderr(contains("`npm install --global @cuilo/cli@99.0.0` failed"));
	Ok(())
}

#[cfg(unix)]
#[test]
fn upgrade_should_print_the_project_s_own_command_for_a_project_copy_and_run_nothing() -> TestResult {
	let dir = tempfile::tempdir()?;
	std::fs::write(dir.path().join("package.json"), "{}")?;
	std::fs::write(dir.path().join("bun.lock"), "")?;
	let exe = copy_in(&dir.path().join("node_modules/@cuilo/cli-test/bin"))?;
	let nothing = tempfile::tempdir()?;
	let root = dir.path().canonicalize()?;

	let output = Command::new(&exe)
		.args(["--json", "upgrade", "--version", "99.0.0"])
		.env("PATH", nothing.path())
		.output()?;
	let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;

	assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
	assert_eq!(
		(&value["install"], &value["kind"], &value["upgraded"]),
		(&"bun".into(), &"project".into(), &false.into())
	);
	assert_eq!(value["project"], root.to_string_lossy().as_ref());
	assert_eq!(value["command"], "bun add --dev @cuilo/cli@99.0.0");
	Command::new(&exe)
		.args(["upgrade", "--version", "99.0.0"])
		.env("PATH", nothing.path())
		.assert()
		.success()
		.stdout(contains(format!(
			"; this copy is a dependency of the project in {}; run `bun add --dev @cuilo/cli@99.0.0` there.",
			root.display()
		)));
	Ok(())
}

#[cfg(unix)]
#[test]
fn upgrade_should_point_an_npx_copy_back_to_npx_and_run_nothing() -> TestResult {
	let dir = tempfile::tempdir()?;
	let exe = copy_in(
		&dir
			.path()
			.join("_npx/0e5ad6a08e1e9350/node_modules/@cuilo/cli-test/bin"),
	)?;
	let nothing = tempfile::tempdir()?;

	let output = Command::new(&exe)
		.args(["--json", "upgrade", "--version", "99.0.0"])
		.env("PATH", nothing.path())
		.output()?;
	let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;

	assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
	assert_eq!(
		(&value["install"], &value["kind"], &value["command"]),
		(&"npm".into(), &"ephemeral".into(), &"npx @cuilo/cli@99.0.0".into())
	);
	Command::new(&exe)
		.args(["upgrade", "--version", "99.0.0"])
		.env("PATH", nothing.path())
		.assert()
		.success()
		.stdout(contains("; this copy runs through npx; run `npx @cuilo/cli@99.0.0`."));
	Ok(())
}

#[test]
fn add_should_print_the_install_command_for_missing_packages() -> TestResult {
	let dir = react_project()?;

	in_project(&dir)?
		.args(["add", "ui/atom", "Pill", "-i", "tests=true", "--yes", "--no-input"])
		.assert()
		.success()
		.stdout(contains("Add the packages with:").and(contains("npm install --save-dev vitest@^4.0")));
	Ok(())
}

#[cfg(unix)]
#[test]
fn verify_should_refuse_a_plan_that_breaks_a_check_and_apply_one_that_does_not() -> TestResult {
	let dir = react_project()?;
	std::fs::write(
		dir.path().join("cuilo.json"),
		r#"{"verify": [["sh", "-c", "! grep -rqs Broken src"]]}"#,
	)?;
	in_project(&dir)?.arg("trust").assert().success();
	let cache = private_cache()?;

	in_project(&dir)?
		.env("CUILO_CACHE_DIR", cache.path())
		.args(["add", "ui/atom", "Broken", "--verify", "--yes"])
		.assert()
		.code(3)
		.stdout(contains("FAIL"))
		.stderr(contains("Nothing was changed"));
	assert!(!dir.path().join("src").exists());
	in_project(&dir)?
		.env("CUILO_CACHE_DIR", cache.path())
		.args(["add", "ui/atom", "Fine", "--verify", "--yes"])
		.assert()
		.success()
		.stdout(contains("pass").and(contains("Applied transaction 1")));
	Ok(())
}

#[cfg(unix)]
#[test]
fn verify_should_show_the_plan_and_run_nothing_until_the_user_agrees() -> TestResult {
	let dir = react_project()?;
	let marker = dir.path().join(".home/checked");
	let check = format!("touch '{}'", marker.display());
	std::fs::write(
		dir.path().join("cuilo.json"),
		serde_json::json!({ "verify": [["sh", "-c", check]] }).to_string(),
	)?;
	in_project(&dir)?.arg("trust").assert().success();
	let cache = private_cache()?;

	in_project(&dir)?
		.env("CUILO_CACHE_DIR", cache.path())
		.args(["add", "ui/atom", "Tag", "--dry-run", "--verify"])
		.assert()
		.code(2)
		.stdout(contains("create"))
		.stderr(contains("Nothing was run or changed"));
	assert!(!marker.exists(), "a check ran before the user agreed");

	let output = in_project(&dir)?
		.env("CUILO_CACHE_DIR", cache.path())
		.args(["add", "ui/atom", "Tag", "--dry-run", "--verify", "--yes"])
		.assert()
		.success()
		.get_output()
		.stdout
		.clone();
	let output = String::from_utf8(output)?;
	let (plan, checks) = (
		output.find("create").ok_or("no plan")?,
		output.find("Checks on a copy").ok_or("no checks")?,
	);
	assert!(plan < checks, "{output}");
	assert!(marker.exists());
	assert!(!dir.path().join("src").exists());
	Ok(())
}

#[cfg(unix)]
#[test]
fn rustfmt_should_run_only_in_a_trusted_project() -> TestResult {
	use std::os::unix::fs::PermissionsExt as _;

	let dir = tempfile::tempdir()?;
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
	)?;
	local_pack(
		&dir,
		"rust-pack",
		&[
			(
				"pack.toml",
				"schema_version = 1\nname = \"rusty\"\nversion = \"1.0.0\"\n[permissions]\nwrite = [\"src/**\"]\n",
			),
			(
				"pieces/module/piece.toml",
				"schema_version = 1\nname = \"module\"\ntarget = \"src\"\n",
			),
			("pieces/module/files/extra.rs", "pub fn extra() {}\n"),
		],
	)?;
	let bin = dir.path().join(".home/bin");
	std::fs::create_dir_all(&bin)?;
	let marker = dir.path().join(".home/rustfmt-ran");
	std::fs::write(
		bin.join("rustfmt"),
		format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.display()),
	)?;
	std::fs::set_permissions(bin.join("rustfmt"), std::fs::Permissions::from_mode(0o755))?;
	let path = format!("{}:{}", bin.display(), std::env::var("PATH")?);
	let add = || -> Result<Command, Box<dyn Error>> {
		let mut command = in_project(&dir)?;
		command
			.env("PATH", &path)
			.args(["add", "module", "--from", "./rust-pack", "--dry-run"]);
		Ok(command)
	};

	add()?.assert().success().stdout(contains("not formatted with rustfmt"));
	assert!(!marker.exists(), "rustfmt ran in an untrusted project");
	in_project(&dir)?.arg("trust").assert().success();
	add()?.assert().success();
	assert!(marker.exists(), "rustfmt did not run once the project was trusted");
	Ok(())
}

#[test]
fn trust_should_name_only_formatters_cuilo_knows() -> TestResult {
	let dir = react_project()?;
	std::fs::write(
		dir.path().join("cuilo.json"),
		r#"{"stack": {"formatter": ["prettier", "zzz-tool"]}}"#,
	)?;

	in_project(&dir)?
		.args(["--json", "trust"])
		.assert()
		.success()
		.stdout(contains(r#""Prettier""#).and(contains("zzz-tool").not()));
	Ok(())
}

#[test]
fn a_project_config_with_control_characters_should_be_refused_and_shown_escaped() -> TestResult {
	let dir = react_project()?;
	std::fs::write(
		dir.path().join("cuilo.json"),
		r#"{"stack": {"formatter": ["prettier", "\u001b[2K\rTrust? [y/N]"]}}"#,
	)?;

	in_project(&dir)?
		.args(["add", "ui/atom", "Tag", "--dry-run"])
		.assert()
		.failure()
		.stderr(contains("must be a plain name").and(contains("\u{1b}").not()));
	Ok(())
}

#[test]
fn text_from_a_pack_should_reach_the_terminal_escaped() -> TestResult {
	let dir = react_project()?;
	local_pack(
		&dir,
		"third-party",
		&[
			(
				"pack.toml",
				"schema_version = 1\nname = \"demo\"\nversion = \"1.0.0\"\ndescription = \"Demo\\u001b]0;title\\u0007\\u001b[1A\"\n[permissions]\nwrite = [\"src/**\"]\n",
			),
			(
				"pieces/atom/piece.toml",
				"schema_version = 1\nname = \"atom\"\ntarget = \"src\"\ndescription = \"An atom\\r\\u001b[2K\"\n",
			),
			("pieces/atom/files/atom.ts", "export const atom = 1;\n"),
		],
	)?;
	local_pack(
		&dir,
		"broken",
		&[
			(
				"pack.toml",
				"schema_version = 1\nname = \"broken\"\nversion = \"1.0.0\"\n",
			),
			(
				"pieces/atom/piece.toml",
				"schema_version = 1\nname = \"atom\"\ntarget = \"src\"\n\n[[dependencies]]\necosystem = \"npm\"\nname = \"x\\u001b[1A\"\nversion = \"1\"\n",
			),
			("pieces/atom/files/atom.ts", "export const atom = 1;\n"),
		],
	)?;

	in_project(&dir)?
		.args(["add", "atom", "--from", "./third-party", "--yes"])
		.assert()
		.success();
	in_project(&dir)?
		.args(["pack", "show", "demo"])
		.assert()
		.success()
		.stdout(
			contains(r"Demo\u{1b}]0;title\u{7}")
				.and(contains(r"An atom\u{d}\u{1b}[2K"))
				.and(contains("\u{1b}").not()),
		);
	in_project(&dir)?
		.args(["add", "atom", "--from", "./broken", "--yes"])
		.assert()
		.failure()
		.stderr(contains(r"x\u{1b}[1A").and(contains("\u{1b}").not()));
	Ok(())
}

#[test]
fn a_pack_from_elsewhere_should_not_take_a_built_in_name() -> TestResult {
	let dir = react_project()?;
	local_pack(
		&dir,
		"third-party",
		&[
			(
				"pack.toml",
				"schema_version = 1\nname = \"ui\"\nversion = \"50.0.0\"\n[permissions]\nwrite = [\"src/**\"]\n",
			),
			(
				"pieces/gizmo/piece.toml",
				"schema_version = 1\nname = \"gizmo\"\ntarget = \"src\"\n",
			),
			("pieces/gizmo/files/gizmo.ts", "export const gizmo = 1;\n"),
		],
	)?;

	in_project(&dir)?
		.args(["add", "gizmo", "--from", "./third-party", "--yes"])
		.assert()
		.failure()
		.stderr(contains("built into cuilo"));
	assert!(!dir.path().join("cuilo.lock").exists());
	Ok(())
}

#[test]
fn undo_should_refuse_a_cloned_journal_that_writes_into_git() -> TestResult {
	let dir = react_project()?;
	std::fs::create_dir_all(dir.path().join(".git/hooks"))?;
	let planted = b"planted by the project\n";
	let digest = cuilo_core::model::Digest::of(planted);
	let hex = digest.hex();
	let object = dir.path().join(".cuilo/objects").join(&hex[..2]).join(&hex[2..]);
	std::fs::create_dir_all(object.parent().ok_or("no parent folder")?)?;
	std::fs::write(object, planted)?;
	std::fs::create_dir_all(dir.path().join(".cuilo/history"))?;
	std::fs::write(
		dir.path().join(".cuilo/history/00000001.toml"),
		format!(
			"id = 1\ntime = 0\nsummary = \"planted\"\nstate = \"pending\"\n\n[[change]]\npath = \".git/hooks/marker\"\nbefore = \"{digest}\"\n"
		),
	)?;

	in_project(&dir)?
		.args(["undo", "--force"])
		.assert()
		.failure()
		.stderr(contains("cuilo never writes"));
	assert!(!dir.path().join(".git/hooks/marker").exists());
	Ok(())
}

#[cfg(unix)]
#[test]
fn a_running_rpc_session_should_stop_running_checks_once_trust_is_revoked() -> TestResult {
	use std::io::{BufRead as _, BufReader, Write as _};
	use std::process::Stdio;

	let dir = react_project()?;
	let marker = dir.path().join(".home/checked");
	let check = format!("echo ran >> '{}'", marker.display());
	std::fs::write(
		dir.path().join("cuilo.json"),
		serde_json::json!({ "verify": [["sh", "-c", check]] }).to_string(),
	)?;
	in_project(&dir)?.arg("trust").assert().success();
	let cache = private_cache()?;
	let mut server = std::process::Command::new(assert_cmd::cargo::cargo_bin(brand::BINARY_NAME))
		.current_dir(dir.path())
		.env("CUILO_CONFIG_DIR", dir.path().join(".home/config"))
		.env("CUILO_CACHE_DIR", cache.path())
		.arg("rpc")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()?;
	let mut input = server.stdin.take().ok_or("no stdin")?;
	let mut output = BufReader::new(server.stdout.take().ok_or("no stdout")?);
	let mut call = |id: u64, method: &str, params: serde_json::Value| -> Result<serde_json::Value, Box<dyn Error>> {
		writeln!(
			input,
			"{}",
			serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
		)?;
		loop {
			let mut line = String::new();
			if output.read_line(&mut line)? == 0 {
				return Err("the server stopped".into());
			}
			let reply: serde_json::Value = serde_json::from_str(&line)?;
			if reply["id"] == id {
				return Ok(reply);
			}
		}
	};

	let plan = call(
		1,
		"plan.create",
		serde_json::json!({ "pack": "ui", "piece": "atom", "inputs": { "name": "Tag" } }),
	)?;
	let id = plan["result"]["plan"].clone();
	assert!(call(2, "plan.verify", serde_json::json!({ "plan": id }))?["result"].is_object());
	in_project(&dir)?.args(["trust", "--revoke"]).assert().success();
	let refused = call(3, "plan.verify", serde_json::json!({ "plan": id }))?;

	assert!(
		refused["error"]["message"]
			.as_str()
			.is_some_and(|message| message.contains("trusted project")),
		"{refused}"
	);
	assert_eq!(std::fs::read_to_string(&marker)?.lines().count(), 1);
	drop(input);
	assert!(server.wait()?.success());
	Ok(())
}

/// A Hono project with a local pack whose `page` piece has a plain step and a wiring step.
fn wiring_project() -> Result<tempfile::TempDir, Box<dyn Error>> {
	let dir = tempfile::tempdir()?;
	std::fs::create_dir_all(dir.path().join(".home/config"))?;
	std::fs::create_dir(dir.path().join("src"))?;
	std::fs::write(dir.path().join("cuilo.json"), r#"{"stack": {"framework": ["hono"]}}"#)?;
	std::fs::write(dir.path().join("src/index.ts"), "const app = new Hono();\n")?;
	local_pack(
		&dir,
		"site",
		&[
			(
				"pack.toml",
				"schema_version = 1\nname = \"site\"\nversion = \"1.0.0\"\n[permissions]\nwrite = [\"src/**\"]\n",
			),
			(
				"pieces/page/piece.toml",
				"schema_version = 1\nname = \"page\"\ntarget = \"src/pages\"\n[inputs.name]\ntype = \"string\"\nexample = \"MyPage\"\n\
				 [[steps]]\ntext = \"Restart the dev server.\"\n\
				 [[steps]]\ntext = \"Mount `myPageRoutes` in `src/index.ts`.\"\n\
				 [[steps.patches]]\nkind = \"anchor\"\nfile = \"src/index.ts\"\nanchor = \"const app = new Hono()\"\ninsert = \"app.route('/my-page', myPageRoutes);\"\n",
			),
			("pieces/page/files/my-page.ts", "export const myPageRoutes = [];\n"),
		],
	)?;
	Ok(dir)
}

#[test]
fn add_should_list_next_steps_once_at_the_end() -> TestResult {
	let dir = wiring_project()?;

	let preview = in_project(&dir)?
		.args(["add", "page", "Settings", "--from", "./site", "--dry-run"])
		.output()?;
	let applied = in_project(&dir)?
		.args(["add", "page", "Settings", "--from", "./site", "--yes"])
		.output()?;

	let preview = String::from_utf8(preview.stdout)?;
	let applied = String::from_utf8(applied.stdout)?;
	assert_eq!(preview.matches("Next steps:").count(), 1, "{preview}");
	assert_eq!(applied.matches("Next steps:").count(), 1, "{applied}");
	assert!(
		applied.ends_with("Next steps:\n  1. Restart the dev server.\n  2. Mount `settingsRoutes` in `src/index.ts`.\n"),
		"{applied}"
	);
	assert!(applied.find("Applied") < applied.find("Next steps:"), "{applied}");
	assert_eq!(
		std::fs::read_to_string(dir.path().join("src/index.ts"))?,
		"const app = new Hono();\n"
	);
	Ok(())
}

#[test]
fn wire_flags_should_override_the_user_config() -> TestResult {
	let dir = wiring_project()?;
	let index = || std::fs::read_to_string(dir.path().join("src/index.ts"));

	let wired = in_project(&dir)?
		.args(["add", "page", "Settings", "--from", "./site", "--wire", "--yes"])
		.output()?;
	assert!(index()?.contains("app.route('/settings', settingsRoutes);"));
	assert!(!String::from_utf8(wired.stdout)?.contains("Mount"));

	std::fs::write(dir.path().join(".home/config/config.toml"), "wire = true\n")?;
	let unwired = in_project(&dir)?
		.args(["add", "page", "Billing", "--from", "./site", "--no-wire", "--yes"])
		.output()?;
	assert!(!index()?.contains("billing"));
	assert!(String::from_utf8(unwired.stdout)?.contains("2. Mount `billingRoutes` in `src/index.ts`."));

	in_project(&dir)?
		.args(["add", "page", "Profile", "--from", "./site", "--yes"])
		.assert()
		.success();
	assert!(index()?.contains("app.route('/profile', profileRoutes);"));

	in_project(&dir)?
		.args(["add", "page", "Team", "--from", "./site", "--wire", "--no-wire"])
		.assert()
		.code(2);
	Ok(())
}

#[test]
fn json_plan_should_carry_the_next_steps() -> TestResult {
	let dir = wiring_project()?;

	let output = in_project(&dir)?
		.args(["--json", "add", "page", "Settings", "--from", "./site", "--dry-run"])
		.output()?;
	let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;

	assert_eq!(
		value["plan"]["next_steps"],
		serde_json::json!(["Restart the dev server.", "Mount `settingsRoutes` in `src/index.ts`."])
	);
	Ok(())
}
