//! Next steps and opt-in wiring: selecting steps, applying their patches all or none, and updating them.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use cuilo_core::engine::UpdateRequest;
use cuilo_core::plan::{Action, OnConflict, Pick, Plan, ProblemKind, Selection, Validated};
use cuilo_core::{Engine, EngineError, MemoryFs, Pack, PieceRequest, RelPath, UserConfig, Value, Values};

type TestResult = Result<(), Box<dyn Error>>;

const INDEX: &str = "import { Hono } from \"hono\";\n\nconst app = new Hono();\n\nexport default app;\n";
const MOUNT: &str = "Mount `myPageRoutes` in `src/index.ts`.";

/// A pack whose `page` piece has a step for every variant, one per variant, conditional ones and, for Hono,
/// a wiring step that imports the page and inserts `mount` after the app.
fn pack_with(version: &str, mount: &str, extra_steps: &str) -> Result<Pack, Box<dyn Error>> {
	let piece = format!(
		r#"schema_version = 1
name = "page"
target = "src/pages"

[inputs.name]
type = "string"
example = "MyPage"

[inputs.tests]
type = "bool"
default = false

[[variants]]
name = "hono"
when = {{ framework = "hono" }}
files = "files/shared"

[[variants]]
name = "express"
when = {{ framework = "express" }}
files = "files/shared"

[[steps]]
text = "Restart the server."

[[steps]]
text = "{MOUNT}"
variants = ["hono"]

[[steps.patches]]
kind = "import"
file = "src/index.ts"
import = 'import {{ myPageRoutes }} from "./pages/my-page";'

[[steps.patches]]
kind = "anchor"
file = "src/index.ts"
anchor = "const app = new Hono()"
insert = '{mount}'

[[steps]]
text = "Mount `myPageRoutes` with Express."
variants = ["express"]

[[steps]]
text = "Run the tests with Vitest."
when = "stack.vitest"

[[steps]]
text = "Write tests for `MyPage`."
when = "tests"
{extra_steps}"#
	);
	let files = BTreeMap::from([
		(
			"pack.toml".to_owned(),
			format!("schema_version = 1\nname = \"site\"\nversion = \"{version}\"\n[permissions]\nwrite = [\"src/**\"]\n")
				.into_bytes(),
		),
		("pieces/page/piece.toml".to_owned(), piece.into_bytes()),
		(
			"pieces/page/files/shared/my-page.ts".to_owned(),
			b"export const myPageRoutes = [];\n".to_vec(),
		),
	]);
	Ok(Pack::from_files(files)?)
}

fn pack() -> Result<Pack, Box<dyn Error>> {
	pack_with("1.0.0", "app.route(\"/my-page\", myPageRoutes);", "")
}

/// A project on `framework` with the given files.
fn project(framework: &str, files: &[(&str, &str)]) -> Result<(Arc<MemoryFs>, Engine), Box<dyn Error>> {
	let fs = Arc::new(MemoryFs::new());
	fs.insert(
		&RelPath::new("cuilo.json")?,
		format!(r#"{{"stack": {{"framework": ["{framework}"]}}}}"#).as_str(),
	);
	for (path, text) in files {
		fs.insert(&RelPath::new(path)?, *text);
	}
	let engine = Engine::builder()
		.user_config(UserConfig::default())
		.open_fs(fs.clone())?;
	Ok((fs, engine))
}

fn hono() -> Result<(Arc<MemoryFs>, Engine), Box<dyn Error>> {
	project("hono", &[("src/index.ts", INDEX)])
}

fn values() -> Values {
	Values::from([("name".to_owned(), Value::String("Settings".to_owned()))])
}

fn request<'a>(pack: &'a Pack, inputs: &'a Values, wire: bool) -> PieceRequest<'a> {
	PieceRequest {
		pack,
		piece: "page",
		inputs,
		variant: None,
		target: None,
		granted: None,
		force: false,
		wire,
	}
}

fn read(fs: &MemoryFs, path: &str) -> Result<String, Box<dyn Error>> {
	let bytes = fs.files().remove(&RelPath::new(path)?).ok_or("no such file")?;
	Ok(String::from_utf8(bytes)?)
}

fn action(plan: &Plan<Validated>, path: &str) -> Option<Action> {
	plan
		.changes()
		.iter()
		.find(|change| change.path.as_str() == path)
		.map(|change| change.action)
}

#[test]
fn steps_should_be_chosen_by_variant_and_condition() -> TestResult {
	let pack = pack()?;
	let (_fs, engine) = project("express", &[])?;
	let mut inputs = values();
	inputs.insert("tests".to_owned(), Value::Bool(true));

	let plan = engine.plan(&request(&pack, &inputs, false))?;

	assert_eq!(
		plan.next_steps(),
		[
			"Restart the server.",
			"Mount `settingsRoutes` with Express.",
			"Write tests for `Settings`."
		]
	);
	Ok(())
}

#[test]
fn wiring_off_should_list_wiring_steps_and_patch_nothing() -> TestResult {
	let pack = pack()?;
	let (fs, engine) = hono()?;

	let plan = engine.plan(&request(&pack, &values(), false))?;

	assert_eq!(
		plan.next_steps(),
		["Restart the server.", "Mount `settingsRoutes` in `src/index.ts`."]
	);
	assert_eq!(action(&plan, "src/index.ts"), None);
	assert!(plan.notes().is_empty(), "{:?}", plan.notes());
	engine.apply(plan)?;
	assert_eq!(read(&fs, "src/index.ts")?, INDEX);
	assert!(!engine.lockfile()?.instances[0].wired);
	Ok(())
}

#[test]
fn wiring_on_should_patch_with_examples_replaced_and_drop_the_step() -> TestResult {
	let pack = pack()?;
	let (fs, engine) = hono()?;

	let plan = engine.plan(&request(&pack, &values(), true))?;

	assert_eq!(plan.next_steps(), ["Restart the server."]);
	assert_eq!(action(&plan, "src/index.ts"), Some(Action::Modify));
	engine.apply(plan)?;
	assert_eq!(
		read(&fs, "src/index.ts")?,
		"import { Hono } from \"hono\";\nimport { settingsRoutes } from \"./pages/settings\";\n\nconst app = new Hono();\napp.route(\"/settings\", settingsRoutes);\n\nexport default app;\n"
	);
	let lock = engine.lockfile()?;
	assert!(lock.instances[0].wired);
	assert_eq!(lock.instances[0].patches.len(), 2);
	Ok(())
}

#[test]
fn a_step_with_a_patch_that_cannot_apply_should_apply_none_and_say_why() -> TestResult {
	let pack = pack()?;
	let app = "import { Hono } from \"hono\";\n\nconst app = new Hono<{ Bindings: Env }>();\n";
	let (fs, engine) = project("hono", &[("src/index.ts", app)])?;

	let plan = engine.plan(&request(&pack, &values(), true))?;

	assert_eq!(
		plan.next_steps(),
		["Restart the server.", "Mount `settingsRoutes` in `src/index.ts`."]
	);
	assert_eq!(action(&plan, "src/index.ts"), None);
	assert_eq!(plan.notes().len(), 1);
	assert_eq!(plan.notes()[0].path.as_str(), "src/index.ts");
	assert_eq!(
		plan.notes()[0].message,
		"no line contains `const app = new Hono()`; wire it by hand"
	);
	engine.apply(plan)?;
	assert_eq!(read(&fs, "src/index.ts")?, app);
	assert!(!engine.lockfile()?.instances[0].wired);
	Ok(())
}

#[test]
fn a_step_whose_file_is_missing_should_be_listed_with_a_note() -> TestResult {
	let pack = pack()?;
	let (_fs, engine) = project("hono", &[])?;

	let plan = engine.plan(&request(&pack, &values(), true))?;

	assert!(
		plan
			.next_steps()
			.contains(&"Mount `settingsRoutes` in `src/index.ts`.".to_owned())
	);
	assert_eq!(plan.notes()[0].message, "the file does not exist; wire it by hand");
	Ok(())
}

#[test]
fn an_ambiguous_anchor_should_leave_the_step_to_the_user() -> TestResult {
	let pack = pack()?;
	let app = "const app = new Hono();\nconst app = new Hono();\n";
	let (_fs, engine) = project("hono", &[("src/index.ts", app)])?;

	let plan = engine.plan(&request(&pack, &values(), true))?;

	assert!(
		plan.notes()[0]
			.message
			.starts_with("2 lines contain `const app = new Hono()`")
	);
	assert_eq!(action(&plan, "src/index.ts"), None);
	Ok(())
}

#[test]
fn wiring_should_not_apply_when_the_piece_goes_elsewhere() -> TestResult {
	let pack = pack()?;
	let (_fs, engine) = hono()?;
	let elsewhere = RelPath::new("src/features")?;
	let inputs = values();

	let plan = engine.plan(&PieceRequest {
		target: Some(&elsewhere),
		..request(&pack, &inputs, true)
	})?;

	assert_eq!(
		plan.next_steps(),
		["Restart the server.".to_owned(), MOUNT.replace("myPage", "settings")]
	);
	assert!(plan.notes()[0].message.contains("not in its default folder"));
	assert_eq!(action(&plan, "src/index.ts"), None);
	Ok(())
}

#[test]
fn wiring_patches_should_need_the_packs_write_permission() -> TestResult {
	let mount = "app.route(\"/my-page\", myPageRoutes);";
	let outside = "\n[[steps]]\ntext = \"Add the page to the docs.\"\n[[steps.patches]]\nkind = \"anchor\"\nfile = \"docs/pages.md\"\nanchor = \"# Pages\"\ninsert = \"- MyPage\"\n";
	let pack = pack_with("1.0.0", mount, outside)?;
	let (_fs, engine) = project("hono", &[("src/index.ts", INDEX), ("docs/pages.md", "# Pages\n")])?;

	let result = engine.plan(&request(&pack, &values(), true));

	let Err(EngineError::Rejected(rejected)) = result else {
		return Err(format!("expected a rejected plan, got {result:?}").into());
	};
	assert_eq!(rejected.problems[0].path.as_str(), "docs/pages.md");
	assert_eq!(rejected.problems[0].kind, ProblemKind::NotDeclared);
	Ok(())
}

#[test]
fn adopt_should_list_no_steps_and_wire_nothing() -> TestResult {
	let pack = pack()?;
	let (_fs, engine) = hono()?;

	let plan = engine.plan_adopt(&request(&pack, &values(), true))?;

	assert!(plan.next_steps().is_empty(), "{:?}", plan.next_steps());
	assert_eq!(action(&plan, "src/index.ts"), None);
	assert!(!plan.instances()[0].wired);
	Ok(())
}

#[test]
fn report_should_carry_next_steps_only_when_there_are_some() -> TestResult {
	let pack = pack()?;
	let (_fs, engine) = hono()?;
	let stepless = Pack::from_files(BTreeMap::from([
		(
			"pack.toml".to_owned(),
			b"schema_version = 1\nname = \"plain\"\nversion = \"1.0.0\"\n[permissions]\nwrite = [\"src/**\"]\n".to_vec(),
		),
		(
			"pieces/page/piece.toml".to_owned(),
			b"schema_version = 1\nname = \"page\"\ntarget = \"src\"\n".to_vec(),
		),
		("pieces/page/files/page.ts".to_owned(), b"x\n".to_vec()),
	]))?;

	let listed = serde_json::to_value(engine.plan(&request(&pack, &values(), false))?.report())?;
	let empty = serde_json::to_value(engine.plan(&request(&stepless, &Values::new(), false))?.report())?;

	assert_eq!(
		listed["next_steps"],
		serde_json::json!(["Restart the server.", "Mount `settingsRoutes` in `src/index.ts`."])
	);
	assert!(empty.get("next_steps").is_none(), "{empty}");
	Ok(())
}

#[test]
fn skipping_the_wired_file_in_review_should_list_its_step_again() -> TestResult {
	let pack = pack()?;
	let (fs, engine) = hono()?;
	let mut plan = engine.plan(&request(&pack, &values(), true))?;

	plan.select(&Selection::from([(RelPath::new("src/index.ts")?, Pick::None)]))?;

	assert_eq!(
		plan.next_steps(),
		["Restart the server.", "Mount `settingsRoutes` in `src/index.ts`."]
	);
	assert!(!plan.instances()[0].wired);
	engine.apply(plan)?;
	assert_eq!(read(&fs, "src/index.ts")?, INDEX);
	Ok(())
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

#[test]
fn update_should_never_wire_an_instance_added_without_wiring() -> TestResult {
	let old = pack()?;
	let new = pack_with("1.1.0", "app.route(\"/my-page\", myPageRoutes, { strict: true });", "")?;
	let (fs, engine) = hono()?;
	engine.apply(engine.plan(&request(&old, &values(), false))?)?;

	let plan = engine.plan_update(&update(&new), &[old][..])?;

	assert_eq!(action(&plan, "src/index.ts"), None);
	assert!(plan.next_steps().is_empty(), "{:?}", plan.next_steps());
	engine.apply(plan)?;
	assert_eq!(read(&fs, "src/index.ts")?, INDEX);
	assert!(!engine.lockfile()?.instances[0].wired);
	Ok(())
}

#[test]
fn update_should_keep_the_wiring_of_a_wired_instance_up_to_date() -> TestResult {
	let old = pack()?;
	let new = pack_with("1.1.0", "app.route(\"/my-page\", myPageRoutes, { strict: true });", "")?;
	let (fs, engine) = hono()?;
	engine.apply(engine.plan(&request(&old, &values(), true))?)?;

	let plan = engine.plan_update(&update(&new), &[old][..])?;
	assert!(plan.notes().is_empty(), "{:?}", plan.notes());
	engine.apply(plan)?;

	let text = read(&fs, "src/index.ts")?;
	assert!(
		text.contains("app.route(\"/settings\", settingsRoutes, { strict: true });\n"),
		"{text}"
	);
	assert!(!text.contains("settingsRoutes);"), "{text}");
	assert_eq!(text.matches("import { settingsRoutes }").count(), 1, "{text}");
	let lock = engine.lockfile()?;
	assert!(lock.instances[0].wired);
	assert_eq!(lock.instances[0].patches.len(), 2);
	Ok(())
}

#[test]
fn update_should_leave_a_file_whose_wiring_no_longer_applies() -> TestResult {
	let old = pack()?;
	let new = pack_with("1.1.0", "app.route(\"/my-page\", myPageRoutes, { strict: true });", "")?;
	let (fs, engine) = hono()?;
	engine.apply(engine.plan(&request(&old, &values(), true))?)?;
	let wired = read(&fs, "src/index.ts")?;
	let edited = wired.replace("new Hono()", "new Hono<{ Bindings: Env }>()");
	fs.insert(&RelPath::new("src/index.ts")?, edited.as_str());

	let plan = engine.plan_update(&update(&new), &[old][..])?;

	assert_eq!(action(&plan, "src/index.ts"), None);
	assert_eq!(
		plan.notes()[0].message,
		"no line contains `const app = new Hono()`, so this file's patches were not updated"
	);
	let records = plan.instances()[0].patches.len();
	engine.apply(plan)?;
	assert_eq!(read(&fs, "src/index.ts")?, edited);
	assert_eq!(records, 2);
	assert!(engine.lockfile()?.instances[0].wired);
	Ok(())
}
