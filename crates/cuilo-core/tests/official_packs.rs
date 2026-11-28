//! Renders every piece of the official packs in `packs/`, in every variant, and wires them into
//! conventional entry files.

use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cuilo_core::model::{Fact, Input, Locale, Value};
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

/// Every official pack in `packs/`.
fn official_packs() -> Result<Vec<Pack>, Box<dyn Error>> {
	let mut packs = Vec::new();
	for entry in std::fs::read_dir(packs_dir())? {
		packs.push(Pack::load(&entry?.path())?);
	}
	Ok(packs)
}

/// Adds `what` to `problems` unless it has an English description and a Mexican Spanish one that differs from it.
fn check_spanish(problems: &mut Vec<String>, what: &str, english: Option<&str>, spanish: Option<&str>) {
	match (english, spanish) {
		(None, _) => problems.push(format!("{what} has no description")),
		(Some(_), None) => problems.push(format!("{what} has no es-mx description")),
		(Some(english), Some(spanish)) if english == spanish => {
			problems.push(format!("{what} has an es-mx description that is still the English one"));
		}
		_ => {}
	}
}

#[test]
fn official_packs_should_describe_every_pack_piece_and_input_in_mexican_spanish() -> TestResult {
	let spanish = Locale::new("es-mx")?;
	let mut problems = Vec::new();
	for pack in official_packs()? {
		let manifest = pack.manifest();
		let translation = manifest.translations.get(&spanish);
		check_spanish(
			&mut problems,
			&format!("pack `{}`", manifest.name),
			manifest.description.as_deref(),
			translation.and_then(|translation| translation.description.as_deref()),
		);
		for piece in pack.pieces() {
			let piece = piece.manifest();
			let path = format!("{}/{}", manifest.name, piece.name);
			let translation = piece.translations.get(&spanish);
			check_spanish(
				&mut problems,
				&format!("piece `{path}`"),
				piece.description.as_deref(),
				translation.and_then(|translation| translation.description.as_deref()),
			);
			for (name, input) in &piece.inputs {
				check_spanish(
					&mut problems,
					&format!("input `{name}` of `{path}`"),
					input.description(),
					translation
						.and_then(|translation| translation.inputs.get(name))
						.map(String::as_str),
				);
			}
		}
	}

	assert!(problems.is_empty(), "{}", problems.join("\n"));
	Ok(())
}

#[test]
fn official_packs_should_declare_their_kinds() -> TestResult {
	for pack in official_packs()? {
		assert!(!pack.manifest().kinds.is_empty(), "pack `{}`", pack.manifest().name);
	}
	Ok(())
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

#[test]
fn every_official_template_should_resolve_its_relative_imports_in_each_import_style() -> TestResult {
	for name in ["ui", "backend"] {
		let pack = pack(name)?;
		for piece in pack.pieces() {
			let manifest = piece.manifest();
			for variant in &manifest.variants {
				for (style, extensions) in [("imports-js", &["js"][..]), ("imports-ts", &["ts", "tsx"][..])] {
					let (fs, engine) = project(&format!(r#"{{"import_style": ["{style}"]}}"#), &[])?;
					let inputs = inputs(&pack, manifest.name.as_str())?;
					let plan = engine.plan(&PieceRequest {
						pack: &pack,
						piece: manifest.name.as_str(),
						inputs: &inputs,
						variant: Some(variant.name.as_str()),
						target: None,
						granted: None,
						force: false,
						wire: false,
					})?;
					let label = format!("{name}/{} · {} ({style})", manifest.name, variant.name);
					let unresolved: Vec<String> = plan
						.notes()
						.iter()
						.filter(|note| note.message.contains("no file matches"))
						.map(|note| format!("{}: {}", note.path, note.message))
						.collect();
					assert_eq!(unresolved, Vec::<String>::new(), "{label}");
					engine.apply(plan)?;
					for (path, specifier) in relative_imports(&fs) {
						let rewritten = extensions.iter().any(|extension| has_extension(&specifier, extension));
						let kept = [".css", ".svelte", ".vue", ".astro", "?inline"]
							.iter()
							.any(|asset| specifier.ends_with(asset));
						assert!(rewritten || kept, "{label}: `{specifier}` in {path}");
					}
				}
			}
		}
	}
	Ok(())
}

/// An entry file as the `FastAPI` tutorial writes it.
const FASTAPI_MAIN: &str = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/\")\nasync def root():\n    return {\"message\": \"Hello World\"}\n";

/// An axum entry file as its README writes it.
const AXUM_MAIN: &str = "use axum::{Router, routing::get};\n\n#[tokio::main]\nasync fn main() {\n    let app = Router::new().route(\"/\", get(|| async { \"Hello, World!\" }));\n    let listener = tokio::net::TcpListener::bind(\"0.0.0.0:3000\").await.unwrap();\n    axum::serve(listener, app).await.unwrap();\n}\n";

/// Adds `resource` (with the `error` piece it requires) and then `middleware` to a project with `files`, wiring
/// them when `wire`, and returns every next step and note left, and the project.
fn add_backend(stack: &str, files: &[(&str, &str)], wire: bool) -> Result<(String, Arc<MemoryFs>), Box<dyn Error>> {
	add_backend_pieces(stack, files, wire, &["resource", "middleware"])
}

/// [`add_backend`] with the backend `pieces` given, added in order.
fn add_backend_pieces(
	stack: &str,
	files: &[(&str, &str)],
	wire: bool,
	pieces: &[&str],
) -> Result<(String, Arc<MemoryFs>), Box<dyn Error>> {
	let pack = pack("backend")?;
	let (fs, engine) = project(stack, files)?;
	let mut listing = String::new();
	for &piece in pieces {
		let inputs = inputs(&pack, piece)?;
		let plan = engine.plan(&PieceRequest {
			pack: &pack,
			piece,
			inputs: &inputs,
			variant: None,
			target: None,
			granted: None,
			force: false,
			wire,
		})?;
		for step in plan.next_steps() {
			writeln!(listing, "→ {step}")?;
		}
		for note in plan.notes() {
			writeln!(listing, "! {}: {}", note.path, note.message)?;
		}
		engine.apply(plan)?;
	}
	Ok((listing, fs))
}

/// [`add_backend`] with wiring on in a project with only `entry`, returning the entry file afterwards.
fn wired(stack: &str, entry: &str, original: &str) -> Result<(String, String), Box<dyn Error>> {
	let (listing, fs) = add_backend(stack, &[(entry, original)], true)?;
	let patched = fs.files().remove(&RelPath::new(entry)?).ok_or("entry file is gone")?;
	Ok((listing, String::from_utf8_lossy(&patched).into_owned()))
}

/// [`wired`] as one snapshot.
fn wired_snapshot(stack: &str, entry: &str, original: &str) -> Result<String, Box<dyn Error>> {
	let (listing, patched) = wired(stack, entry, original)?;
	Ok(format!("{listing}---\n{patched}"))
}

/// The files of create-hono's `template`, taken from `honojs/starter` at commit `c59d199`: its entry file, and
/// its `tsconfig.json` or `deno.json` when it has one.
fn create_hono_files(template: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("tests/fixtures/create-hono")
		.join(template);
	let mut files = Vec::new();
	let mut folders = vec![root.clone()];
	while let Some(folder) = folders.pop() {
		for entry in std::fs::read_dir(folder)? {
			let path = entry?.path();
			if path.is_dir() {
				folders.push(path);
			} else {
				let relative = path.strip_prefix(&root)?.to_string_lossy().replace('\\', "/");
				files.push((relative, std::fs::read_to_string(&path)?));
			}
		}
	}
	files.sort();
	Ok(files)
}

const HONO: &str = r#"{"framework": ["hono"], "language": ["typescript"]}"#;

/// [`add_backend`] on a copy of create-hono's `template`.
fn add_to_create_hono(template: &str, wire: bool) -> Result<(String, Arc<MemoryFs>), Box<dyn Error>> {
	let files = create_hono_files(template)?;
	let files: Vec<(&str, &str)> = files
		.iter()
		.map(|(path, text)| (path.as_str(), text.as_str()))
		.collect();
	add_backend(HONO, &files, wire)
}

/// Wires the backend pack into a copy of create-hono's `template`, checks that `entry` changed only if `wires`,
/// and returns the snapshot.
fn create_hono(template: &str, entry: &str, wires: bool) -> Result<String, Box<dyn Error>> {
	let original = create_hono_files(template)?
		.into_iter()
		.find(|(path, _)| path == entry)
		.map(|(_, text)| text)
		.ok_or("no such entry file")?;
	let (listing, fs) = add_to_create_hono(template, true)?;
	let patched = fs.files().remove(&RelPath::new(entry)?).ok_or("entry file is gone")?;
	let patched = String::from_utf8_lossy(&patched).into_owned();
	assert_eq!(patched != original, wires, "{template}: {listing}");
	Ok(format!("{template}: {entry}\n{listing}---\n{patched}"))
}

/// Whether `path` ends in `.<extension>`.
fn has_extension(path: &str, extension: &str) -> bool {
	Path::new(path).extension().is_some_and(|found| found == extension)
}

/// Every relative specifier in the project's TypeScript files, as `(file, specifier)`.
fn relative_imports(fs: &MemoryFs) -> Vec<(String, String)> {
	relative_imports_in(fs.files().into_iter().map(|(path, bytes)| (path.to_string(), bytes)))
}

/// Every relative specifier in the TypeScript files among `files`, as `(file, specifier)`.
fn relative_imports_in(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Vec<(String, String)> {
	let mut found = Vec::new();
	for (path, bytes) in files {
		if !has_extension(path.as_str(), "ts") && !has_extension(path.as_str(), "tsx") {
			continue;
		}
		let text = String::from_utf8_lossy(&bytes).into_owned();
		for opening in ["from \"", "from '", "import \"", "import '", "import(\""] {
			for (at, _) in text.match_indices(opening) {
				let rest = &text[at + opening.len()..];
				let specifier = rest.split(['"', '\'']).next().unwrap_or_default();
				if specifier.starts_with('.') {
					found.push((path.clone(), specifier.to_owned()));
				}
			}
		}
	}
	found
}

/// The import style the lockfile recorded for every instance.
fn recorded_styles(fs: &MemoryFs) -> Result<Vec<String>, Box<dyn Error>> {
	let lock = fs.files().remove(&RelPath::new("cuilo.lock")?).ok_or("no lockfile")?;
	let lock = cuilo_core::workspace::Lockfile::parse(&String::from_utf8_lossy(&lock))?;
	Ok(
		lock
			.instances
			.iter()
			.flat_map(|instance| {
				instance
					.stack
					.values(Fact::ImportStyle)
					.map(str::to_owned)
					.collect::<Vec<_>>()
			})
			.collect(),
	)
}

#[test]
fn create_hono_nodejs_and_vercel_should_import_with_js_extensions_wired_and_unwired() -> TestResult {
	for template in ["nodejs", "vercel"] {
		for wire in [true, false] {
			let (listing, fs) = add_to_create_hono(template, wire)?;
			let imports = relative_imports(&fs);

			assert!(imports.len() > 10, "{template}: {imports:?}");
			assert!(
				imports.iter().all(|(_, specifier)| has_extension(specifier, "js")),
				"{template} (wire: {wire}): {imports:?}"
			);
			assert!(!listing.contains("no file matches"), "{template}: {listing}");
			assert_eq!(recorded_styles(&fs)?, ["imports-js"; 3], "{template}");
		}
	}
	let (_, fs) = add_to_create_hono("nodejs", true)?;
	let mut listing = String::new();
	for (path, specifier) in relative_imports(&fs) {
		writeln!(listing, "{path}: {specifier}")?;
	}
	insta::assert_snapshot!(listing);
	Ok(())
}

/// Files as `(path, bytes)`.
#[cfg(unix)]
type Files = Vec<(String, Vec<u8>)>;

/// Every file below `dir`, by its path relative to `root`.
#[cfg(unix)]
fn disk_files(root: &Path, dir: &Path) -> Result<Files, Box<dyn Error>> {
	let mut files = Vec::new();
	let mut folders = vec![dir.to_path_buf()];
	while let Some(folder) = folders.pop() {
		for entry in std::fs::read_dir(folder)? {
			let path = entry?.path();
			if path.is_dir() {
				folders.push(path);
			} else {
				let relative = path.strip_prefix(root)?.to_string_lossy().into_owned();
				files.push((relative, std::fs::read(&path)?));
			}
		}
	}
	Ok(files)
}

#[cfg(unix)]
#[test]
fn backend_pack_should_import_with_js_extensions_in_a_node_project_whose_tsconfig_pnpm_links() -> TestResult {
	let dir = tempfile::tempdir()?;
	let root = dir.path();
	let store = "node_modules/.pnpm/@tsconfig+node22@22.0.2/node_modules/@tsconfig/node22";
	for (path, text) in [
		(
			format!("{store}/package.json"),
			r#"{"name": "@tsconfig/node22", "version": "22.0.2"}"#,
		),
		(
			format!("{store}/tsconfig.json"),
			r#"{"compilerOptions": {"module": "nodenext", "target": "es2022", "strict": true}}"#,
		),
		(
			"package.json".to_owned(),
			r#"{"type": "module", "devDependencies": {"@tsconfig/node22": "^22.0.2"}}"#,
		),
		("pnpm-lock.yaml".to_owned(), "lockfileVersion: '9.0'\n"),
		(
			"tsconfig.json".to_owned(),
			r#"{"extends": "@tsconfig/node22/tsconfig.json", "compilerOptions": {"outDir": "dist"}}"#,
		),
		("cuilo.json".to_owned(), &format!(r#"{{"stack": {HONO}}}"#)),
	] {
		let path = root.join(path);
		std::fs::create_dir_all(path.parent().ok_or("no parent")?)?;
		std::fs::write(path, text)?;
	}
	std::fs::create_dir(root.join("node_modules/@tsconfig"))?;
	std::os::unix::fs::symlink(
		"../.pnpm/@tsconfig+node22@22.0.2/node_modules/@tsconfig/node22",
		root.join("node_modules/@tsconfig/node22"),
	)?;
	let engine = Engine::builder().user_config(UserConfig::default()).open(root)?;
	let pack = pack("backend")?;

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
			wire: false,
		})?;
		assert!(
			plan
				.notes()
				.iter()
				.all(|note| !note.message.contains("no file matches")),
			"{:?}",
			plan.notes()
		);
		engine.apply(plan)?;
	}
	let imports = relative_imports_in(disk_files(root, &root.join("src"))?);

	assert!(imports.len() > 10, "{imports:?}");
	assert!(
		imports.iter().all(|(_, specifier)| has_extension(specifier, "js")),
		"{imports:?}"
	);
	assert_eq!(
		engine.project()?.stack.values(Fact::ImportStyle).collect::<Vec<_>>(),
		["imports-js"]
	);
	let recorded: Vec<String> = engine
		.lockfile()?
		.instances
		.iter()
		.flat_map(|instance| {
			instance
				.stack
				.values(Fact::ImportStyle)
				.map(str::to_owned)
				.collect::<Vec<_>>()
		})
		.collect();
	assert_eq!(recorded, ["imports-js"; 3]);
	Ok(())
}

#[test]
fn create_hono_bun_and_cloudflare_workers_should_keep_extensionless_imports() -> TestResult {
	for template in ["bun", "cloudflare-workers"] {
		let (_, fs) = add_to_create_hono(template, true)?;
		let imports = relative_imports(&fs);

		assert!(imports.len() > 10, "{template}: {imports:?}");
		assert!(
			imports
				.iter()
				.all(|(_, specifier)| !has_extension(specifier, "js") && !has_extension(specifier, "ts")),
			"{template}: {imports:?}"
		);
		assert_eq!(recorded_styles(&fs)?, ["imports-extensionless"; 3], "{template}");
	}
	Ok(())
}

#[test]
fn create_hono_deno_should_import_with_ts_extensions() -> TestResult {
	let (_, fs) = add_to_create_hono("deno", false)?;
	let imports = relative_imports(&fs);

	assert!(imports.len() > 10, "{imports:?}");
	assert!(
		imports.iter().all(|(_, specifier)| has_extension(specifier, "ts")),
		"{imports:?}"
	);
	assert!(imports.contains(&(
		"src/modules/status-badges/status-badge.service.ts".to_owned(),
		"../../shared/errors.ts".to_owned()
	)));
	assert_eq!(recorded_styles(&fs)?, ["imports-ts"; 3]);
	Ok(())
}

#[test]
fn a_configured_import_style_should_override_the_tsconfig() -> TestResult {
	let files = create_hono_files("nodejs")?;
	let files: Vec<(&str, &str)> = files
		.iter()
		.map(|(path, text)| (path.as_str(), text.as_str()))
		.collect();
	let (_, fs) = add_backend(
		r#"{"framework": ["hono"], "language": ["typescript"], "import_style": ["imports-extensionless"]}"#,
		&files,
		true,
	)?;

	assert!(
		relative_imports(&fs)
			.iter()
			.all(|(_, specifier)| !has_extension(specifier, "js")),
	);
	assert_eq!(recorded_styles(&fs)?, ["imports-extensionless"; 3]);
	Ok(())
}

#[test]
fn create_hono_aws_lambda_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("aws-lambda", "src/index.ts", true)?);
	Ok(())
}

#[test]
fn create_hono_bun_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("bun", "src/index.ts", true)?);
	Ok(())
}

#[test]
fn create_hono_cloudflare_pages_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("cloudflare-pages", "src/index.tsx", true)?);
	Ok(())
}

#[test]
fn create_hono_cloudflare_workers_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("cloudflare-workers", "src/index.ts", true)?);
	Ok(())
}

#[test]
fn create_hono_cloudflare_workers_vite_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("cloudflare-workers+vite", "src/index.tsx", true)?);
	Ok(())
}

#[test]
fn create_hono_deno_should_list_its_steps_since_its_entry_is_main_ts() -> TestResult {
	insta::assert_snapshot!(create_hono("deno", "main.ts", false)?);
	Ok(())
}

#[test]
fn create_hono_fastly_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("fastly", "src/index.ts", true)?);
	Ok(())
}

#[test]
fn create_hono_lambda_edge_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("lambda-edge", "src/index.ts", true)?);
	Ok(())
}

#[test]
fn create_hono_netlify_should_list_its_steps_since_its_entry_is_an_edge_function() -> TestResult {
	insta::assert_snapshot!(create_hono("netlify", "netlify/edge-functions/index.ts", false)?);
	Ok(())
}

#[test]
fn create_hono_nextjs_should_list_its_steps_since_its_entry_is_a_route_handler() -> TestResult {
	insta::assert_snapshot!(create_hono("nextjs", "app/api/[...route]/route.ts", false)?);
	Ok(())
}

#[test]
fn create_hono_nodejs_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("nodejs", "src/index.ts", true)?);
	Ok(())
}

#[test]
fn create_hono_vercel_should_be_wired() -> TestResult {
	insta::assert_snapshot!(create_hono("vercel", "src/index.ts", true)?);
	Ok(())
}

#[test]
fn create_hono_x_basic_should_list_its_steps_since_honox_builds_the_app() -> TestResult {
	insta::assert_snapshot!(create_hono("x-basic", "app/server.ts", false)?);
	Ok(())
}

#[test]
fn backend_pack_should_wire_fastapi_into_its_entry_file() -> TestResult {
	insta::assert_snapshot!(wired_snapshot(
		r#"{"framework": ["fastapi"]}"#,
		"app/main.py",
		FASTAPI_MAIN
	)?);
	Ok(())
}

#[test]
fn backend_pack_should_wire_axum_into_its_entry_file() -> TestResult {
	insta::assert_snapshot!(wired_snapshot(r#"{"framework": ["axum"]}"#, "src/main.rs", AXUM_MAIN)?);
	Ok(())
}

/// The Kotlin files `cuilo add ui/atom` writes to a Compose Multiplatform project with these Gradle modules, and
/// the folder `cuilo.lock` records for it.
fn compose_multiplatform_atom(modules: &[&str]) -> Result<(Vec<String>, String), Box<dyn Error>> {
	let pack = pack("ui")?;
	let builds: Vec<String> = modules
		.iter()
		.map(|module| format!("{module}/build.gradle.kts"))
		.collect();
	let files: Vec<(&str, &str)> = builds.iter().map(|path| (path.as_str(), "")).collect();
	let (fs, engine) = project(r#"{"framework": ["compose-multiplatform", "compose"]}"#, &files)?;
	let plan = engine.plan(&PieceRequest {
		pack: &pack,
		piece: "atom",
		inputs: &inputs(&pack, "atom")?,
		variant: None,
		target: None,
		granted: None,
		force: false,
		wire: false,
	})?;
	engine.apply(plan)?;

	let written = fs.files();
	let lock = String::from_utf8_lossy(written.get(&RelPath::new("cuilo.lock")?).ok_or("no cuilo.lock")?);
	let target = lock
		.lines()
		.find_map(|line| line.strip_prefix("target = "))
		.ok_or("no target in cuilo.lock")?
		.trim_matches('"')
		.to_owned();
	let kotlin = written
		.keys()
		.map(ToString::to_string)
		.filter(|path| has_extension(path, "kt"))
		.collect();
	Ok((kotlin, target))
}

#[test]
fn compose_multiplatform_pieces_should_go_to_shared_or_else_to_the_older_compose_app_module() -> TestResult {
	let into = |module: &str| {
		(
			vec![
				format!("{module}/src/commonMain/kotlin/dev/acme/shop/ui/atoms/StatusBadge.kt"),
				format!("{module}/src/jvmTest/kotlin/dev/acme/shop/ui/atoms/StatusBadgeTest.kt"),
			],
			module.to_owned(),
		)
	};

	assert_eq!(compose_multiplatform_atom(&["shared", "desktopApp"])?, into("shared"));
	assert_eq!(compose_multiplatform_atom(&["composeApp"])?, into("composeApp"));
	assert_eq!(compose_multiplatform_atom(&["composeApp", "shared"])?, into("shared"));
	assert_eq!(compose_multiplatform_atom(&[])?, into("shared"));
	Ok(())
}

/// The variant `cuilo add backend/database` picks in an empty project with `stack`.
fn database_variant(stack: &str) -> Result<String, Box<dyn Error>> {
	chosen_variant("backend", "database", stack)
}

/// The variant `cuilo add <pack>/<piece>` picks in an empty project with `stack`.
fn chosen_variant(pack_name: &str, piece: &str, stack: &str) -> Result<String, Box<dyn Error>> {
	let pack = pack(pack_name)?;
	let (_, engine) = project(stack, &[])?;
	let inputs = inputs(&pack, piece)?;
	let plan = engine.plan(&PieceRequest {
		pack: &pack,
		piece,
		inputs: &inputs,
		variant: None,
		target: None,
		granted: None,
		force: false,
		wire: false,
	})?;
	engine.apply(plan)?;
	let lock = engine.lockfile()?;
	let variant = lock.instances.first().and_then(|instance| instance.variant.as_ref());
	Ok(variant.ok_or("no variant recorded")?.to_string())
}

#[test]
fn database_should_pick_the_variant_of_the_project_s_framework_and_orm() -> TestResult {
	for (stack, expected) in [
		(
			r#"{"framework": ["hono"], "language": ["typescript"], "database": ["drizzle"]}"#,
			"hono-drizzle",
		),
		(
			r#"{"framework": ["hono"], "language": ["typescript"], "database": ["prisma"]}"#,
			"hono-prisma",
		),
		(
			r#"{"framework": ["hono"], "language": ["typescript"], "database": ["prisma", "drizzle"]}"#,
			"hono-drizzle",
		),
		(
			r#"{"framework": ["express"], "language": ["typescript"], "database": ["drizzle"]}"#,
			"express-drizzle",
		),
		(
			r#"{"framework": ["express"], "language": ["typescript"], "database": ["prisma"]}"#,
			"express-prisma",
		),
		(r#"{"framework": ["axum"], "database": ["sqlx"]}"#, "axum"),
		(r#"{"framework": ["fastapi"], "database": ["sqlalchemy"]}"#, "fastapi"),
		(r#"{"framework": ["chi"], "database": ["gorm"]}"#, "chi"),
		(r#"{"framework": ["echo", "echo-v4"], "database": ["gorm"]}"#, "echo"),
		(r#"{"framework": ["echo", "echo-v5"], "database": ["gorm"]}"#, "echo-v5"),
	] {
		assert_eq!(database_variant(stack)?, expected, "{stack}");
	}
	Ok(())
}

#[test]
fn database_should_refuse_a_project_without_a_supported_orm() -> TestResult {
	for stack in [
		r#"{"framework": ["hono"], "language": ["typescript"]}"#,
		r#"{"framework": ["axum"], "database": ["diesel"]}"#,
		r#"{"framework": ["chi"], "database": ["ent"]}"#,
	] {
		let error = database_variant(stack).err().ok_or("a variant matched")?;

		assert!(error.to_string().contains("no variant matches"), "{stack}: {error}");
	}
	Ok(())
}

#[test]
fn backend_and_ui_pieces_should_pick_the_variant_of_the_framework_s_major() -> TestResult {
	for piece in ["error", "middleware", "resource"] {
		assert_eq!(
			chosen_variant("backend", piece, r#"{"framework": ["echo", "echo-v4"]}"#)?,
			"echo"
		);
		assert_eq!(
			chosen_variant("backend", piece, r#"{"framework": ["echo", "echo-v5"]}"#)?,
			"echo-v5"
		);
	}
	for piece in ["atom", "molecule", "organism"] {
		let solid_2 = r#"{"framework": ["solid", "solid-v2"], "language": ["typescript"]}"#;
		assert_eq!(chosen_variant("ui", piece, solid_2)?, "solid");
	}
	Ok(())
}

#[test]
fn ui_pieces_should_refuse_solid_1_naming_the_major_they_need() -> TestResult {
	for piece in ["atom", "molecule", "organism"] {
		let solid_1 = r#"{"framework": ["solid", "solid-v1"], "language": ["typescript"]}"#;
		let error = chosen_variant("ui", piece, solid_1).err().ok_or("a variant matched")?;

		assert!(
			error
				.to_string()
				.contains("the variants for solid need solid-v2, and the project has solid-v1"),
			"{piece}: {error}"
		);
	}
	Ok(())
}

#[test]
fn prisma_resource_should_import_its_client_through_the_database_piece_in_each_import_style() -> TestResult {
	for (style, client, database) in [
		(
			"imports-js",
			"../generated/prisma/client.js",
			"../../shared/database.js",
		),
		(
			"imports-ts",
			"../generated/prisma/client.ts",
			"../../shared/database.ts",
		),
		(
			"imports-extensionless",
			"../generated/prisma/client",
			"../../shared/database",
		),
	] {
		let stack = format!(
			r#"{{"framework": ["hono"], "language": ["typescript"], "database": ["prisma"], "import_style": ["{style}"]}}"#
		);
		let (listing, fs) = add_backend_pieces(&stack, &[], false, &["database", "resource"])?;
		let imports = relative_imports(&fs);

		assert!(
			imports.contains(&("src/shared/database.ts".to_owned(), client.to_owned())),
			"{style}: {imports:?}"
		);
		assert!(
			imports.contains(&(
				"src/modules/status-badges/status-badge.prisma-repository.ts".to_owned(),
				database.to_owned()
			)),
			"{style}: {imports:?}"
		);
		assert!(!listing.contains("no file matches"), "{style}: {listing}");
	}
	Ok(())
}

#[test]
fn backend_pack_should_wire_the_database_and_an_sqlx_resource_into_axum() -> TestResult {
	let (listing, fs) = add_backend_pieces(
		r#"{"framework": ["axum"], "database": ["sqlx"]}"#,
		&[("src/main.rs", AXUM_MAIN)],
		true,
		&["database", "resource"],
	)?;
	let main = fs
		.files()
		.remove(&RelPath::new("src/main.rs")?)
		.ok_or("entry file is gone")?;

	insta::assert_snapshot!(format!("{listing}---\n{}", String::from_utf8_lossy(&main)));
	Ok(())
}
