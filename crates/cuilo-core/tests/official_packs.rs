//! Renders every piece of the official packs in `packs/`, in every variant, and wires them into
//! conventional entry files.

use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cuilo_core::model::{Input, Value};
use cuilo_core::{Engine, MemoryFs, Pack, PieceRequest, RelPath, UserConfig, Values};

type TestResult = Result<(), Box<dyn Error>>;

/// Input values that share no word with any template example.
const INPUTS: [(&str, &str); 5] = [
	("name", "StatusBadge"),
	("plural", "StatusBadges"),
	("app", "Shop"),
	("package", "dev.acme.shop"),
	("module", "github.com/acme/shop"),
];

/// Lowercase spellings of an example that must not survive rendering, e.g. `requestid`, `request-id`.
fn spellings(example: &str) -> Vec<String> {
	let mut words: Vec<String> = Vec::new();
	for c in example.chars() {
		if !c.is_alphanumeric() {
			words.push(String::new());
		} else if c.is_uppercase() || words.is_empty() {
			words.push(c.to_lowercase().collect());
		} else if let Some(word) = words.last_mut() {
			word.push(c);
		}
	}
	words.retain(|word| !word.is_empty());
	["", "-", "_", " "]
		.iter()
		.map(|separator| words.join(separator))
		.collect()
}

fn packs_dir() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs")
}

fn pack(name: &str) -> Result<Pack, Box<dyn Error>> {
	Ok(Pack::load(&packs_dir().join(name))?)
}

/// The test inputs `piece` declares.
fn inputs(pack: &Pack, piece: &str) -> Result<Values, Box<dyn Error>> {
	let declared = &pack.piece(piece).ok_or("missing piece")?.manifest().inputs;
	let mut inputs: Values = INPUTS
		.iter()
		.filter(|(name, _)| declared.contains_key(*name))
		.map(|(name, value)| ((*name).to_owned(), Value::String((*value).to_owned())))
		.collect();
	if declared.contains_key("tests") {
		inputs.insert("tests".to_owned(), Value::Bool(true));
	}
	Ok(inputs)
}

/// A project with the given stack and files.
fn project(stack: &str, files: &[(&str, &str)]) -> Result<(Arc<MemoryFs>, Engine), Box<dyn Error>> {
	let fs = Arc::new(MemoryFs::new());
	fs.insert(
		&RelPath::new("cuilo.json")?,
		format!(r#"{{"stack": {stack}}}"#).as_str(),
	);
	for (path, text) in files {
		fs.insert(&RelPath::new(path)?, *text);
	}
	let engine = Engine::builder()
		.user_config(UserConfig::default())
		.open_fs(fs.clone())?;
	Ok((fs, engine))
}

/// Renders one variant into an empty project with the given stack and returns its files and next steps.
fn render(pack: &Pack, piece: &str, variant: &str, stack: &str) -> Result<Rendered, Box<dyn Error>> {
	let (fs, engine) = project(stack, &[])?;
	let inputs = inputs(pack, piece)?;

	let plan = engine.plan(&PieceRequest {
		pack,
		piece,
		inputs: &inputs,
		variant: Some(variant),
		target: None,
		granted: None,
		force: false,
		wire: false,
	})?;
	let next_steps = plan.next_steps().to_vec();
	engine.apply(plan)?;

	let files = fs
		.files()
		.into_iter()
		.map(|(path, bytes)| (path.to_string(), String::from_utf8_lossy(&bytes).into_owned()))
		.filter(|(path, _)| path != "cuilo.json" && path != "cuilo.lock" && !path.starts_with(".cuilo/"))
		.collect();
	Ok((files, next_steps))
}

/// Written files as `(path, contents)`, and the plan's next steps.
type Rendered = (Vec<(String, String)>, Vec<String>);

/// Every piece and variant of `name`, rendered with each stack, as a list of written paths.
fn rendered_paths(name: &str, stacks: &[(&str, &str)]) -> Result<String, Box<dyn Error>> {
	let pack = pack(name)?;
	let mut listing = String::new();
	for piece in pack.pieces() {
		let manifest = piece.manifest();
		let leftovers: Vec<String> = manifest
			.inputs
			.values()
			.filter_map(|input| match input {
				Input::String {
					example: Some(example), ..
				} => Some(spellings(example)),
				_ => None,
			})
			.flatten()
			.chain(["example".to_owned()])
			.collect();
		for variant in &manifest.variants {
			for (label, stack) in stacks {
				let (files, next_steps) = render(&pack, manifest.name.as_str(), variant.name.as_str(), stack)?;
				writeln!(listing, "{} · {}{label}", manifest.name, variant.name)?;
				let steps = next_steps.iter().map(|step| ("next step", step.as_str()));
				for (path, contents) in files
					.iter()
					.map(|(path, contents)| (path.as_str(), contents.as_str()))
					.chain(steps)
				{
					if path == "next step" {
						writeln!(listing, "  → {contents}")?;
					} else {
						writeln!(listing, "  {path}")?;
					}
					let text = format!("{path}\n{contents}").to_lowercase();
					let leftover = leftovers.iter().find(|spelling| text.contains(spelling.as_str()));
					assert!(
						leftover.is_none(),
						"`{path}` of {} · {} still has {leftover:?}",
						manifest.name,
						variant.name
					);
					assert!(!contents.contains("@cuilo"), "`{path}` still has a directive");
				}
			}
		}
	}
	Ok(listing)
}

#[test]
fn ui_pack_should_render_every_variant_with_and_without_tailwind() -> TestResult {
	let listing = rendered_paths("ui", &[("", "{}"), (" (tailwind)", r#"{"styling": ["tailwind"]}"#)])?;

	insta::assert_snapshot!(listing);
	Ok(())
}

#[test]
fn backend_pack_should_render_every_variant_with_and_without_an_orm() -> TestResult {
	let orms = r#"{"database": ["drizzle", "prisma", "sqlx", "sqlalchemy", "gorm"]}"#;
	let listing = rendered_paths("backend", &[("", "{}"), (" (orm)", orms)])?;

	insta::assert_snapshot!(listing);
	Ok(())
}

/// A Hono entry file as `create-hono` writes it for Bun and Cloudflare Workers.
const HONO_INDEX: &str = "import { Hono } from 'hono'\n\nconst app = new Hono()\n\napp.get('/', (c) => {\n  return c.text('Hello Hono!')\n})\n\nexport default app\n";

/// An entry file as the `FastAPI` tutorial writes it.
const FASTAPI_MAIN: &str = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/\")\nasync def root():\n    return {\"message\": \"Hello World\"}\n";

/// An axum entry file as its README writes it.
const AXUM_MAIN: &str = "use axum::{Router, routing::get};\n\n#[tokio::main]\nasync fn main() {\n    let app = Router::new().route(\"/\", get(|| async { \"Hello, World!\" }));\n    let listener = tokio::net::TcpListener::bind(\"0.0.0.0:3000\").await.unwrap();\n    axum::serve(listener, app).await.unwrap();\n}\n";

/// Adds `resource` (with the `error` piece it requires) and then `middleware` with wiring on, and returns the
/// patched entry file and every next step left.
fn wired(stack: &str, entry: &str, original: &str) -> Result<String, Box<dyn Error>> {
	let pack = pack("backend")?;
	let (fs, engine) = project(stack, &[(entry, original)])?;
	let mut listing = String::new();
	for piece in ["resource", "middleware"] {
		let inputs = inputs(&pack, piece)?;
		let plan = engine.plan(&PieceRequest {
			pack: &pack,
			piece,
			inputs: &inputs,
			variant: None,
			target: None,
			granted: None,
			force: false,
			wire: true,
		})?;
		for step in plan.next_steps() {
			writeln!(listing, "→ {step}")?;
		}
		assert!(plan.notes().is_empty(), "{:?}", plan.notes());
		engine.apply(plan)?;
	}
	let patched = fs.files().remove(&RelPath::new(entry)?).ok_or("entry file is gone")?;
	Ok(format!("{listing}---\n{}", String::from_utf8_lossy(&patched)))
}

#[test]
fn backend_pack_should_wire_hono_into_its_entry_file() -> TestResult {
	insta::assert_snapshot!(wired(
		r#"{"framework": ["hono"], "language": ["typescript"]}"#,
		"src/index.ts",
		HONO_INDEX
	)?);
	Ok(())
}

#[test]
fn backend_pack_should_wire_fastapi_into_its_entry_file() -> TestResult {
	insta::assert_snapshot!(wired(r#"{"framework": ["fastapi"]}"#, "app/main.py", FASTAPI_MAIN)?);
	Ok(())
}

#[test]
fn backend_pack_should_wire_axum_into_its_entry_file() -> TestResult {
	insta::assert_snapshot!(wired(r#"{"framework": ["axum"]}"#, "src/main.rs", AXUM_MAIN)?);
	Ok(())
}
