//! `pack.toml` and `piece.toml`.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use semver::Version;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::Glob;

/// The only manifest schema version this build reads.
pub const SCHEMA_VERSION: u32 = 1;

/// A pack, piece or variant name: lowercase letters, digits and `-`, starting with a letter.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(String);

/// Why a name was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("`{0}` must be 1-64 lowercase letters, digits or `-`, starting with a letter")]
pub struct NameError(String);

impl Name {
	/// Validates `name`.
	pub fn new(name: &str) -> Result<Self, NameError> {
		let valid = (1..=64).contains(&name.len())
			&& name.starts_with(|c: char| c.is_ascii_lowercase())
			&& name
				.chars()
				.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
			&& !name.ends_with('-')
			&& !name.contains("--");
		if valid {
			Ok(Self(name.to_owned()))
		} else {
			Err(NameError(name.to_owned()))
		}
	}

	/// The name as written.
	#[must_use]
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for Name {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

impl fmt::Debug for Name {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(&self.0, f)
	}
}

impl Serialize for Name {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&self.0)
	}
}

impl<'de> Deserialize<'de> for Name {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let text = Cow::<str>::deserialize(deserializer)?;
		Self::new(&text).map_err(serde::de::Error::custom)
	}
}

impl JsonSchema for Name {
	fn schema_name() -> Cow<'static, str> {
		"Name".into()
	}

	fn json_schema(_: &mut SchemaGenerator) -> Schema {
		json_schema!({
			"type": "string",
			"pattern": "^[a-z](?:[a-z0-9]|-(?=[a-z0-9])){0,63}$"
		})
	}
}

/// `pack.toml`: a pack's name, version and permissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
	/// Manifest format version; must be `1`.
	pub schema_version: u32,
	/// Lowercase letters, digits and `-`, starting with a letter.
	pub name: Name,
	/// Semantic version of the pack.
	pub version: Version,
	/// One-line summary.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	/// SPDX license expression.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub license: Option<String>,
	/// What the pack may do in a project.
	#[serde(default)]
	pub permissions: Permissions,
	/// Code changes applied when an update crosses a version.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub migrations: Vec<Migration>,
}

/// A change to project code made when an update crosses `to`, shown in the plan like any other change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Migration {
	/// Runs when updating from a version below this one to this one or later.
	pub to: Version,
	/// What it changes, for the plan.
	pub description: String,
	/// Project files it runs on; they must also be in `permissions.write`.
	pub files: Vec<Glob>,
	/// Renames an identifier, matching whole words only.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rename: Option<Rename>,
	/// An ast-grep rule file in the pack, e.g. `migrations/2.0.0.yml`; needs the `ast-grep` program.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ast_grep: Option<String>,
}

/// Old and new identifier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rename {
	/// Identifier to replace.
	pub from: String,
	/// Its replacement.
	pub to: String,
}

/// What a pack may do; granted by the user on install.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
	/// Paths the pack may create or modify.
	#[serde(default)]
	pub write: Vec<Glob>,
	/// External commands the pack may run.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub commands: Vec<String>,
	/// Whether the pack may add dependencies.
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub dependencies: bool,
}

/// `piece.toml`: one piece's inputs, templates and patches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PieceManifest {
	/// Manifest format version; must be `1`.
	pub schema_version: u32,
	/// Must match its folder; lowercase letters, digits and `-`.
	pub name: Name,
	/// What the piece is for.
	#[serde(default)]
	pub kind: PieceKind,
	/// One-line summary.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	/// Default output folder, relative to the project root; may contain examples. Empty for the root.
	pub target: String,
	/// Inputs by name.
	#[serde(default)]
	pub inputs: BTreeMap<String, Input>,
	/// Stack-specific template sets, tried in order.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub variants: Vec<Variant>,
	/// Conditions for individual template files.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub files: Vec<FileRule>,
	/// Edits to existing project files.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub patches: Vec<PatchSpec>,
	/// Setup the user does after `cuilo add`, such as mounting new routes; listed as next steps unless
	/// cuilo applies the step's own patches.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub steps: Vec<Step>,
	/// Pieces of the same pack this one builds on; added first when the project lacks them.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub requires: Vec<Name>,
	/// Packages the generated code imports.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub dependencies: Vec<Dependency>,
}

/// What a piece is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PieceKind {
	/// A code module such as a component, model or middleware.
	#[default]
	Piece,
	/// A saved set of answers for other pieces.
	Preset,
	/// Project configuration such as lint or CI settings.
	Config,
}

/// One input of a piece.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
	/// Text; with a multi-word `example`, an identifier replaced in every case form.
	String {
		/// Shown in help and schemas.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		description: Option<String>,
		/// Question asked when no value is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		prompt: Option<String>,
		/// Text in the templates that this input replaces.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		example: Option<String>,
		/// Value used when none is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		default: Option<String>,
	},
	/// `true` or `false`, for conditions.
	Bool {
		/// Shown in help and schemas.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		description: Option<String>,
		/// Question asked when no value is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		prompt: Option<String>,
		/// Value used when none is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		default: Option<bool>,
		/// Condition on the project's stack that gives the default instead, e.g. `stack.vitest`.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		default_when: Option<String>,
	},
	/// One of a fixed set of choices, for conditions.
	Enum {
		/// Shown in help and schemas.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		description: Option<String>,
		/// Question asked when no value is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		prompt: Option<String>,
		/// Allowed values.
		choices: Vec<String>,
		/// Value used when none is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		default: Option<String>,
	},
	/// A whole number.
	Number {
		/// Shown in help and schemas.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		description: Option<String>,
		/// Question asked when no value is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		prompt: Option<String>,
		/// Number in the templates that this input replaces.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		example: Option<i64>,
		/// Value used when none is given.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		default: Option<i64>,
		/// Smallest allowed value.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		min: Option<i64>,
		/// Largest allowed value.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		max: Option<i64>,
	},
}

impl Input {
	/// Help text.
	#[must_use]
	pub fn description(&self) -> Option<&str> {
		match self {
			Self::String { description, .. }
			| Self::Bool { description, .. }
			| Self::Enum { description, .. }
			| Self::Number { description, .. } => description.as_deref(),
		}
	}
}

/// A set of templates for one stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Variant {
	/// Recorded in the lockfile; lowercase letters, digits and `-`.
	pub name: Name,
	/// Project facts that must all hold, e.g. `{ framework = "react" }`.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub when: BTreeMap<Fact, String>,
	/// Template folder inside the piece; defaults to `files/<name>`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub files: Option<String>,
	/// Output folder instead of the piece's `target`, e.g. `lib/ui` for Flutter.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub target: Option<String>,
}

/// A kind of project fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fact {
	/// e.g. `react`, `svelte`, `express`.
	Framework,
	/// e.g. `typescript`, `rust`.
	Language,
	/// e.g. `node`, `bun`, `deno`, `tokio`.
	Runtime,
	/// The library whose conventions database code follows: ORMs, ODMs and query builders,
	/// e.g. `prisma`, `diesel`, `activerecord`. Drivers, SQL and schema files are not part of it.
	Database,
	/// e.g. `tailwind`, `stylex`.
	Styling,
	/// e.g. `bun`, `pnpm`, `cargo`.
	PackageManager,
	/// Test libraries the project installed, e.g. `vitest`, `jest`, `pytest`; built-in runners are not listed.
	Testing,
	/// Formatters and fixing linters the project uses, e.g. `biome`, `prettier`, `eslint`, `ruff`, `rustfmt`.
	Formatter,
	/// How generated JavaScript and TypeScript write relative imports: `imports-extensionless` (`./foo`),
	/// `imports-js` (`./foo.js`) or `imports-ts` (`./foo.ts`), from the `tsconfig.json` that governs the
	/// output folder, or Deno.
	ImportStyle,
}

impl Fact {
	/// The fact's key in conditions and config.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Framework => "framework",
			Self::Language => "language",
			Self::Runtime => "runtime",
			Self::Database => "database",
			Self::Styling => "styling",
			Self::PackageManager => "package_manager",
			Self::Testing => "testing",
			Self::Formatter => "formatter",
			Self::ImportStyle => "import_style",
		}
	}
}

/// Setup the user does after adding a piece; with `patches`, a wiring step cuilo applies when wiring is on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
	/// The next step shown after `cuilo add`, e.g. ``Mount the routes with `app.route("/example-items", routes)`.``;
	/// may contain examples.
	pub text: String,
	/// Variants it applies to; empty for every variant.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub variants: Vec<Name>,
	/// Condition on inputs or stack facts, e.g. `stack.vitest`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub when: Option<String>,
	/// Edits that do the step, applied all or none, and only with `cuilo add --wire` or `wire = true` in the
	/// user config; the same kinds and fields as `patches`.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub patches: Vec<PatchSpec>,
}

/// A package the generated code imports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
	/// Registry, which also decides the project file the package is added to.
	pub ecosystem: Ecosystem,
	/// Package name, e.g. `zod` or `github.com/google/uuid`.
	pub name: String,
	/// Version requirement in the ecosystem's syntax, e.g. `^4.1`.
	pub version: String,
	/// Cargo features or Python extras, e.g. `["derive"]`.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub features: Vec<String>,
	/// Only needed to develop and test.
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub dev: bool,
	/// Variants that need it; empty for every variant.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub variants: Vec<Name>,
	/// Condition on inputs or stack facts.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub when: Option<String>,
}

/// A package registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Ecosystem {
	/// npm (`package.json`), also used by Bun, pnpm, Yarn and Deno.
	Npm,
	/// crates.io (`Cargo.toml`).
	Cargo,
	/// The Python Package Index (`pyproject.toml`).
	Pypi,
	/// Go modules (`go.mod`).
	Go,
	/// pub.dev (`pubspec.yaml`).
	Pub,
	/// The Ruby gem registry (`Gemfile`).
	Rubygems,
	/// Packagist (`composer.json`).
	Packagist,
}

impl Ecosystem {
	/// The project file that lists these dependencies.
	#[must_use]
	pub fn manifest(self) -> &'static str {
		match self {
			Self::Npm => "package.json",
			Self::Cargo => "Cargo.toml",
			Self::Pypi => "pyproject.toml",
			Self::Go => "go.mod",
			Self::Pub => "pubspec.yaml",
			Self::Rubygems => "Gemfile",
			Self::Packagist => "composer.json",
		}
	}
}

/// A condition on one template file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileRule {
	/// Template path inside the variant folder.
	pub path: String,
	/// Condition that keeps the file.
	pub when: String,
}

/// An edit to an existing project file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatchSpec {
	/// Inserts text next to an anchor line; works in every language.
	Anchor {
		/// Project file to edit, or a list of alternatives in order of preference: the first one the project has
		/// is used. A step's patches, or the piece's own, that share a file must list the same alternatives in the
		/// same order. May contain examples.
		file: Files,
		/// Text the anchor line contains, or a list of alternatives tried in order: the first one in the file
		/// is used, and one on more than one line fails the patch without trying the rest.
		anchor: Anchors,
		/// Where to insert relative to the anchor line.
		#[serde(default)]
		position: Position,
		/// Text to insert; may contain examples.
		insert: String,
		/// Condition that keeps the patch.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		when: Option<String>,
	},
	/// Adds a JavaScript or TypeScript import after the existing imports.
	Import {
		/// Project file to edit, or a list of alternatives in order of preference: the first one the project has
		/// is used. A step's patches, or the piece's own, that share a file must list the same alternatives in the
		/// same order. May contain examples.
		file: Files,
		/// The import statement; may contain examples.
		import: String,
		/// Condition that keeps the patch.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		when: Option<String>,
	},
	/// Adds CSS custom properties to a block such as `:root`, keeping values the project already set.
	CssVars {
		/// Project CSS file to edit, or a list of alternatives in order of preference: the first one the project
		/// has is used. A step's patches, or the piece's own, that share a file must list the same alternatives in
		/// the same order. May contain examples.
		file: Files,
		/// The block's selector or at-rule, e.g. `:root`, `.dark` or `@theme inline`.
		selector: String,
		/// Properties without the leading `--`, e.g. `sidebar-border = "220 13% 91%"`.
		vars: BTreeMap<String, String>,
		/// Condition that keeps the patch.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		when: Option<String>,
	},
}

impl PatchSpec {
	/// The project file the patch edits, or its alternatives.
	#[must_use]
	pub fn file(&self) -> &Files {
		match self {
			Self::Anchor { file, .. } | Self::Import { file, .. } | Self::CssVars { file, .. } => file,
		}
	}

	/// The condition that keeps the patch.
	#[must_use]
	pub fn when(&self) -> Option<&str> {
		match self {
			Self::Anchor { when, .. } | Self::Import { when, .. } | Self::CssVars { when, .. } => when.as_deref(),
		}
	}
}

/// The text an anchor line contains, or alternatives in order of preference; never empty.
///
/// Written as a string for one alternative and as a list for several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchors(Vec<String>);

impl Anchors {
	/// Alternatives in order of preference; `None` when there are none.
	#[must_use]
	pub fn new(alternatives: Vec<String>) -> Option<Self> {
		(!alternatives.is_empty()).then_some(Self(alternatives))
	}

	/// The alternatives in order of preference.
	pub fn iter(&self) -> impl Iterator<Item = &str> {
		self.0.iter().map(String::as_str)
	}
}

impl From<&str> for Anchors {
	fn from(anchor: &str) -> Self {
		Self(vec![anchor.to_owned()])
	}
}

/// The alternatives in backticks, e.g. `` `a`, `b` or `c` ``.
impl fmt::Display for Anchors {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&or_list(&self.0))
	}
}

impl Serialize for Anchors {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serialize_alternatives(&self.0, serializer)
	}
}

impl<'de> Deserialize<'de> for Anchors {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		deserializer
			.deserialize_any(AlternativesVisitor(
				"the anchor text or a non-empty list of alternatives",
			))
			.map(Self)
	}
}

impl JsonSchema for Anchors {
	fn schema_name() -> Cow<'static, str> {
		"Anchors".into()
	}

	fn json_schema(_: &mut SchemaGenerator) -> Schema {
		json_schema!({
			"description": "The text an anchor line contains, or alternatives in order of preference.",
			"anyOf": [
				{
					"description": "Text that exactly one line of the file must contain.",
					"type": "string"
				},
				{
					"description": "Alternatives tried in order: the first one in the file is used, and one on more than one line fails the patch without trying the rest.",
					"type": "array",
					"items": { "type": "string" },
					"minItems": 1
				}
			]
		})
	}
}

/// The project file a patch edits, or alternatives in order of preference; never empty.
///
/// Written as a string for one alternative and as a list for several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Files(Vec<String>);

impl Files {
	/// Alternatives in order of preference; `None` when there are none.
	#[must_use]
	pub fn new(alternatives: Vec<String>) -> Option<Self> {
		(!alternatives.is_empty()).then_some(Self(alternatives))
	}

	/// The alternatives in order of preference.
	#[must_use]
	pub fn as_slice(&self) -> &[String] {
		&self.0
	}
}

impl From<&str> for Files {
	fn from(file: &str) -> Self {
		Self(vec![file.to_owned()])
	}
}

impl Serialize for Files {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serialize_alternatives(&self.0, serializer)
	}
}

impl<'de> Deserialize<'de> for Files {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		deserializer
			.deserialize_any(AlternativesVisitor(
				"a project file or a non-empty list of alternatives",
			))
			.map(Self)
	}
}

impl JsonSchema for Files {
	fn schema_name() -> Cow<'static, str> {
		"Files".into()
	}

	fn json_schema(_: &mut SchemaGenerator) -> Schema {
		json_schema!({
			"description": "The project file a patch edits, or alternatives in order of preference.",
			"anyOf": [
				{
					"description": "Path from the project root of a file the project must have.",
					"type": "string"
				},
				{
					"description": "Alternatives in order of preference: the first one the project has is used, and on `cuilo update` the one the piece already patched. A step's patches, or the piece's own, that share an alternative must list the same alternatives in the same order, so they all edit the same file.",
					"type": "array",
					"items": { "type": "string" },
					"minItems": 1
				}
			]
		})
	}
}

/// Writes one alternative as a string and several as a list.
fn serialize_alternatives<S: Serializer>(alternatives: &[String], serializer: S) -> Result<S::Ok, S::Error> {
	match alternatives {
		[one] => serializer.serialize_str(one),
		several => several.serialize(serializer),
	}
}

/// Reads a string or a non-empty list of strings; holds what it expects, for errors.
struct AlternativesVisitor(&'static str);

impl<'de> serde::de::Visitor<'de> for AlternativesVisitor {
	type Value = Vec<String>;

	fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.0)
	}

	fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Vec<String>, E> {
		Ok(vec![text.to_owned()])
	}

	fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
		let mut alternatives = Vec::new();
		while let Some(alternative) = seq.next_element::<String>()? {
			alternatives.push(alternative);
		}
		if alternatives.is_empty() {
			return Err(serde::de::Error::invalid_length(0, &self));
		}
		Ok(alternatives)
	}
}

/// `items` in backticks, e.g. `` `a`, `b` or `c` ``.
pub(crate) fn or_list<T: fmt::Display>(items: &[T]) -> String {
	let mut text = String::new();
	for (i, item) in items.iter().enumerate() {
		let separator = match i {
			0 => "",
			_ if i + 1 == items.len() => " or ",
			_ => ", ",
		};
		text.push_str(separator);
		text.push('`');
		text.push_str(&item.to_string());
		text.push('`');
	}
	text
}

/// A file that two different lists of alternatives in `lists` share, which could send patches meant for one
/// file to two.
pub(crate) fn shared_alternative<'a, T: PartialEq>(lists: &[&'a [T]]) -> Option<&'a T> {
	lists.iter().enumerate().find_map(|(i, list)| {
		lists[i + 1..]
			.iter()
			.filter(|other| other != &list)
			.find_map(|other| list.iter().find(|file| other.contains(file)))
	})
}

/// Where an anchor patch inserts its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Position {
	/// On the lines after the anchor.
	#[default]
	After,
	/// On the lines before the anchor.
	Before,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn name_should_accept_kebab_case() {
		assert!(Name::new("react-component2").is_ok());
	}

	#[test]
	fn name_should_reject_uppercase() {
		assert!(Name::new("Component").is_err());
	}

	#[test]
	fn name_should_reject_double_dash() {
		assert!(Name::new("a--b").is_err());
	}

	#[test]
	fn piece_manifest_should_parse_tagged_inputs() {
		let manifest: PieceManifest = toml::from_str(
			r#"
			schema_version = 1
			name = "component"
			target = "src/components"

			[inputs.name]
			type = "string"
			example = "MyExample"

			[inputs.styled]
			type = "bool"
			default = false
			"#,
		)
		.unwrap();

		assert!(matches!(
			manifest.inputs.get("styled"),
			Some(Input::Bool {
				default: Some(false),
				..
			})
		));
	}

	fn anchor_of(patch: &str) -> Result<Anchors, toml::de::Error> {
		let manifest: PieceManifest = toml::from_str(&format!(
			"schema_version = 1\nname = \"x\"\ntarget = \"src\"\n[[patches]]\nkind = \"anchor\"\nfile = \"a.ts\"\ninsert = \"x\"\n{patch}\n"
		))?;
		match manifest.patches.into_iter().next() {
			Some(PatchSpec::Anchor { anchor, .. }) => Ok(anchor),
			other => panic!("expected an anchor patch, got {other:?}"),
		}
	}

	#[test]
	fn anchor_should_accept_one_text_and_write_it_back_as_a_string() {
		let anchor = anchor_of("anchor = \"export default app\"").unwrap();

		assert_eq!(anchor.iter().collect::<Vec<_>>(), ["export default app"]);
		assert_eq!(serde_json::to_value(&anchor).unwrap(), "export default app");
	}

	#[test]
	fn anchor_should_accept_alternatives_in_order() {
		let anchor = anchor_of("anchor = [\"export default app\", \"serve(\"]").unwrap();

		assert_eq!(anchor.iter().collect::<Vec<_>>(), ["export default app", "serve("]);
		assert_eq!(
			serde_json::to_value(&anchor).unwrap(),
			serde_json::json!(["export default app", "serve("])
		);
	}

	#[test]
	fn anchor_should_reject_an_empty_list() {
		let error = anchor_of("anchor = []").unwrap_err();

		assert!(error.to_string().contains("non-empty list of alternatives"), "{error}");
	}

	#[test]
	fn anchors_should_display_as_a_backticked_list() {
		let one = Anchors::from("a");
		let three = Anchors::new(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]).unwrap();

		assert_eq!(one.to_string(), "`a`");
		assert_eq!(three.to_string(), "`a`, `b` or `c`");
		assert_eq!(Anchors::new(Vec::new()), None);
	}

	fn piece_with_import(file: &str) -> Result<PieceManifest, toml::de::Error> {
		toml::from_str(&format!(
			"schema_version = 1\nname = \"x\"\ntarget = \"src\"\n[[patches]]\nkind = \"import\"\n{file}\nimport = \"import a from 'a';\"\n"
		))
	}

	fn file_of(file: &str) -> Result<Files, toml::de::Error> {
		let manifest = piece_with_import(file)?;
		Ok(manifest.patches[0].file().clone())
	}

	#[test]
	fn file_should_accept_one_path_and_write_it_back_as_a_string() {
		let file = file_of("file = \"src/index.ts\"").unwrap();

		assert_eq!(file.as_slice(), ["src/index.ts"]);
		assert_eq!(serde_json::to_value(&file).unwrap(), "src/index.ts");
	}

	#[test]
	fn file_should_accept_alternatives_in_order_and_write_them_back_as_a_list() {
		let file = file_of("file = [\"src/index.ts\", \"src/index.tsx\"]").unwrap();

		assert_eq!(file.as_slice(), ["src/index.ts", "src/index.tsx"]);
		assert_eq!(
			serde_json::to_value(&file).unwrap(),
			serde_json::json!(["src/index.ts", "src/index.tsx"])
		);
	}

	#[test]
	fn file_should_reject_an_empty_list() {
		let error = file_of("file = []").unwrap_err();

		assert!(
			error
				.to_string()
				.contains("a project file or a non-empty list of alternatives"),
			"{error}"
		);
	}

	#[test]
	fn manifest_should_keep_a_single_file_a_string_when_written_back() {
		let one = toml::to_string(&piece_with_import("file = \"src/index.ts\"").unwrap()).unwrap();
		let two = toml::to_string(&piece_with_import("file = [\"src/index.ts\", \"src/index.tsx\"]").unwrap()).unwrap();

		assert!(one.contains("file = \"src/index.ts\"\n"), "{one}");
		assert!(two.contains("file = [\"src/index.ts\", \"src/index.tsx\"]\n"), "{two}");
		assert_eq!(
			piece_with_import("file = \"src/index.ts\"").unwrap(),
			toml::from_str(&one).unwrap()
		);
	}

	#[test]
	fn shared_alternative_should_find_a_file_that_two_different_lists_share() {
		let entries = ["src/index.ts", "src/index.tsx"];
		let reversed = ["src/index.tsx", "src/index.ts"];

		assert_eq!(
			shared_alternative::<&str>(&[&entries, &entries, &["src/main.ts"]]),
			None
		);
		assert_eq!(
			shared_alternative::<&str>(&[&entries, &["src/main.ts"], &["src/index.tsx"]]),
			Some(&"src/index.tsx")
		);
		assert_eq!(
			shared_alternative::<&str>(&[&entries, &reversed]),
			Some(&"src/index.ts")
		);
	}

	#[test]
	fn piece_manifest_should_reject_unknown_fields() {
		let result =
			toml::from_str::<PieceManifest>("schema_version = 1\nname = \"x\"\ntarget = \"src\"\nunknown = true\n");

		assert!(result.is_err());
	}
}
