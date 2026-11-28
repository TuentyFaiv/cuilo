//! `--verify` runs the project's checks on a copy with the plan applied; installs list package-manager commands.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use cuilo_core::{Engine, EngineError, Pack, PieceRequest, UserConfig, Value, Values};

type TestResult = Result<(), Box<dyn Error>>;

/// A pack whose piece writes `src/<name>.ts` with `text`, needing `zod` and the dev package `vitest`.
fn pack(text: &str) -> Result<Pack, Box<dyn Error>> {
	Ok(Pack::from_files(BTreeMap::from([
		(
			"pack.toml".to_owned(),
			b"schema_version = 1\nname = \"site\"\nversion = \"1.0.0\"\n[permissions]\nwrite = [\"src/**\"]\ndependencies = true\n"
				.to_vec(),
		),
		(
			"pieces/page/piece.toml".to_owned(),
			b"schema_version = 1\nname = \"page\"\ntarget = \"src\"\n[inputs.name]\ntype = \"string\"\nexample = \"MyPage\"\n\n[[dependencies]]\necosystem = \"npm\"\nname = \"zod\"\nversion = \"^4.1\"\n\n[[dependencies]]\necosystem = \"npm\"\nname = \"vitest\"\nversion = \"^4.0\"\ndev = true\n"
				.to_vec(),
		),
		("pieces/page/files/my-page.ts".to_owned(), text.as_bytes().to_vec()),
	]))?)
}

/// A project whose check fails on any line with `bad` in `src/`, printing those lines.
fn project(existing: Option<&str>) -> Result<tempfile::TempDir, Box<dyn Error>> {
	let dir = tempfile::tempdir()?;
	fs::create_dir_all(dir.path().join("src"))?;
	fs::write(
		dir.path().join("cuilo.json"),
		r#"{"verify": [["sh", "-c", "! grep -rn bad src"]]}"#,
	)?;
	fs::write(
		dir.path().join("package.json"),
		r#"{"packageManager": "bun@1.3.0", "devDependencies": {"vitest": "4.0.0"}}"#,
	)?;
	if let Some(text) = existing {
		fs::write(dir.path().join("src/old.ts"), text)?;
	}
	Ok(dir)
}

/// Copies go below the build folder, which no other user can write to, unlike the shared temp folder.
fn staging() -> std::path::PathBuf {
	Path::new(env!("CARGO_TARGET_TMPDIR")).join("verify")
}

fn open(root: &Path, trusted: bool) -> Result<Engine, EngineError> {
	Engine::builder()
		.user_config(UserConfig::default())
		.project_tools(trusted)
		.verify_dir(staging())
		.open(root)
}

fn plan_page(engine: &Engine, pack: &Pack) -> Result<cuilo_core::plan::Plan<cuilo_core::plan::Validated>, EngineError> {
	let inputs = Values::from([("name".to_owned(), Value::String("Home".to_owned()))]);
	engine.plan(&PieceRequest {
		pack,
		piece: "page",
		inputs: &inputs,
		variant: None,
		target: None,
		granted: None,
		force: false,
		wire: false,
	})
}

#[test]
fn verify_should_pass_a_good_plan_without_touching_the_project() -> TestResult {
	let dir = project(None)?;
	let engine = open(dir.path(), true)?;
	let plan = plan_page(&engine, &pack("export const page = 'fine'\n")?)?;

	let verification = engine.verify(&plan)?;

	assert!(verification.passed(), "{verification:?}");
	assert_eq!(verification.checks[0].command, "sh -c ! grep -rn bad src");
	assert!(!dir.path().join("src/home.ts").exists());
	Ok(())
}

#[test]
fn verify_should_block_a_plan_that_breaks_a_check_even_when_it_already_failed() -> TestResult {
	let bad = pack("export const page = 'bad news'\n")?;

	let clean = project(None)?;
	let engine = open(clean.path(), true)?;
	let verification = engine.verify(&plan_page(&engine, &bad)?)?;
	assert!(!verification.passed());
	assert_eq!(verification.checks[0].passes_without_plan, Some(true));

	let failing = project(Some("export const old = 'bad already'\n"))?;
	let engine = open(failing.path(), true)?;
	let verification = engine.verify(&plan_page(&engine, &bad)?)?;
	let check = &verification.checks[0];
	assert!(!verification.passed());
	assert_eq!(check.passes_without_plan, Some(false));
	assert_eq!(check.output, "src/home.ts:1:export const page = 'bad news'");
	Ok(())
}

#[test]
fn verify_should_not_blame_a_plan_for_a_check_that_already_fails_the_same_way() -> TestResult {
	let dir = project(Some("export const old = 'bad already'\n"))?;
	let engine = open(dir.path(), true)?;
	let plan = plan_page(&engine, &pack("export const page = 'fine'\n")?)?;

	let verification = engine.verify(&plan)?;

	assert!(verification.passed());
	assert!(!verification.checks[0].passed && !verification.checks[0].blocking);
	Ok(())
}

#[test]
fn verify_should_refuse_a_plan_that_writes_into_linked_dependencies() -> TestResult {
	let dir = project(None)?;
	fs::create_dir_all(dir.path().join("node_modules/dep"))?;
	let pack = Pack::from_files(BTreeMap::from([
		(
			"pack.toml".to_owned(),
			b"schema_version = 1\nname = \"deps\"\nversion = \"1.0.0\"\n[permissions]\nwrite = [\"node_modules/**\"]\n"
				.to_vec(),
		),
		(
			"pieces/plant/piece.toml".to_owned(),
			b"schema_version = 1\nname = \"plant\"\ntarget = \"\"\n".to_vec(),
		),
		(
			"pieces/plant/files/node_modules/planted/index.js".to_owned(),
			b"module.exports = 'planted';\n".to_vec(),
		),
	]))?;
	let engine = open(dir.path(), true)?;
	let plan = engine.plan(&PieceRequest {
		pack: &pack,
		piece: "plant",
		inputs: &Values::default(),
		variant: None,
		target: None,
		granted: None,
		force: false,
		wire: false,
	})?;

	let error = engine
		.verify(&plan)
		.err()
		.ok_or("verified a plan that writes into node_modules")?;

	assert!(error.to_string().contains("shares with the project"), "{error}");
	assert!(!dir.path().join("node_modules/planted").exists());
	Ok(())
}

#[test]
fn verify_should_run_the_checks_in_a_private_copy() -> TestResult {
	let dir = project(None)?;
	let out = tempfile::tempdir()?;
	let marker = out.path().join("marker.txt");
	let script = format!("ls -ld . > '{0}' && pwd -P >> '{0}'", marker.display());
	fs::write(
		dir.path().join("cuilo.json"),
		serde_json::json!({ "verify": [["sh", "-c", script]] }).to_string(),
	)?;
	let engine = open(dir.path(), true)?;
	let plan = plan_page(&engine, &pack("export const page = 'fine'\n")?)?;

	assert!(engine.verify(&plan)?.passed());

	let seen = fs::read_to_string(&marker)?;
	let (mode, cwd) = seen.split_once('\n').ok_or("the check did not run")?;
	assert!(mode.starts_with("drwx------"), "{mode}");
	assert!(Path::new(cwd.trim()).starts_with(staging().canonicalize()?), "{cwd}");
	Ok(())
}

#[test]
fn verify_should_need_a_trusted_project() -> TestResult {
	let dir = project(None)?;
	let engine = open(dir.path(), false)?;
	let plan = plan_page(&engine, &pack("export const page = 'fine'\n")?)?;

	let error = engine.verify(&plan).err().ok_or("verified an untrusted project")?;

	assert!(error.to_string().contains("cuilo trust"), "{error}");
	Ok(())
}

#[test]
fn installs_should_use_the_project_manager_and_skip_declared_packages() -> TestResult {
	let dir = project(None)?;
	let engine = open(dir.path(), false)?;
	let plan = plan_page(&engine, &pack("export const page = 'fine'\n")?)?;

	let installs = engine.installs(plan.dependencies())?;

	assert_eq!(
		installs.commands.iter().map(ToString::to_string).collect::<Vec<_>>(),
		["bun add zod@^4.1"]
	);
	assert_eq!(installs.present, ["vitest"]);
	Ok(())
}
