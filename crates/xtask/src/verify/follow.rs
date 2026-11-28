//! The next steps cuilo lists but does not apply, followed by hand as a user would, so the checks see the pieces
//! compiled and in use.

use super::Project;

/// Which next steps a stack follows after `cuilo add`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Follow {
	/// None: the checks see every generated file as it is.
	Nothing,
	/// Declares Rust UI components from the library crate, `src/lib.rs`, so they compile and are public.
	RustUi,
	/// Declares Axum modules, connects the database and mounts the routes and middleware in `src/main.rs`.
	Axum,
	/// Adds the resource's model to `prisma/schema.prisma` and generates the Prisma client.
	Prisma,
	/// Adds Robolectric and Compose's test rule to the unit tests of the Android app, `app`, with its resources.
	Robolectric,
	/// Adds Compose's UI test library and the desktop runtime to the `jvmTest` dependencies of this Gradle module.
	ComposeUiTest(&'static str),
}

/// The piece `cuilo add` just wrote, with the inputs it got.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Added<'a> {
	/// The piece's name, e.g. `resource`.
	pub(crate) piece: &'a str,
	/// The folder it writes to, e.g. `src/ui/atoms`.
	pub(crate) target: &'a str,
	/// Its `name` input, e.g. `blog-post`.
	pub(crate) name: Option<&'a str>,
	/// Its `plural` input, e.g. `blog-posts`.
	pub(crate) plural: Option<&'a str>,
}

impl Follow {
	/// Follows the next steps of `added` in `project`.
	pub(crate) fn apply(self, added: &Added<'_>, project: &mut Project) -> Result<(), String> {
		match self {
			Self::Nothing => Ok(()),
			Self::RustUi => rust_ui(added, project),
			Self::Axum => axum(added, project),
			Self::Prisma => prisma(added, project),
			Self::Robolectric => robolectric(project),
			Self::ComposeUiTest(module) => compose_ui_test(module, project),
		}
	}

	/// Whether following adds versions to the Gradle version catalog, which `--latest` then moves to their newest
	/// releases like the fixture's own.
	pub(crate) fn pins_versions(self) -> bool {
		self == Self::Robolectric
	}
}

fn input<'a>(value: Option<&'a str>, input: &str, piece: &str) -> Result<&'a str, String> {
	value.ok_or_else(|| format!("`{piece}` got no `{input}` input"))
}

/// `pub mod <name>;` in `src/lib.rs` and in each `mod.rs` down to the component, e.g. `pub mod ui;`, `pub mod atoms;`
/// and `pub mod status_badge;` for `src/ui/atoms/status_badge.rs`.
fn rust_ui(added: &Added<'_>, project: &mut Project) -> Result<(), String> {
	let component = snake(input(added.name, "name", added.piece)?);
	let folder = added
		.target
		.strip_prefix("src/")
		.ok_or_else(|| format!("`{}` writes to {}, outside `src/`", added.piece, added.target))?;
	let mut parent = "src/lib.rs".to_owned();
	let mut path = String::from("src");
	for module in folder.split('/').chain([component.as_str()]) {
		project.edit(&parent, &format!("declare `pub mod {module};`"), |text| {
			Ok(declare(text, &format!("pub mod {module};")))
		})?;
		path = format!("{path}/{module}");
		parent = format!("{path}/mod.rs");
	}
	Ok(())
}

/// `text` with `line` appended, unless it has it already.
fn declare(mut text: String, line: &str) -> String {
	if !text.lines().any(|existing| existing.trim() == line) {
		if !text.is_empty() && !text.ends_with('\n') {
			text.push('\n');
		}
		text.push_str(line);
		text.push('\n');
	}
	text
}

const MAIN: &str = "src/main.rs";
/// How the fixture's `src/main.rs` starts the router.
const ROUTER: &str = "Router::new()";
/// The pool `database::connect()` gives before a repository uses it.
const UNUSED_POOL: &str = "let _pool = ";

fn axum(added: &Added<'_>, project: &mut Project) -> Result<(), String> {
	match added.piece {
		"database" => {
			project.run(&[
				"cargo",
				"add",
				"sqlx",
				"--features",
				"runtime-tokio,tls-rustls,postgres,uuid,chrono",
			])?;
			project.edit(MAIN, "connect with `database::connect().await`", |text| {
				insert_after(
					&text,
					"async fn main() {\n",
					&format!("    {UNUSED_POOL}database::connect().await.unwrap();\n"),
				)
			})
		}
		"middleware" => {
			let module = snake(input(added.name, "name", added.piece)?);
			project.edit(
				"src/middleware/mod.rs",
				&format!("declare `pub mod {module};`"),
				|text| Ok(declare(text, &format!("pub mod {module};"))),
			)?;
			project.edit(MAIN, "declare `mod middleware;` and add the layer", |text| {
				let text = if text.contains("mod middleware;") {
					text
				} else {
					insert_before(&text, "#[tokio::main]", "mod middleware;\n\n")?
				};
				let layer = format!(".layer(axum::middleware::from_fn(middleware::{module}::{module}))");
				end_router(&text, &layer)
			})
		}
		"resource" => {
			let item = pascal(input(added.name, "name", added.piece)?);
			let plural = input(added.plural, "plural", added.piece)?;
			let module = snake(plural);
			project.edit(MAIN, "nest the routes", |text| {
				let (text, repository) = if text.contains(UNUSED_POOL) {
					(
						text.replacen(UNUSED_POOL, "let pool = ", 1),
						format!("{module}::Pg{item}Repository::new(pool)"),
					)
				} else {
					(text, format!("{module}::InMemory{item}Repository::default()"))
				};
				let nest = format!(
					".nest(\"/{}\", {module}::routes(std::sync::Arc::new({module}::{item}Service::new({repository}))))",
					kebab(plural)
				);
				insert_after(&text, ROUTER, &nest)
			})
		}
		_ => Ok(()),
	}
}

fn prisma(added: &Added<'_>, project: &mut Project) -> Result<(), String> {
	if added.piece == "resource" {
		let model = pascal(input(added.name, "name", added.piece)?);
		project.edit("prisma/schema.prisma", &format!("add `model {model}`"), |text| {
			Ok(format!(
				"{}\nmodel {model} {{\n  id        String   @id @default(uuid())\n  name      String\n  createdAt DateTime @default(now())\n  updatedAt DateTime @updatedAt\n}}\n",
				text.trim_end()
			))
		})?;
	}
	if matches!(added.piece, "database" | "resource") {
		project.run(&["bun", "run", "prisma", "generate"])?;
	}
	Ok(())
}

const CATALOG: &str = "gradle/libs.versions.toml";
/// The Robolectric release the next step's user would pin; `--latest` moves it.
const ROBOLECTRIC: &str = "4.18.1";

fn robolectric(project: &mut Project) -> Result<(), String> {
	project.edit(CATALOG, "add Robolectric", |text| {
		if text.contains("org.robolectric") {
			return Ok(text);
		}
		let text = insert_after(&text, "[versions]\n", &format!("robolectric = \"{ROBOLECTRIC}\"\n"))?;
		insert_after(
			&text,
			"[libraries]\n",
			"robolectric = { group = \"org.robolectric\", name = \"robolectric\", version.ref = \"robolectric\" }\n",
		)
	})?;
	project.edit(
		"app/build.gradle.kts",
		"run the unit tests with Robolectric and Compose's test rule",
		|text| {
			if text.contains("libs.robolectric") {
				return Ok(text);
			}
			let text = insert_after(
				&text,
				"    testImplementation(libs.junit)\n",
				"    testImplementation(libs.robolectric)\n    testImplementation(platform(libs.androidx.compose.bom))\n    testImplementation(libs.androidx.compose.ui.test.junit4)\n",
			)?;
			insert_after(
				&text,
				"    buildFeatures {\n        compose = true\n    }\n",
				"    testOptions {\n        unitTests {\n            isIncludeAndroidResources = true\n        }\n    }\n",
			)
		},
	)
}

fn compose_ui_test(module: &str, project: &mut Project) -> Result<(), String> {
	project.edit(CATALOG, "add Compose's UI test library", |text| {
		if text.contains("org.jetbrains.compose.ui:ui-test\"") {
			return Ok(text);
		}
		insert_after(
			&text,
			"[libraries]\n",
			"compose-uiTest = { module = \"org.jetbrains.compose.ui:ui-test\", version.ref = \"composeMultiplatform\" }\n",
		)
	})?;
	project.edit(
		&format!("{module}/build.gradle.kts"),
		"run the jvmTest tests with Compose's UI test library",
		|text| {
			if text.contains("libs.compose.uiTest") {
				return Ok(text);
			}
			insert_after(
				&text,
				"        commonTest.dependencies {\n            implementation(libs.kotlin.test)\n        }\n",
				"        jvmTest.dependencies {\n            implementation(libs.compose.uiTest)\n            implementation(compose.desktop.currentOs)\n        }\n",
			)
		},
	)
}

fn insert_after(text: &str, anchor: &str, insert: &str) -> Result<String, String> {
	let (before, after) = text
		.split_once(anchor)
		.ok_or_else(|| format!("no `{}` to insert after", anchor.trim()))?;
	Ok(format!("{before}{anchor}{insert}{after}"))
}

fn insert_before(text: &str, anchor: &str, insert: &str) -> Result<String, String> {
	let (before, after) = text
		.split_once(anchor)
		.ok_or_else(|| format!("no `{}` to insert before", anchor.trim()))?;
	Ok(format!("{before}{insert}{anchor}{after}"))
}

/// `text` with `call` chained onto the end of the statement that builds the router.
fn end_router(text: &str, call: &str) -> Result<String, String> {
	let start = text.find(ROUTER).ok_or_else(|| format!("no `{ROUTER}` to add to"))?;
	let end = text[start..]
		.find(';')
		.map(|offset| start + offset)
		.ok_or("the router statement has no end")?;
	Ok(format!("{}{call}{}", &text[..end], &text[end..]))
}

/// The lowercase words of a name, e.g. `blog`, `post` for `blog-post` or `BlogPost`.
fn words(name: &str) -> Vec<String> {
	let mut words: Vec<String> = Vec::new();
	let mut previous_lower = false;
	for c in name.chars() {
		if !c.is_alphanumeric() {
			words.push(String::new());
			previous_lower = false;
			continue;
		}
		if words.is_empty() || (c.is_uppercase() && previous_lower) {
			words.push(String::new());
		}
		if let Some(word) = words.last_mut() {
			word.extend(c.to_lowercase());
		}
		previous_lower = c.is_lowercase() || c.is_ascii_digit();
	}
	words.retain(|word| !word.is_empty());
	words
}

fn snake(name: &str) -> String {
	words(name).join("_")
}

fn kebab(name: &str) -> String {
	words(name).join("-")
}

fn pascal(name: &str) -> String {
	words(name)
		.iter()
		.map(|word| {
			let mut chars = word.chars();
			chars
				.next()
				.map(|first| first.to_uppercase().chain(chars).collect::<String>())
				.unwrap_or_default()
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn names_should_convert_between_cases() {
		assert_eq!(snake("blog-post"), "blog_post");
		assert_eq!(snake("StatusBadge"), "status_badge");
		assert_eq!(kebab("BlogPosts"), "blog-posts");
		assert_eq!(pascal("request-id"), "RequestId");
	}

	#[test]
	fn declare_should_append_a_line_only_once() {
		let once = declare("mod app;".to_owned(), "pub mod ui;");

		assert_eq!(once, "mod app;\npub mod ui;\n");
		assert_eq!(declare(once.clone(), "pub mod ui;"), once);
	}

	#[test]
	fn end_router_should_chain_onto_the_router_statement() -> Result<(), String> {
		let main = "let app = Router::new().route(\"/\", get(root));\nlet other = 1;\n";

		assert_eq!(
			end_router(main, ".layer(x)")?,
			"let app = Router::new().route(\"/\", get(root)).layer(x);\nlet other = 1;\n"
		);
		Ok(())
	}
}
