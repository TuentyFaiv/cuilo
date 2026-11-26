//! How a project resolves relative imports: Deno, or the `tsconfig.json` that governs a folder with the
//! configs it extends, and whether its `package.json` makes `.js` and `.ts` files `CommonJS`.
//!
//! Tsconfig files, and the `package.json` files that resolve `extends`, are read through links whose real
//! path is inside the root, as in pnpm's `node_modules`; nothing else here follows links.

use std::fmt;

use serde::Deserialize;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde_json::Value as Json;

use super::Workspace;
use super::detect::parse_jsonc;
use crate::model::{Fact, ImportStyle, RelPath, Stack};

/// The most configs one `extends` chain goes through, the governing one included.
const MAX_DEPTH: usize = 16;
/// The most configs one lookup reads, across every branch of `extends` arrays.
const MAX_CONFIGS: usize = 64;
/// The largest `tsconfig.json` or `package.json` read, in bytes.
const MAX_SIZE: u64 = 1024 * 1024;
/// The `exports` conditions TypeScript matches when it resolves a package in `extends`.
const CONDITIONS: [&str; 4] = ["require", "types", "node", "default"];
/// Values of `module` that imply `NodeNext`-style resolution when `moduleResolution` is unset.
const NODE_MODULES: [&str; 4] = ["node16", "node18", "node20", "nodenext"];

/// The import style for code generated in `folder` (the root when `None`), or `None` when the stack has
/// no JavaScript or TypeScript.
///
/// The nearest folder from `folder` up to the root with a `deno.json` or `deno.jsonc` gives the source
/// style; one with a `tsconfig.json` gives that config's style. With neither, the `deno` runtime gives
/// the source style, and anything else stays extensionless.
pub(crate) fn import_style(workspace: &Workspace, folder: Option<&RelPath>, stack: &Stack) -> Option<ImportStyle> {
	if !stack.has(Fact::Language, "typescript") && !stack.has(Fact::Language, "javascript") {
		return None;
	}
	for dir in upward(folder) {
		if ["deno.json", "deno.jsonc"]
			.into_iter()
			.any(|name| is_file(workspace, &child(dir.as_ref(), name)))
		{
			return Some(ImportStyle::SourceExtension);
		}
		let tsconfig = child(dir.as_ref(), "tsconfig.json");
		if let Some(tsconfig) = workspace.resolve_links_inside_root_for_reading(&tsconfig) {
			return Some(tsconfig_style(workspace, &tsconfig));
		}
	}
	Some(if stack.has(Fact::Runtime, "deno") {
		ImportStyle::SourceExtension
	} else {
		ImportStyle::Extensionless
	})
}

/// Whether `.js` and `.ts` files in `folder` are `CommonJS`: the nearest `package.json` does not say
/// `"type": "module"`, or there is none.
pub(crate) fn is_commonjs(workspace: &Workspace, folder: Option<&RelPath>) -> bool {
	for dir in upward(folder) {
		if let Some(text) = read(workspace, &child(dir.as_ref(), "package.json")) {
			let package: Json = serde_json::from_str(&text).unwrap_or_default();
			return package.get("type").and_then(Json::as_str) != Some("module");
		}
	}
	true
}

/// The import style the effective options of the `tsconfig.json` at the real path `path` give.
fn tsconfig_style(workspace: &Workspace, path: &RelPath) -> ImportStyle {
	let mut budget = MAX_CONFIGS;
	let options = load(workspace, path, &mut Vec::new(), &mut budget);
	options.style()
}

/// The `compilerOptions` that decide how relative imports are written; unset ones are `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Options {
	module: Option<String>,
	module_resolution: Option<String>,
	allow_importing_ts_extensions: Option<bool>,
	rewrite_relative_import_extensions: Option<bool>,
}

impl Options {
	/// The options a config sets itself.
	fn of(config: &Json) -> Self {
		let options = config.get("compilerOptions");
		let text = |key: &str| {
			options
				.and_then(|options| options.get(key))
				.and_then(Json::as_str)
				.map(str::to_ascii_lowercase)
		};
		let flag = |key: &str| options.and_then(|options| options.get(key)).and_then(Json::as_bool);
		Self {
			module: text("module"),
			module_resolution: text("moduleResolution"),
			allow_importing_ts_extensions: flag("allowImportingTsExtensions"),
			rewrite_relative_import_extensions: flag("rewriteRelativeImportExtensions"),
		}
	}

	/// These options, with `base` filling the ones they leave unset.
	fn over(self, base: Self) -> Self {
		Self {
			module: self.module.or(base.module),
			module_resolution: self.module_resolution.or(base.module_resolution),
			allow_importing_ts_extensions: self
				.allow_importing_ts_extensions
				.or(base.allow_importing_ts_extensions),
			rewrite_relative_import_extensions: self
				.rewrite_relative_import_extensions
				.or(base.rewrite_relative_import_extensions),
		}
	}

	fn style(&self) -> ImportStyle {
		if self.allow_importing_ts_extensions == Some(true) || self.rewrite_relative_import_extensions == Some(true) {
			return ImportStyle::SourceExtension;
		}
		let node = match &self.module_resolution {
			Some(resolution) => matches!(resolution.as_str(), "node16" | "nodenext"),
			None => self
				.module
				.as_deref()
				.is_some_and(|module| NODE_MODULES.contains(&module)),
		};
		if node {
			ImportStyle::RuntimeExtension
		} else {
			ImportStyle::Extensionless
		}
	}
}

/// The effective options of the config at the real path `path`: those of each config it extends, in order,
/// then its own. `chain` holds the configs being read, to stop cycles; `budget` counts the reads left.
fn load(workspace: &Workspace, path: &RelPath, chain: &mut Vec<RelPath>, budget: &mut usize) -> Options {
	if chain.len() >= MAX_DEPTH || *budget == 0 || chain.contains(path) {
		return Options::default();
	}
	*budget -= 1;
	let Some(config) = read(workspace, path).and_then(|text| parse_jsonc(&text)) else {
		return Options::default();
	};
	let bases: Vec<&str> = match config.get("extends") {
		Some(Json::String(base)) => vec![base.as_str()],
		Some(Json::Array(bases)) => bases.iter().filter_map(Json::as_str).collect(),
		_ => Vec::new(),
	};
	chain.push(path.clone());
	let mut inherited = Options::default();
	for base in bases {
		if let Some(base) = extended(workspace, path, base) {
			inherited = load(workspace, &base, chain, budget).over(inherited);
		}
	}
	chain.pop();
	Options::of(&config).over(inherited)
}

/// The real path of the config that `specifier`, an `extends` entry of the config at the real path `from`,
/// names; `None` when it is missing, absolute or outside the project.
fn extended(workspace: &Workspace, from: &RelPath, specifier: &str) -> Option<RelPath> {
	let dir = from.parent();
	if specifier.starts_with("./") || specifier.starts_with("../") {
		return config_file(workspace, &RelPath::resolve(dir.as_ref(), specifier)?);
	}
	if specifier.is_empty()
		|| specifier.starts_with(['/', '\\', '.'])
		|| specifier.contains(':')
		|| specifier.split('/').any(|segment| matches!(segment, "" | "." | ".."))
	{
		return None;
	}
	let scope_end = if specifier.starts_with('@') {
		specifier.find('/')? + 1
	} else {
		0
	};
	let (name, subpath) = match specifier[scope_end..].split_once('/') {
		Some((name, subpath)) => (&specifier[..scope_end + name.len()], subpath),
		None => (specifier, ""),
	};
	upward(dir.as_ref()).find_map(|dir| {
		let package = RelPath::resolve(dir.as_ref(), &format!("node_modules/{name}"))?;
		package_config(workspace, &package, subpath)
	})
}

/// The real path of the config `subpath` names in the package at `package`, through its `exports` when it
/// has them, else its `tsconfig` field or `tsconfig.json` for the package itself.
fn package_config(workspace: &Workspace, package: &RelPath, subpath: &str) -> Option<RelPath> {
	let manifest: Manifest = workspace
		.resolve_links_inside_root_for_reading(&package.join(&RelPath::constant("package.json")))
		.and_then(|manifest| read(workspace, &manifest))
		.and_then(|text| serde_json::from_str(&text).ok())
		.unwrap_or_default();
	if let Some(exports) = &manifest.exports {
		let key = if subpath.is_empty() {
			".".to_owned()
		} else {
			format!("./{subpath}")
		};
		let target = exports.resolve(&key)?;
		let path = RelPath::resolve(Some(package), target.strip_prefix("./")?)?;
		if !path.as_str().starts_with(&format!("{package}/")) {
			return None;
		}
		return workspace.resolve_links_inside_root_for_reading(&path);
	}
	if subpath.is_empty() {
		if let Some(field) = manifest
			.tsconfig
			.as_deref()
			.and_then(|field| RelPath::resolve(Some(package), field))
		{
			return config_file(workspace, &field);
		}
		return config_file(workspace, &package.join(&RelPath::constant("tsconfig.json")));
	}
	let path = RelPath::resolve(Some(package), subpath)?;
	config_file(workspace, &path).or_else(|| config_file(workspace, &path.join(&RelPath::constant("tsconfig.json"))))
}

/// The real path of `path` when it is a file, else of `path.json` when that is one, as TypeScript tries.
fn config_file(workspace: &Workspace, path: &RelPath) -> Option<RelPath> {
	if let Some(real) = workspace.resolve_links_inside_root_for_reading(path) {
		return Some(real);
	}
	if path
		.file_name()
		.rsplit_once('.')
		.is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("json"))
	{
		return None;
	}
	let with_json = RelPath::new(&format!("{path}.json")).ok()?;
	workspace.resolve_links_inside_root_for_reading(&with_json)
}

/// The fields of a package's `package.json` that pick the config `extends` names.
#[derive(Debug, Default, Deserialize)]
struct Manifest {
	exports: Option<Exports>,
	tsconfig: Option<String>,
}

/// A `package.json` `exports` value, with object keys in the order written, as conditions need.
#[derive(Debug)]
enum Exports {
	/// A path such as `./tsconfig.json`.
	Path(String),
	/// Alternatives, the first that resolves wins.
	Alternatives(Vec<Exports>),
	/// Subpaths such as `./*`, or conditions such as `require`.
	Map(Vec<(String, Exports)>),
	/// `null` or anything else: nothing.
	Blocked,
}

impl Exports {
	/// The path `key`, a subpath such as `.` or `./tsconfig.json`, maps to.
	fn resolve(&self, key: &str) -> Option<String> {
		let Self::Map(entries) = self else {
			return (key == ".").then(|| self.target("")).flatten();
		};
		if !entries.iter().any(|(name, _)| name.starts_with('.')) {
			return (key == ".").then(|| self.target("")).flatten();
		}
		if let Some((_, exact)) = entries.iter().find(|(name, _)| name == key) {
			return exact.target("");
		}
		entries
			.iter()
			.filter_map(|(name, target)| {
				let (prefix, suffix) = name.split_once('*')?;
				let star = key.strip_prefix(prefix)?.strip_suffix(suffix)?;
				(!suffix.contains('*')).then_some((prefix.len(), star, target))
			})
			.max_by_key(|(prefix, ..)| *prefix)
			.and_then(|(_, star, target)| target.target(star))
	}

	/// The path this target gives, with `star` in place of `*`.
	fn target(&self, star: &str) -> Option<String> {
		match self {
			Self::Path(path) => Some(path.replace('*', star)),
			Self::Alternatives(alternatives) => alternatives.iter().find_map(|target| target.target(star)),
			Self::Map(conditions) => conditions
				.iter()
				.filter(|(condition, _)| CONDITIONS.contains(&condition.as_str()))
				.find_map(|(_, target)| target.target(star)),
			Self::Blocked => None,
		}
	}
}

impl<'de> Deserialize<'de> for Exports {
	fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		deserializer.deserialize_any(ExportsVisitor)
	}
}

struct ExportsVisitor;

impl<'de> Visitor<'de> for ExportsVisitor {
	type Value = Exports;

	fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("a package.json `exports` value")
	}

	fn visit_str<E: de::Error>(self, path: &str) -> Result<Exports, E> {
		Ok(Exports::Path(path.to_owned()))
	}

	fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Exports, A::Error> {
		let mut alternatives = Vec::new();
		while let Some(alternative) = seq.next_element()? {
			alternatives.push(alternative);
		}
		Ok(Exports::Alternatives(alternatives))
	}

	fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Exports, A::Error> {
		let mut entries = Vec::new();
		while let Some(entry) = map.next_entry()? {
			entries.push(entry);
		}
		Ok(Exports::Map(entries))
	}

	fn visit_unit<E: de::Error>(self) -> Result<Exports, E> {
		Ok(Exports::Blocked)
	}

	fn visit_bool<E: de::Error>(self, _: bool) -> Result<Exports, E> {
		Ok(Exports::Blocked)
	}

	fn visit_i64<E: de::Error>(self, _: i64) -> Result<Exports, E> {
		Ok(Exports::Blocked)
	}

	fn visit_u64<E: de::Error>(self, _: u64) -> Result<Exports, E> {
		Ok(Exports::Blocked)
	}

	fn visit_f64<E: de::Error>(self, _: f64) -> Result<Exports, E> {
		Ok(Exports::Blocked)
	}
}

/// `folder`, then each folder above it, then the root (`None`).
fn upward(folder: Option<&RelPath>) -> impl Iterator<Item = Option<RelPath>> {
	std::iter::successors(Some(folder.cloned()), |dir| dir.as_ref().map(RelPath::parent))
}

fn child(dir: Option<&RelPath>, name: &'static str) -> RelPath {
	let name = RelPath::constant(name);
	dir.map_or_else(|| name.clone(), |dir| dir.join(&name))
}

/// Whether `path` is a file; links, folders and unreadable paths are not.
fn is_file(workspace: &Workspace, path: &RelPath) -> bool {
	workspace.exists(path).unwrap_or(false)
}

/// The text of the file at `path`, unless it is missing, unreadable, a link or over [`MAX_SIZE`].
fn read(workspace: &Workspace, path: &RelPath) -> Option<String> {
	let bytes = workspace.read_limited(path, MAX_SIZE).ok()??;
	String::from_utf8(bytes).ok()
}

impl Workspace {
	/// Read-only, inside-root link resolution: the real path of the file at `path`, following symlinks and
	/// Windows junctions in any segment, when it is inside the root; `None` when a link is broken, loops or
	/// leads outside, or no file is there. The real path is link-free, so the usual reads take it. Private to
	/// this module: only tsconfig files and the `package.json` files that resolve `extends` are read this way.
	fn resolve_links_inside_root_for_reading(&self, path: &RelPath) -> Option<RelPath> {
		let real = self.fs.resolve_links_inside_root(path).ok()??;
		self.exists(&real).unwrap_or(false).then_some(real)
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::workspace::MemoryFs;
	#[cfg(any(unix, windows))]
	use crate::workspace::{DiskFs, EntryKind, Fs};

	fn workspace(files: &[(&str, &str)]) -> Workspace {
		let fs = MemoryFs::new();
		for (path, text) in files {
			fs.insert(&RelPath::new(path).unwrap(), text.as_bytes());
		}
		Workspace::new(Arc::new(fs))
	}

	fn typescript() -> Stack {
		let mut stack = Stack::default();
		stack.insert(Fact::Language, "typescript");
		stack
	}

	fn style(files: &[(&str, &str)], folder: Option<&str>) -> Option<ImportStyle> {
		let folder = folder.map(|folder| RelPath::new(folder).unwrap());
		import_style(&workspace(files), folder.as_ref(), &typescript())
	}

	fn tsconfig(options: &str) -> String {
		format!("{{ \"compilerOptions\": {{ {options} }} }}")
	}

	/// Where pnpm puts `@tsconfig/node22`; `node_modules/@tsconfig/node22` links to it.
	const PNPM_NODE22: &str = "node_modules/.pnpm/@tsconfig+node22@22.0.2/node_modules/@tsconfig/node22";
	/// The link pnpm writes at `node_modules/@tsconfig/node22`.
	const PNPM_NODE22_LINK: &str = "../.pnpm/@tsconfig+node22@22.0.2/node_modules/@tsconfig/node22";
	const NODE22_PACKAGE: &str = r#"{ "name": "@tsconfig/node22", "version": "22.0.2" }"#;
	const NODE22_TSCONFIG: &str =
		r#"{ "display": "Node 22", "compilerOptions": { "module": "nodenext", "target": "es2022" } }"#;
	/// The `extends` entries that name `@tsconfig/node22`.
	const NODE22_SPECIFIERS: [&str; 2] = ["@tsconfig/node22/tsconfig.json", "@tsconfig/node22"];

	/// The root's import style with `files` and `links`, as `(path, target)`.
	fn linked_style(files: &[(&str, &str)], links: &[(&str, &str)]) -> Option<ImportStyle> {
		let fs = MemoryFs::new();
		for (path, text) in files {
			fs.insert(&RelPath::new(path).unwrap(), text.as_bytes());
		}
		for (path, target) in links {
			fs.insert_symlink_to(&RelPath::new(path).unwrap(), target);
		}
		import_style(&Workspace::new(Arc::new(fs)), None, &typescript())
	}

	/// `@tsconfig/node22` in pnpm's store, a project `tsconfig.json` extending `specifier`, and `files`.
	fn pnpm_files<'a>(specifier: &str, files: &[(&'a str, &'a str)]) -> Vec<(String, String)> {
		let mut all = vec![
			(format!("{PNPM_NODE22}/package.json"), NODE22_PACKAGE.to_owned()),
			(format!("{PNPM_NODE22}/tsconfig.json"), NODE22_TSCONFIG.to_owned()),
			(
				"tsconfig.json".to_owned(),
				format!("{{ \"extends\": \"{specifier}\" }}"),
			),
		];
		all.extend(
			files
				.iter()
				.map(|(path, text)| ((*path).to_owned(), (*text).to_owned())),
		);
		all
	}

	fn pnpm_style(specifier: &str, files: &[(&str, &str)], links: &[(&str, &str)]) -> Option<ImportStyle> {
		let files = pnpm_files(specifier, files);
		let files: Vec<(&str, &str)> = files
			.iter()
			.map(|(path, text)| (path.as_str(), text.as_str()))
			.collect();
		linked_style(&files, links)
	}

	#[test]
	fn no_tsconfig_should_stay_extensionless_and_other_languages_get_no_style() {
		let rust = {
			let mut stack = Stack::default();
			stack.insert(Fact::Language, "rust");
			stack
		};

		assert_eq!(style(&[], None), Some(ImportStyle::Extensionless));
		assert_eq!(import_style(&workspace(&[]), None, &rust), None);
	}

	#[test]
	fn module_resolution_should_decide_and_be_read_in_any_case() {
		for (options, expected) in [
			(r#""moduleResolution": "NodeNext""#, ImportStyle::RuntimeExtension),
			(r#""moduleResolution": "node16""#, ImportStyle::RuntimeExtension),
			(r#""moduleResolution": "Bundler""#, ImportStyle::Extensionless),
			(r#""moduleResolution": "node10""#, ImportStyle::Extensionless),
			(r#""moduleResolution": "node""#, ImportStyle::Extensionless),
			(r#""moduleResolution": "classic""#, ImportStyle::Extensionless),
			(
				r#""module": "NodeNext", "moduleResolution": "bundler""#,
				ImportStyle::Extensionless,
			),
			(r#""module": "ESNext""#, ImportStyle::Extensionless),
			(r#""module": "Preserve""#, ImportStyle::Extensionless),
		] {
			assert_eq!(
				style(&[("tsconfig.json", &tsconfig(options))], None),
				Some(expected),
				"{options}"
			);
		}
	}

	#[test]
	fn a_node_module_without_module_resolution_should_imply_node_next() {
		for module in ["node16", "Node18", "node20", "NodeNext"] {
			let config = tsconfig(&format!("\"module\": \"{module}\""));

			assert_eq!(
				style(&[("tsconfig.json", &config)], None),
				Some(ImportStyle::RuntimeExtension),
				"{module}"
			);
		}
	}

	#[test]
	fn ts_extension_flags_should_give_the_source_style() {
		for flag in ["allowImportingTsExtensions", "rewriteRelativeImportExtensions"] {
			let config = tsconfig(&format!("\"moduleResolution\": \"bundler\", \"{flag}\": true"));

			assert_eq!(
				style(&[("tsconfig.json", &config)], None),
				Some(ImportStyle::SourceExtension),
				"{flag}"
			);
		}
		let off = tsconfig(r#""module": "nodenext", "allowImportingTsExtensions": false"#);
		assert_eq!(
			style(&[("tsconfig.json", &off)], None),
			Some(ImportStyle::RuntimeExtension)
		);
	}

	#[test]
	fn tsconfig_should_be_read_as_jsonc() {
		let config = "{\n  // Node.js\n  \"compilerOptions\": {\n    /* runtime */ \"module\": \"NodeNext\",\n  },\n}\n";

		assert_eq!(
			style(&[("tsconfig.json", config)], None),
			Some(ImportStyle::RuntimeExtension)
		);
	}

	#[test]
	fn the_nearest_tsconfig_to_the_folder_should_govern_it() {
		let files = [
			("tsconfig.json", tsconfig(r#""moduleResolution": "bundler""#)),
			("apps/api/tsconfig.json", tsconfig(r#""module": "nodenext""#)),
		];
		let files: Vec<(&str, &str)> = files.iter().map(|(path, text)| (*path, text.as_str())).collect();

		assert_eq!(
			style(&files, Some("apps/api/src/modules")),
			Some(ImportStyle::RuntimeExtension)
		);
		assert_eq!(style(&files, Some("apps/web/src")), Some(ImportStyle::Extensionless));
		assert_eq!(style(&files, None), Some(ImportStyle::Extensionless));
	}

	#[test]
	fn relative_extends_should_chain_and_own_options_should_win() {
		let files = [
			("tsconfig.json", r#"{ "extends": "./config/tsconfig.base" }"#),
			(
				"config/tsconfig.base.json",
				r#"{ "extends": "../tsconfig.node.json", "compilerOptions": { "strict": true } }"#,
			),
			(
				"tsconfig.node.json",
				r#"{ "compilerOptions": { "module": "nodenext" } }"#,
			),
		];
		let overridden = [
			(
				"tsconfig.json",
				r#"{ "extends": "./tsconfig.node.json", "compilerOptions": { "moduleResolution": "bundler" } }"#,
			),
			(
				"tsconfig.node.json",
				r#"{ "compilerOptions": { "module": "nodenext" } }"#,
			),
		];

		assert_eq!(style(&files, None), Some(ImportStyle::RuntimeExtension));
		assert_eq!(style(&overridden, None), Some(ImportStyle::Extensionless));
	}

	#[test]
	fn an_extends_array_should_let_later_configs_win() {
		let files = |order: &str| {
			vec![
				("tsconfig.json".to_owned(), format!("{{ \"extends\": {order} }}")),
				("node.json".to_owned(), tsconfig(r#""moduleResolution": "nodenext""#)),
				("bundler.json".to_owned(), tsconfig(r#""moduleResolution": "bundler""#)),
			]
		};
		let style_of = |order: &str| {
			let files = files(order);
			let files: Vec<(&str, &str)> = files
				.iter()
				.map(|(path, text)| (path.as_str(), text.as_str()))
				.collect();
			style(&files, None)
		};

		assert_eq!(
			style_of(r#"["./bundler.json", "./node.json"]"#),
			Some(ImportStyle::RuntimeExtension)
		);
		assert_eq!(
			style_of(r#"["./node.json", "./bundler.json"]"#),
			Some(ImportStyle::Extensionless)
		);
	}

	#[test]
	fn package_extends_should_find_the_packages_tsconfig_a_path_in_it_and_its_fields() {
		let node = tsconfig(r#""module": "node20""#);
		let cases: [(&str, Vec<(&str, &str)>); 5] = [
			(
				"@tsconfig/node22",
				vec![("node_modules/@tsconfig/node22/tsconfig.json", node.as_str())],
			),
			(
				"@tsconfig/node22/tsconfig.json",
				vec![("node_modules/@tsconfig/node22/tsconfig.json", node.as_str())],
			),
			("base/node", vec![("node_modules/base/node.json", node.as_str())]),
			(
				"base",
				vec![
					(
						"node_modules/base/package.json",
						r#"{ "tsconfig": "./configs/node.json" }"#,
					),
					("node_modules/base/configs/node.json", node.as_str()),
				],
			),
			(
				"base/node",
				vec![
					(
						"node_modules/base/package.json",
						r#"{ "exports": { "./package.json": "./package.json", "./*": { "import": "./esm/*.json", "require": "./configs/*.json" } } }"#,
					),
					("node_modules/base/configs/node.json", node.as_str()),
				],
			),
		];
		for (specifier, mut files) in cases {
			let config = format!("{{ \"extends\": \"{specifier}\" }}");
			files.push(("apps/api/tsconfig.json", &config));

			assert_eq!(
				style(&files, Some("apps/api/src")),
				Some(ImportStyle::RuntimeExtension),
				"{specifier}"
			);
		}
	}

	#[test]
	fn package_exports_should_be_honoured_even_when_they_hide_the_tsconfig() {
		let files = [
			("tsconfig.json", r#"{ "extends": "base" }"#),
			(
				"node_modules/base/package.json",
				r#"{ "exports": { ".": { "default": "./strict.json" } } }"#,
			),
			(
				"node_modules/base/strict.json",
				r#"{ "compilerOptions": { "strict": true } }"#,
			),
			(
				"node_modules/base/tsconfig.json",
				r#"{ "compilerOptions": { "module": "nodenext" } }"#,
			),
		];

		assert_eq!(style(&files, None), Some(ImportStyle::Extensionless));
	}

	#[test]
	fn extends_outside_the_project_should_be_refused() {
		let node = r#"{ "compilerOptions": { "module": "nodenext" } }"#;
		for specifier in [
			"../outside/tsconfig.json",
			"/etc/tsconfig.json",
			"C:/tsconfig.json",
			"pkg/../../tsconfig.json",
		] {
			let config = format!("{{ \"extends\": \"{specifier}\" }}");

			assert_eq!(
				style(&[("tsconfig.json", &config), ("outside/tsconfig.json", node)], None),
				Some(ImportStyle::Extensionless),
				"{specifier}"
			);
		}
	}

	#[test]
	fn extends_cycles_and_long_chains_should_stop() {
		let cycle = [
			("tsconfig.json", r#"{ "extends": "./a.json" }"#),
			(
				"a.json",
				r#"{ "extends": "./tsconfig.json", "compilerOptions": { "module": "nodenext" } }"#,
			),
		];
		let mut chain: Vec<(String, String)> = (0..40)
			.map(|i| {
				(
					format!("c{i}.json"),
					format!("{{ \"extends\": \"./c{}.json\" }}", i + 1),
				)
			})
			.collect();
		chain.push(("c40.json".to_owned(), tsconfig(r#""module": "nodenext""#)));
		chain.push(("tsconfig.json".to_owned(), r#"{ "extends": "./c0.json" }"#.to_owned()));
		let chain: Vec<(&str, &str)> = chain
			.iter()
			.map(|(path, text)| (path.as_str(), text.as_str()))
			.collect();

		assert_eq!(style(&cycle, None), Some(ImportStyle::RuntimeExtension));
		assert_eq!(style(&chain, None), Some(ImportStyle::Extensionless));
	}

	#[test]
	fn oversized_configs_should_be_ignored() {
		let padding = " ".repeat(usize::try_from(MAX_SIZE).unwrap());
		let config = format!("{{ \"compilerOptions\": {{ \"module\": \"nodenext\" }} }}{padding}");

		assert_eq!(
			style(&[("tsconfig.json", &config)], None),
			Some(ImportStyle::Extensionless)
		);
	}

	#[test]
	fn deno_should_give_the_source_style() {
		let mut deno = typescript();
		deno.insert(Fact::Runtime, "deno");

		assert_eq!(
			style(&[("deno.json", "{}")], Some("src")),
			Some(ImportStyle::SourceExtension)
		);
		assert_eq!(style(&[("deno.jsonc", "{}")], None), Some(ImportStyle::SourceExtension));
		assert_eq!(
			import_style(&workspace(&[]), None, &deno),
			Some(ImportStyle::SourceExtension)
		);
	}

	#[test]
	fn is_commonjs_should_follow_the_nearest_package_type() {
		let ws = workspace(&[
			("package.json", r#"{ "type": "module" }"#),
			("legacy/package.json", "{}"),
		]);

		assert!(!is_commonjs(&ws, Some(&RelPath::new("src").unwrap())));
		assert!(is_commonjs(&ws, Some(&RelPath::new("legacy/lib").unwrap())));
		assert!(is_commonjs(&workspace(&[]), None));
	}

	#[test]
	fn a_pnpm_link_to_a_package_folder_inside_the_project_should_be_followed() {
		for specifier in NODE22_SPECIFIERS {
			assert_eq!(
				pnpm_style(specifier, &[], &[("node_modules/@tsconfig/node22", PNPM_NODE22_LINK)]),
				Some(ImportStyle::RuntimeExtension),
				"{specifier}"
			);
			assert_eq!(
				pnpm_style(specifier, &[], &[]),
				Some(ImportStyle::Extensionless),
				"{specifier}"
			);
		}
	}

	#[test]
	fn linked_tsconfig_files_inside_the_project_should_be_followed() {
		let node = tsconfig(r#""module": "nodenext""#);
		let files = [
			("configs/node.json", node.as_str()),
			("configs/base.json", r#"{ "extends": "./strict" }"#),
			("configs/shared/strict.json", r#"{ "extends": "../node.json" }"#),
		];
		let links = [
			("tsconfig.json", "configs/base.json"),
			("configs/strict.json", "shared/strict.json"),
		];

		assert_eq!(linked_style(&files, &links), Some(ImportStyle::RuntimeExtension));
	}

	#[test]
	fn links_leading_out_of_the_project_should_be_refused() {
		let outside = [("outside/node22/tsconfig.json", NODE22_TSCONFIG)];
		for target in [
			"/outside/node22",
			"../../../outside/node22",
			"../../../../outside/node22",
		] {
			for specifier in NODE22_SPECIFIERS {
				assert_eq!(
					pnpm_style(specifier, &outside, &[("node_modules/@tsconfig/node22", target)]),
					Some(ImportStyle::Extensionless),
					"{target} {specifier}"
				);
			}
		}
		for target in ["/outside/node22/tsconfig.json", "../outside/node22/tsconfig.json"] {
			assert_eq!(
				linked_style(&outside, &[("tsconfig.json", target)]),
				Some(ImportStyle::Extensionless),
				"{target}"
			);
		}
	}

	#[test]
	fn broken_and_looping_links_should_be_refused() {
		let broken = [(
			"node_modules/@tsconfig/node22",
			"../.pnpm/@tsconfig+node22@22.0.1/node_modules/@tsconfig/node22",
		)];
		let looping = [
			("node_modules/@tsconfig/node22", "node20"),
			("node_modules/@tsconfig/node20", "./node22"),
		];
		let into_itself = [("node_modules/@tsconfig/node22", "../@tsconfig/node22/inner")];
		for links in [&broken[..], &looping, &into_itself] {
			for specifier in NODE22_SPECIFIERS {
				assert_eq!(
					pnpm_style(specifier, &[], links),
					Some(ImportStyle::Extensionless),
					"{links:?} {specifier}"
				);
			}
		}
		assert_eq!(
			linked_style(&[], &[("tsconfig.json", "tsconfig.json")]),
			Some(ImportStyle::Extensionless)
		);
	}

	/// A [`DiskFs`] for lookups: it keeps the real path of every file read and refuses every write.
	#[cfg(any(unix, windows))]
	#[derive(Debug)]
	struct ReadLog {
		disk: DiskFs,
		reads: std::sync::Mutex<Vec<std::path::PathBuf>>,
	}

	#[cfg(any(unix, windows))]
	impl ReadLog {
		fn open(root: &std::path::Path) -> Arc<Self> {
			Arc::new(Self {
				disk: DiskFs::open(root).unwrap(),
				reads: std::sync::Mutex::default(),
			})
		}

		/// The style of the root, and whether every file read is inside the root.
		fn style(self: &Arc<Self>) -> (Option<ImportStyle>, bool) {
			let style = import_style(&Workspace::new(self.clone()), None, &typescript());
			let reads = self.reads.lock().unwrap();
			assert!(!reads.is_empty());
			(style, reads.iter().all(|read| read.starts_with(self.disk.root())))
		}
	}

	#[cfg(any(unix, windows))]
	impl Fs for ReadLog {
		fn entry(&self, path: &RelPath) -> std::io::Result<Option<EntryKind>> {
			self.disk.entry(path)
		}

		fn read(&self, path: &RelPath) -> std::io::Result<Vec<u8>> {
			let real = std::fs::canonicalize(path.under(self.disk.root()))?;
			self.reads.lock().unwrap().push(real);
			self.disk.read(path)
		}

		fn size(&self, path: &RelPath) -> std::io::Result<u64> {
			self.disk.size(path)
		}

		fn write(&self, _: &RelPath, _: &[u8]) -> std::io::Result<()> {
			Err(std::io::ErrorKind::Unsupported.into())
		}

		fn create_new(&self, _: &RelPath, _: &[u8]) -> std::io::Result<()> {
			Err(std::io::ErrorKind::Unsupported.into())
		}

		fn create_dir(&self, _: &RelPath) -> std::io::Result<()> {
			Err(std::io::ErrorKind::Unsupported.into())
		}

		fn remove_file(&self, _: &RelPath) -> std::io::Result<()> {
			Err(std::io::ErrorKind::Unsupported.into())
		}

		fn remove_dir(&self, _: &RelPath) -> std::io::Result<()> {
			Err(std::io::ErrorKind::Unsupported.into())
		}

		fn rename(&self, _: &RelPath, _: &RelPath) -> std::io::Result<()> {
			Err(std::io::ErrorKind::Unsupported.into())
		}

		fn list(&self, dir: Option<&RelPath>) -> std::io::Result<Vec<String>> {
			self.disk.list(dir)
		}

		fn resolve_links_inside_root(&self, path: &RelPath) -> std::io::Result<Option<RelPath>> {
			self.disk.resolve_links_inside_root(path)
		}
	}

	/// `rel`, a `/`-separated path, below `root`.
	#[cfg(any(unix, windows))]
	fn native(root: &std::path::Path, rel: &str) -> std::path::PathBuf {
		RelPath::new(rel).unwrap().under(root)
	}

	/// Writes the files of [`pnpm_files`] below `root`, `@tsconfig/node22` in pnpm's store included.
	#[cfg(any(unix, windows))]
	fn write_pnpm_files(root: &std::path::Path, specifier: &str) {
		for (path, text) in pnpm_files(specifier, &[]) {
			let path = native(root, &path);
			std::fs::create_dir_all(path.parent().unwrap()).unwrap();
			std::fs::write(path, text).unwrap();
		}
		std::fs::create_dir_all(native(root, "node_modules/@tsconfig")).unwrap();
	}

	#[cfg(unix)]
	#[test]
	fn a_pnpm_layout_on_disk_should_give_the_linked_packages_style() {
		for specifier in NODE22_SPECIFIERS {
			let dir = tempfile::tempdir().unwrap();
			write_pnpm_files(dir.path(), specifier);
			std::os::unix::fs::symlink(PNPM_NODE22_LINK, native(dir.path(), "node_modules/@tsconfig/node22")).unwrap();

			assert_eq!(
				ReadLog::open(dir.path()).style(),
				(Some(ImportStyle::RuntimeExtension), true),
				"{specifier}"
			);
		}
	}

	#[cfg(unix)]
	#[test]
	fn a_pnpm_link_on_disk_leading_out_of_the_project_should_be_refused_unread() {
		for specifier in NODE22_SPECIFIERS {
			let dir = tempfile::tempdir().unwrap();
			let outside = tempfile::tempdir().unwrap();
			write_pnpm_files(dir.path(), specifier);
			write_pnpm_files(outside.path(), specifier);
			let outside_name = outside.path().file_name().unwrap().to_str().unwrap();
			let link = native(dir.path(), "node_modules/@tsconfig/node22");
			for target in [
				native(outside.path(), PNPM_NODE22),
				std::path::PathBuf::from(format!("../../../{outside_name}/{PNPM_NODE22}")),
			] {
				assert!(std::fs::canonicalize(link.parent().unwrap().join(&target)).is_ok());
				let _ = std::fs::remove_file(&link);
				std::os::unix::fs::symlink(&target, &link).unwrap();

				assert_eq!(
					ReadLog::open(dir.path()).style(),
					(Some(ImportStyle::Extensionless), true),
					"{} {specifier}",
					target.display()
				);
			}
		}
	}

	#[cfg(windows)]
	#[test]
	fn pnpm_junctions_should_be_followed_inside_the_project_and_refused_out_of_it() {
		for specifier in NODE22_SPECIFIERS {
			let dir = tempfile::tempdir().unwrap();
			let outside = tempfile::tempdir().unwrap();
			write_pnpm_files(dir.path(), specifier);
			write_pnpm_files(outside.path(), specifier);
			let link = native(dir.path(), "node_modules/@tsconfig/node22");
			for (target, expected) in [
				(native(dir.path(), PNPM_NODE22), ImportStyle::RuntimeExtension),
				(native(outside.path(), PNPM_NODE22), ImportStyle::Extensionless),
			] {
				if link.exists() {
					std::fs::remove_dir(&link).unwrap();
				}
				let output = std::process::Command::new("cmd")
					.args(["/C", "mklink", "/J"])
					.arg(link.as_os_str())
					.arg(target.as_os_str())
					.output()
					.unwrap();
				assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
				let fs = ReadLog::open(dir.path());

				assert_eq!(
					fs.disk
						.entry(&RelPath::new("node_modules/@tsconfig/node22").unwrap())
						.unwrap(),
					Some(EntryKind::Symlink)
				);
				assert!(matches!(
					Workspace::new(fs.clone()).read(&RelPath::new("node_modules/@tsconfig/node22/tsconfig.json").unwrap()),
					Err(crate::workspace::WorkspaceError::Symlink(_))
				));
				assert_eq!(fs.style(), (Some(expected), true), "{} {specifier}", target.display());
			}
		}
	}
}
