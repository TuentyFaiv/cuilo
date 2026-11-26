//! Writes the relative imports of generated JavaScript and TypeScript in a project's [`ImportStyle`].

mod scan;

use crate::model::{ImportStyle, RelPath};

/// Extensions of the files whose imports are rewritten, and that a specifier may name.
const CODE_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Extensions of files a specifier names as written, so an import of one that is missing is no surprise.
const ASSET_EXTENSIONS: [&str; 39] = [
	"css", "scss", "sass", "less", "styl", "pcss", "json", "jsonc", "json5", "wasm", "svg", "png", "jpg", "jpeg", "gif",
	"webp", "avif", "ico", "bmp", "woff", "woff2", "ttf", "otf", "eot", "mp3", "mp4", "webm", "html", "md", "mdx", "txt",
	"yaml", "yml", "toml", "graphql", "gql", "vue", "svelte", "astro",
];

/// Relative imports rewritten in one file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Rewritten {
	/// The text with its imports rewritten.
	pub text: String,
	/// Relative specifiers that name no file, left as written.
	pub unresolved: Vec<String>,
}

/// Whether imports in the file at `path` are rewritten: a `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`,
/// `.mjs` or `.cjs` file.
pub(crate) fn is_code_file(path: &RelPath) -> bool {
	extension(path.file_name()).is_some_and(|extension| CODE_EXTENSIONS.contains(&extension))
}

/// Whether the file at `path` is `CommonJS` by its extension alone: `Some(true)` for `.cjs` and `.cts`,
/// `Some(false)` for `.mjs` and `.mts`, `None` when the nearest `package.json` decides.
pub(crate) fn commonjs_by_extension(path: &RelPath) -> Option<bool> {
	match extension(path.file_name()) {
		Some("cjs" | "cts") => Some(true),
		Some("mjs" | "mts") => Some(false),
		_ => None,
	}
}

/// Rewrites the relative specifiers in `code`, the text of or for the file at `file`, to `style`;
/// `require()` calls count when `commonjs`, and `exists` tells whether a project file exists. Lines with
/// `@cuilo` directives stay as they are.
pub(crate) fn rewrite(
	code: &str,
	file: &RelPath,
	style: ImportStyle,
	commonjs: bool,
	exists: &dyn Fn(&RelPath) -> bool,
) -> Rewritten {
	let mut rewritten = Rewritten {
		text: String::with_capacity(code.len()),
		unresolved: Vec::new(),
	};
	if style == ImportStyle::Extensionless {
		code.clone_into(&mut rewritten.text);
		return rewritten;
	}
	let dir = file.parent();
	let mut copied = 0;
	for range in scan::specifiers(code, commonjs) {
		if line_of(code, range.start).contains("@cuilo") {
			continue;
		}
		let specifier = &code[range.clone()];
		match adapt(specifier, dir.as_ref(), style, exists) {
			Adapted::Keep => {}
			Adapted::Unresolved => {
				if !rewritten.unresolved.iter().any(|known| known == specifier) {
					rewritten.unresolved.push(specifier.to_owned());
				}
			}
			Adapted::Replace(replacement) => {
				rewritten.text.push_str(&code[copied..range.start]);
				rewritten.text.push_str(&replacement);
				copied = range.end;
			}
		}
	}
	rewritten.text.push_str(&code[copied..]);
	rewritten
}

/// What becomes of one specifier.
#[derive(Debug, PartialEq, Eq)]
enum Adapted {
	/// Not relative, already has a code extension, or names a file that exists as written.
	Keep,
	/// Relative, but it names no file.
	Unresolved,
	/// The specifier in the style.
	Replace(String),
}

/// What becomes of `specifier`, imported by a file in `dir` (the root when `None`).
fn adapt(specifier: &str, dir: Option<&RelPath>, style: ImportStyle, exists: &dyn Fn(&RelPath) -> bool) -> Adapted {
	let relative = matches!(specifier, "." | "..") || specifier.starts_with("./") || specifier.starts_with("../");
	if !relative || specifier.contains(['?', '#', '\\']) {
		return Adapted::Keep;
	}
	let name = specifier.rsplit('/').next().unwrap_or(specifier);
	let named = extension(name);
	if named.is_some_and(|extension| CODE_EXTENSIONS.contains(&extension)) {
		return Adapted::Keep;
	}
	let folder = matches!(name, "" | "." | "..");
	if let Some(target) = RelPath::resolve(dir, specifier) {
		if !folder {
			if exists(&target) {
				return Adapted::Keep;
			}
			if let Some(found) = with_extension(&target, exists) {
				return Adapted::Replace(format!("{specifier}.{}", written(found, style)));
			}
		}
		if let Some(found) = index_in(&target, exists) {
			let base = specifier.trim_end_matches('/');
			return Adapted::Replace(format!("{base}/index.{}", written(found, style)));
		}
	} else if folder && let Some(found) = index_in_root(dir, specifier, exists) {
		let base = specifier.trim_end_matches('/');
		return Adapted::Replace(format!("{base}/index.{}", written(found, style)));
	}
	if named.is_some_and(|extension| ASSET_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())) {
		Adapted::Keep
	} else {
		Adapted::Unresolved
	}
}

/// The first code extension that, added to `target`, names a file.
fn with_extension(target: &RelPath, exists: &dyn Fn(&RelPath) -> bool) -> Option<&'static str> {
	CODE_EXTENSIONS
		.into_iter()
		.find(|extension| RelPath::new(&format!("{target}.{extension}")).is_ok_and(|candidate| exists(&candidate)))
}

/// The extension of the `index` file in the folder `dir`, if it has one.
fn index_in(dir: &RelPath, exists: &dyn Fn(&RelPath) -> bool) -> Option<&'static str> {
	CODE_EXTENSIONS
		.into_iter()
		.find(|extension| RelPath::new(&format!("{dir}/index.{extension}")).is_ok_and(|candidate| exists(&candidate)))
}

/// [`index_in`] for a specifier such as `.` or `../` that names the project root itself.
fn index_in_root(dir: Option<&RelPath>, specifier: &str, exists: &dyn Fn(&RelPath) -> bool) -> Option<&'static str> {
	CODE_EXTENSIONS.into_iter().find(|extension| {
		RelPath::resolve(dir, &format!("{}/index.{extension}", specifier.trim_end_matches('/')))
			.is_some_and(|candidate| exists(&candidate))
	})
}

/// The extension an import of a file with the extension `found` is written with.
fn written(found: &'static str, style: ImportStyle) -> &'static str {
	match (style, found) {
		(ImportStyle::RuntimeExtension, "mts" | "mjs") => "mjs",
		(ImportStyle::RuntimeExtension, "cts" | "cjs") => "cjs",
		(ImportStyle::RuntimeExtension, _) => "js",
		_ => found,
	}
}

/// The extension of a file name, without the dot.
fn extension(name: &str) -> Option<&str> {
	name
		.rsplit_once('.')
		.and_then(|(stem, extension)| (!stem.is_empty()).then_some(extension))
}

/// The line holding byte `at`.
fn line_of(code: &str, at: usize) -> &str {
	let start = code[..at].rfind('\n').map_or(0, |p| p + 1);
	let end = code[at..].find('\n').map_or(code.len(), |p| at + p);
	&code[start..end]
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeSet;

	use super::*;

	const FILES: [&str; 11] = [
		"src/modules/items/item.routes.ts",
		"src/modules/items/item.service.ts",
		"src/modules/items/item.view.tsx",
		"src/modules/items/item.worker.mts",
		"src/modules/items/item.config.cts",
		"src/modules/items/legacy.js",
		"src/modules/items/styles.css",
		"src/modules/items/badge.svelte",
		"src/shared/errors.ts",
		"src/shared/middleware/index.ts",
		"src/index.ts",
	];

	fn rewrite_in(file: &str, code: &str, style: ImportStyle, commonjs: bool) -> Rewritten {
		let files: BTreeSet<RelPath> = FILES.iter().map(|path| RelPath::new(path).unwrap()).collect();
		rewrite(code, &RelPath::new(file).unwrap(), style, commonjs, &|path| {
			files.contains(path)
		})
	}

	fn js(code: &str) -> String {
		rewrite_in(
			"src/modules/items/item.routes.ts",
			code,
			ImportStyle::RuntimeExtension,
			false,
		)
		.text
	}

	#[test]
	fn should_add_the_runtime_extension_to_every_statement_form() {
		let code = r#"import { a } from "./item.service";
import type { B } from './item.service';
import "./item.service";
export * from "./item.service";
export * as ns from "./item.service";
export { c } from "./item.service";
export type { D } from "./item.service";
const lazy = () => import("./item.service");
"#;

		assert_eq!(
			js(code),
			code
				.replace("item.service\"", "item.service.js\"")
				.replace("item.service'", "item.service.js'")
		);
	}

	#[test]
	fn should_read_dotted_base_names_as_extensionless() {
		assert_eq!(
			js("import { routes } from \"./item.routes\";\nimport { View } from \"./item.view\";\n"),
			"import { routes } from \"./item.routes.js\";\nimport { View } from \"./item.view.js\";\n"
		);
	}

	#[test]
	fn should_climb_folders_and_use_a_folders_index() {
		assert_eq!(
			js(
				"import { NotFound } from \"../../shared/errors\";\nimport { requestId } from \"../../shared/middleware\";\nimport { other } from \"../../shared/middleware/\";\n"
			),
			"import { NotFound } from \"../../shared/errors.js\";\nimport { requestId } from \"../../shared/middleware/index.js\";\nimport { other } from \"../../shared/middleware/index.js\";\n"
		);
	}

	#[test]
	fn should_write_mjs_and_cjs_for_mts_and_cts_files() {
		assert_eq!(
			js("import w from \"./item.worker\";\nimport c from \"./item.config\";\n"),
			"import w from \"./item.worker.mjs\";\nimport c from \"./item.config.cjs\";\n"
		);
	}

	#[test]
	fn should_write_each_files_own_extension_in_the_source_style() {
		let rewritten = rewrite_in(
			"src/modules/items/item.routes.ts",
			"import { a } from \"./item.service\";\nimport v from \"./item.view\";\nimport w from \"./item.worker\";\nimport l from \"./legacy\";\nimport m from \"../../shared/middleware\";\n",
			ImportStyle::SourceExtension,
			false,
		);

		assert_eq!(
			rewritten.text,
			"import { a } from \"./item.service.ts\";\nimport v from \"./item.view.tsx\";\nimport w from \"./item.worker.mts\";\nimport l from \"./legacy.js\";\nimport m from \"../../shared/middleware/index.ts\";\n"
		);
	}

	#[test]
	fn should_leave_extensions_assets_packages_and_aliases_alone() {
		let code = "import a from \"./item.service.js\";\nimport b from \"./item.service.ts\";\nimport \"./styles.css\";\nimport \"./missing.css\";\nimport Badge from \"./badge.svelte\";\nimport raw from \"./styles.css?raw\";\nimport { Hono } from \"hono\";\nimport { cn } from \"@/lib/utils\";\nimport x from \"~/x\";\nimport y from \"#internal/y\";\nimport z from \"node:path\";\n";
		let rewritten = rewrite_in(
			"src/modules/items/item.routes.ts",
			code,
			ImportStyle::RuntimeExtension,
			false,
		);

		assert_eq!(rewritten.text, code);
		assert!(rewritten.unresolved.is_empty(), "{:?}", rewritten.unresolved);
	}

	#[test]
	fn should_report_relative_specifiers_that_name_no_file_once() {
		let rewritten = rewrite_in(
			"src/modules/items/item.routes.ts",
			"import a from \"./nowhere\";\nimport b from \"./nowhere\";\nimport c from \"./item.unknown\";\nimport d from \"../../../../outside\";\n",
			ImportStyle::RuntimeExtension,
			false,
		);

		assert_eq!(
			rewritten.unresolved,
			["./nowhere", "./item.unknown", "../../../../outside"]
		);
		assert!(rewritten.text.contains("\"./nowhere\""));
	}

	#[test]
	fn should_leave_comments_strings_and_directive_lines_alone() {
		let code = "// import a from \"./item.service\";\nconst s = \"./item.service\";\nconst t = `import(\"./item.service\")`;\nimport b from \"./item.service\"; // @cuilo if stack.hono\nimport c from \"./item.service\";\n";

		assert_eq!(
			js(code),
			code.replacen(
				"import c from \"./item.service\"",
				"import c from \"./item.service.js\"",
				1
			)
		);
	}

	#[test]
	fn should_rewrite_require_only_in_commonjs() {
		let code = "const a = require(\"./item.service\");\n";

		assert_eq!(
			rewrite_in(
				"src/modules/items/item.config.cts",
				code,
				ImportStyle::RuntimeExtension,
				true
			)
			.text,
			"const a = require(\"./item.service.js\");\n"
		);
		assert_eq!(js(code), code);
	}

	#[test]
	fn should_keep_everything_in_the_extensionless_style() {
		let code = "import a from \"./nowhere\";\nimport b from \"./item.service\";\n";
		let rewritten = rewrite_in(
			"src/modules/items/item.routes.ts",
			code,
			ImportStyle::Extensionless,
			false,
		);

		assert_eq!(
			rewritten,
			Rewritten {
				text: code.to_owned(),
				unresolved: Vec::new()
			}
		);
	}

	#[test]
	fn should_resolve_imports_inserted_into_another_file_from_that_files_folder() {
		assert_eq!(
			rewrite_in(
				"src/index.ts",
				"import { routes } from \"./modules/items/item.routes\";",
				ImportStyle::RuntimeExtension,
				false
			)
			.text,
			"import { routes } from \"./modules/items/item.routes.js\";"
		);
	}

	#[test]
	fn is_code_file_should_know_script_extensions_only() {
		let code = |path| is_code_file(&RelPath::new(path).unwrap());

		assert!(code("src/a.ts") && code("src/a.d.ts") && code("src/a.mjs") && code("src/a.cts"));
		assert!(!code("src/a.svelte") && !code("src/a.css") && !code("src/.ts") && !code("Makefile"));
	}
}
