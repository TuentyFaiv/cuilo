//! Generated files follow the project's own formatter, only where the user trusts the project.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use cuilo_core::engine::UpdateRequest;
use cuilo_core::format::{Format, FormatError};
use cuilo_core::plan::OnConflict;
use cuilo_core::{Engine, Pack, PieceRequest, RelPath, UserConfig, Value, Values};

type TestResult = Result<(), Box<dyn Error>>;

/// Double quotes and a semicolon at the end of every line, like a strict formatter.
#[derive(Debug)]
struct Semicolons;

impl Format for Semicolons {
	fn format(&self, tool: &str, _path: &RelPath, text: &str) -> Result<Option<String>, FormatError> {
		assert_eq!(tool, "prettier");
		let formatted: String = text
			.replace('\'', "\"")
			.lines()
			.map(|line| {
				if line.is_empty() || line.ends_with(';') {
					format!("{line}\n")
				} else {
					format!("{line};\n")
				}
			})
			.collect();
		Ok((formatted != text).then_some(formatted))
	}
}

/// Fails on every file.
#[derive(Debug)]
struct Broken;

impl Format for Broken {
	fn format(&self, _tool: &str, _path: &RelPath, _text: &str) -> Result<Option<String>, FormatError> {
		Err(FormatError::Failed("Unexpected token".to_owned()))
	}
}

fn page(version: &str, text: &str) -> Result<Pack, Box<dyn Error>> {
	Ok(Pack::from_files(BTreeMap::from([
		(
			"pack.toml".to_owned(),
			format!("schema_version = 1\nname = \"site\"\nversion = \"{version}\"\n[permissions]\nwrite = [\"src/**\"]\n")
				.into_bytes(),
		),
		(
			"pieces/page/piece.toml".to_owned(),
			b"schema_version = 1\nname = \"page\"\ntarget = \"src/pages\"\n[inputs.name]\ntype = \"string\"\nexample = \"MyPage\"\n"
				.to_vec(),
		),
		("pieces/page/files/my-page.ts".to_owned(), text.as_bytes().to_vec()),
	]))?)
}

fn named(name: &str) -> Values {
	Values::from([("name".to_owned(), Value::String(name.to_owned()))])
}

fn request<'a>(pack: &'a Pack, inputs: &'a Values) -> PieceRequest<'a> {
	PieceRequest {
		pack,
		piece: "page",
		inputs,
		variant: None,
		target: None,
		granted: None,
		force: false,
		wire: false,
	}
}

fn update(pack: &Pack) -> UpdateRequest<'_> {
	UpdateRequest {
		pack,
		granted: None,
		on_conflict: OnConflict::Refuse,
		force: false,
		migrate: true,
	}
}

fn project(config: &str) -> Result<tempfile::TempDir, Box<dyn Error>> {
	let dir = tempfile::tempdir()?;
	fs::write(dir.path().join(config), "{}\n")?;
	Ok(dir)
}

fn read(root: &Path) -> Result<String, Box<dyn Error>> {
	Ok(fs::read_to_string(root.join("src/pages/home.ts"))?)
}

#[test]
fn generated_files_should_follow_the_project_formatter_and_updates_should_merge_from_that() -> TestResult {
	let body = "export const a = 1\nexport const x = 0\nexport const y = 0\n";
	let v1 = page(
		"1.0.0",
		&format!("export const page = 'MyPage'\n{body}export const b = 2\n"),
	)?;
	let v2 = page(
		"1.1.0",
		&format!("export const page = 'MyPage'\n{body}export const b = 20\n"),
	)?;
	let dir = project(".prettierrc")?;
	let engine = Engine::builder()
		.user_config(UserConfig::default())
		.formatter(Semicolons)
		.open(dir.path())?;
	let values = named("Home");

	engine.apply(engine.plan(&request(&v1, &values))?)?;
	assert_eq!(
		read(dir.path())?,
		"export const page = \"Home\";\nexport const a = 1;\nexport const x = 0;\nexport const y = 0;\nexport const b = 2;\n"
	);

	fs::write(
		dir.path().join("src/pages/home.ts"),
		"export const page = \"Home\";\nexport const a = 10;\nexport const x = 0;\nexport const y = 0;\nexport const b = 2;\n",
	)?;
	let plan = engine.plan_update(&update(&v2), &[v1][..])?;
	assert!(plan.notes().is_empty(), "{:?}", plan.notes());
	engine.apply(plan)?;

	assert_eq!(
		read(dir.path())?,
		"export const page = \"Home\";\nexport const a = 10;\nexport const x = 0;\nexport const y = 0;\nexport const b = 20;\n"
	);
	Ok(())
}

#[test]
fn a_failing_formatter_should_leave_the_file_as_rendered_and_say_why() -> TestResult {
	let v1 = page("1.0.0", "export const page = 'MyPage'\n")?;
	let dir = project(".prettierrc")?;
	let engine = Engine::builder()
		.user_config(UserConfig::default())
		.formatter(Broken)
		.open(dir.path())?;

	let plan = engine.plan(&request(&v1, &named("Home")))?;

	assert_eq!(plan.notes()[0].path.as_str(), "src/pages/home.ts");
	assert_eq!(
		plan.notes()[0].message,
		"Prettier could not format this file: Unexpected token"
	);
	engine.apply(plan)?;
	assert_eq!(read(dir.path())?, "export const page = 'Home'\n");
	Ok(())
}

#[test]
fn project_tools_should_only_be_noted_until_the_project_is_trusted() -> TestResult {
	let v1 = page("1.0.0", "export const page = 'MyPage'\n")?;
	let dir = project("biome.json")?;
	let values = named("Home");
	let engine = |trusted: bool| {
		Engine::builder()
			.user_config(UserConfig::default())
			.project_tools(trusted)
			.open(dir.path())
	};

	let untrusted = engine(false)?.plan(&request(&v1, &values))?;
	assert!(
		untrusted.notes()[0]
			.message
			.starts_with("not formatted with Biome: cuilo runs a project's formatters and linters only once you trust it"),
		"{:?}",
		untrusted.notes()
	);

	let trusted = engine(true)?.plan(&request(&v1, &values))?;
	assert!(
		trusted.notes()[0]
			.message
			.starts_with("Biome is set up in this project but not installed"),
		"{:?}",
		trusted.notes()
	);
	assert_eq!(trusted.notes().len(), 1);
	Ok(())
}

#[cfg(unix)]
#[test]
fn trusted_project_tools_should_run_from_node_modules() -> TestResult {
	use std::os::unix::fs::PermissionsExt as _;

	let v1 = page("1.0.0", "export const page = 'MyPage'\n")?;
	let dir = project("biome.json")?;
	let bin = dir.path().join("node_modules/.bin");
	fs::create_dir_all(&bin)?;
	fs::write(
		bin.join("biome"),
		"#!/bin/sh\n[ \"$1 $2\" = \"check --write\" ] || exit 9\nsed \"s/'/\\\"/g\"\n",
	)?;
	fs::set_permissions(bin.join("biome"), fs::Permissions::from_mode(0o755))?;
	let engine = Engine::builder()
		.user_config(UserConfig::default())
		.project_tools(true)
		.open(dir.path())?;

	let plan = engine.plan(&request(&v1, &named("Home")))?;
	assert!(plan.notes().is_empty(), "{:?}", plan.notes());
	engine.apply(plan)?;

	assert_eq!(read(dir.path())?, "export const page = \"Home\"\n");
	Ok(())
}
