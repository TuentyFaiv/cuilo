//! Renders a piece into files, patches and steps, without touching the disk.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::Piece;
use crate::model::{
	Dependency, Fact, Name, PatchSpec, PathError, RelPath, Stack, Targets, Values, Variant, shared_alternative,
};
use crate::patch::Patch;
use crate::template::{self, BlockError, Expr, ExprError, ReplaceError, Replacer};

/// What to render.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderRequest<'a> {
	/// The piece.
	pub piece: &'a Piece,
	/// Resolved input values.
	pub values: &'a Values,
	/// Project stack facts.
	pub stack: &'a Stack,
	/// Variant to use instead of the first matching one.
	pub variant: Option<&'a str>,
	/// Output folder instead of the piece's target.
	pub target: Option<&'a RelPath>,
}

/// A rendered piece.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Rendered {
	/// The variant used, if the piece has variants.
	pub variant: Option<Name>,
	/// The output folder used, or `None` for the project root.
	pub target: Option<RelPath>,
	/// Files to create.
	pub files: Vec<RenderedFile>,
	/// Edits to existing files.
	pub patches: Vec<RenderedPatch>,
	/// Packages the files import.
	pub dependencies: Vec<Dependency>,
	/// Steps whose variants and conditions hold.
	pub steps: Vec<RenderedStep>,
	/// Whether `target` is one of the piece's or variant's own folders, which wiring patches assume.
	pub at_default_target: bool,
}

/// A step with examples replaced in its text and patches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct RenderedStep {
	/// What the user does.
	pub text: String,
	/// Edits that do it; empty when only the user can.
	pub patches: Vec<RenderedPatch>,
}

/// A patch with examples replaced and the project files it may edit, in order of preference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct RenderedPatch {
	/// Never empty.
	files: Vec<RelPath>,
	/// Edits the first of `files`.
	patch: Patch,
}

impl RenderedPatch {
	/// The project files it may edit, in order of preference; never empty.
	pub fn files(&self) -> &[RelPath] {
		&self.files
	}

	/// The first of [`Self::files`].
	pub fn preferred_file(&self) -> &RelPath {
		self.patch.file()
	}

	/// The patch editing `file`, one of [`Self::files`].
	pub fn at(&self, file: &RelPath) -> Patch {
		let mut patch = self.patch.clone();
		let (Patch::Anchor { file: edited, .. } | Patch::Import { file: edited, .. } | Patch::CssVars { file: edited, .. }) =
			&mut patch;
		edited.clone_from(file);
		patch
	}
}

/// One rendered file.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RenderedFile {
	/// Project-relative path.
	pub path: RelPath,
	/// File contents.
	#[serde(skip)]
	pub contents: Vec<u8>,
}

impl std::fmt::Debug for RenderedFile {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("RenderedFile")
			.field("path", &self.path)
			.field("len", &self.contents.len())
			.finish()
	}
}

/// Why a piece could not be rendered.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
	/// No variant matches the project's stack.
	#[error("no variant matches this project{}; available: {}", mismatches(.majors), .available.join(", "))]
	NoVariant {
		/// Variant names.
		available: Vec<String>,
		/// Frameworks the project uses whose variants need another major.
		majors: Vec<MajorMismatch>,
	},
	/// The requested variant does not exist.
	#[error("variant `{0}` does not exist")]
	UnknownVariant(String),
	/// A template's directives are invalid.
	#[error("template `{path}`: {source}")]
	Template {
		/// Template path inside the piece.
		path: String,
		/// The cause.
		source: BlockError,
	},
	/// A condition is invalid.
	#[error(transparent)]
	Condition(#[from] ExprError),
	/// Examples conflict.
	#[error(transparent)]
	Replace(#[from] ReplaceError),
	/// A rendered path is invalid.
	#[error("rendered path `{path}` is invalid: {source}")]
	Path {
		/// The rendered path.
		path: String,
		/// The cause.
		source: PathError,
	},
	/// Two templates render to the same path.
	#[error("two templates render to `{0}`")]
	Collision(RelPath),
	/// Two patches of one step, or of the piece itself, render different file alternatives that share a file.
	#[error("two patches list `{0}` among different file alternatives")]
	SharedAlternative(RelPath),
}

/// A framework the project uses in a major its variants do not support, e.g. Solid 1 where they need `solid-v2`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MajorMismatch {
	/// The framework, e.g. `solid`.
	pub framework: String,
	/// The values the variants need, e.g. `solid-v2`.
	pub needed: BTreeSet<String>,
	/// The project's values, e.g. `solid-v1`; empty when cuilo could not read the framework's version.
	pub found: Vec<String>,
}

/// What [`RenderError::NoVariant`] says about `majors` after "no variant matches this project", or nothing.
fn mismatches(majors: &[MajorMismatch]) -> String {
	let explained: Vec<String> = majors
		.iter()
		.map(|major| {
			let needed = major.needed.iter().cloned().collect::<Vec<_>>().join(" or ");
			let found = if major.found.is_empty() {
				format!("cuilo cannot read which {} version the project has", major.framework)
			} else {
				format!("the project has {}", major.found.join(", "))
			};
			format!("the variants for {} need {needed}, and {found}", major.framework)
		})
		.collect();
	if explained.is_empty() {
		String::new()
	} else {
		format!(": {}", explained.join("; "))
	}
}

/// A framework value's framework and major, e.g. `("echo", "5")` for `echo-v5`.
fn framework_major(value: &str) -> Option<(&str, &str)> {
	value.rsplit_once("-v").filter(|(framework, major)| {
		!framework.is_empty() && !major.is_empty() && major.bytes().all(|byte| byte.is_ascii_digit())
	})
}

/// The frameworks of `stack` whose variants among `variants` fail only for lack of another major of them.
fn major_mismatches(variants: &[Variant], stack: &Stack) -> Vec<MajorMismatch> {
	let mut needed: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
	for variant in variants {
		let majors: Option<Vec<(&str, &String)>> = variant
			.when
			.iter()
			.filter(|(fact, value)| !stack.has(**fact, value))
			.map(|(fact, value)| {
				let (framework, _) = framework_major(value)?;
				(*fact == Fact::Framework && stack.has(Fact::Framework, framework)).then_some((framework, value))
			})
			.collect();
		for (framework, value) in majors.into_iter().flatten() {
			needed.entry(framework).or_default().insert(value.clone());
		}
	}
	needed
		.into_iter()
		.map(|(framework, needed)| MajorMismatch {
			framework: framework.to_owned(),
			needed,
			found: stack
				.values(Fact::Framework)
				.filter(|value| framework_major(value).is_some_and(|(of, _)| of == framework))
				.map(str::to_owned)
				.collect(),
		})
		.collect()
}

/// The output folder `request` renders into, or `None` for the project root.
pub(crate) fn target(request: &RenderRequest<'_>) -> Result<Option<RelPath>, RenderError> {
	let (_, own_targets, _) = choose(request)?;
	Ok(
		request
			.target
			.cloned()
			.or_else(|| own_targets.into_iter().next().flatten()),
	)
}

/// The piece's or its variant's own output folders for `request`, in order of preference, with `None` for the
/// project root; never empty.
pub(crate) fn own_targets(request: &RenderRequest<'_>) -> Result<Vec<Option<RelPath>>, RenderError> {
	choose(request).map(|(_, own_targets, _)| own_targets)
}

/// The variant a request uses, if the piece has variants, the piece's or that variant's own output folders in order
/// of preference, and the replacer of its examples.
type Choice<'a> = (Option<&'a Variant>, Vec<Option<RelPath>>, Replacer);

/// What `request` renders with.
fn choose<'a>(request: &RenderRequest<'a>) -> Result<Choice<'a>, RenderError> {
	let manifest = request.piece.manifest();
	let variant = if manifest.variants.is_empty() {
		None
	} else if let Some(name) = request.variant {
		Some(
			manifest
				.variants
				.iter()
				.find(|variant| variant.name.as_str() == name)
				.ok_or_else(|| RenderError::UnknownVariant(name.to_owned()))?,
		)
	} else {
		Some(
			manifest
				.variants
				.iter()
				.find(|variant| request.stack.satisfies(&variant.when))
				.ok_or_else(|| RenderError::NoVariant {
					available: manifest
						.variants
						.iter()
						.map(|variant| variant.name.to_string())
						.collect(),
					majors: major_mismatches(&manifest.variants, request.stack),
				})?,
		)
	};
	let replacer = Replacer::new(&request.piece.examples(request.values))?;
	let own_targets = variant
		.and_then(|variant| variant.target.as_ref())
		.map_or_else(|| std::slice::from_ref(&manifest.target), Targets::as_slice)
		.iter()
		.map(|target| default_target(target, &replacer))
		.collect::<Result<_, _>>()?;
	Ok((variant, own_targets, replacer))
}

/// Renders `request`.
pub(crate) fn render(request: &RenderRequest<'_>) -> Result<Rendered, RenderError> {
	let manifest = request.piece.manifest();
	let (variant, own_targets, replacer) = choose(request)?;

	let mut conditions = request.stack.condition_values();
	conditions.extend(request.values.iter().map(|(name, value)| (name.clone(), value.clone())));

	let target = request
		.target
		.cloned()
		.or_else(|| own_targets.first().cloned().flatten());

	let dir = Piece::files_dir(variant);
	let mut seen = BTreeSet::new();
	let mut files = Vec::new();
	for (template_path, bytes) in request.piece.templates_in(&dir) {
		if let Some(rule) = manifest.files.iter().find(|rule| rule.path == template_path)
			&& !Expr::parse(&rule.when)?.eval(&conditions)
		{
			continue;
		}
		let relative = rel_path(&replacer.apply(template_path))?;
		let path = target
			.as_ref()
			.map_or_else(|| relative.clone(), |target| target.join(&relative));
		if !seen.insert(path.clone()) {
			return Err(RenderError::Collision(path));
		}
		let contents = if template::is_binary(bytes) {
			bytes.to_vec()
		} else {
			let text = String::from_utf8_lossy(bytes);
			template::render(&text, &conditions, &replacer)
				.map_err(|source| RenderError::Template {
					path: format!("{dir}/{template_path}"),
					source,
				})?
				.into_bytes()
		};
		files.push(RenderedFile { path, contents });
	}

	let mut dependencies = Vec::new();
	for dependency in &manifest.dependencies {
		if selected(&dependency.variants, dependency.when.as_deref(), variant, &conditions)? {
			dependencies.push(dependency.clone());
		}
	}
	let mut steps = Vec::new();
	for step in &manifest.steps {
		if selected(&step.variants, step.when.as_deref(), variant, &conditions)? {
			steps.push(RenderedStep {
				text: replacer.apply(&step.text),
				patches: patches(&step.patches, &replacer, &conditions)?,
			});
		}
	}

	Ok(Rendered {
		patches: patches(&manifest.patches, &replacer, &conditions)?,
		variant: variant.map(|variant| variant.name.clone()),
		at_default_target: own_targets.contains(&target),
		target,
		files,
		dependencies,
		steps,
	})
}

/// Whether something limited to `variants` and `when` applies to `variant` and `conditions`.
fn selected(
	variants: &[Name],
	when: Option<&str>,
	variant: Option<&Variant>,
	conditions: &Values,
) -> Result<bool, RenderError> {
	if !variants.is_empty() && !variant.is_some_and(|variant| variants.contains(&variant.name)) {
		return Ok(false);
	}
	match when {
		Some(when) => Ok(Expr::parse(when)?.eval(conditions)),
		None => Ok(true),
	}
}

/// The piece's or variant's output folder, or `None` when it is the project root.
fn default_target(target: &str, replacer: &Replacer) -> Result<Option<RelPath>, RenderError> {
	let target = replacer.apply(target);
	if target.is_empty() {
		Ok(None)
	} else {
		rel_path(&target).map(Some)
	}
}

/// The patches whose conditions hold, with examples replaced in their files and text; patches that share a file
/// alternative must render the same alternatives.
fn patches(specs: &[PatchSpec], replacer: &Replacer, conditions: &Values) -> Result<Vec<RenderedPatch>, RenderError> {
	let mut patches = Vec::new();
	for spec in specs {
		if let Some(when) = spec.when()
			&& !Expr::parse(when)?.eval(conditions)
		{
			continue;
		}
		let files = spec
			.file()
			.as_slice()
			.iter()
			.map(|file| rel_path(&replacer.apply(file)))
			.collect::<Result<Vec<_>, _>>()?;
		let Some(file) = files.first().cloned() else {
			continue;
		};
		let patch = match spec {
			PatchSpec::Anchor {
				anchor,
				position,
				insert,
				..
			} => Patch::Anchor {
				file,
				anchor: anchor.clone(),
				position: *position,
				insert: replacer.apply(insert),
			},
			PatchSpec::Import { import, .. } => Patch::Import {
				file,
				import: replacer.apply(import),
			},
			PatchSpec::CssVars { selector, vars, .. } => Patch::CssVars {
				file,
				selector: selector.clone(),
				vars: vars.clone(),
			},
		};
		patches.push(RenderedPatch { files, patch });
	}
	let lists: Vec<&[RelPath]> = patches.iter().map(RenderedPatch::files).collect();
	match shared_alternative(&lists) {
		Some(shared) => Err(RenderError::SharedAlternative(shared.clone())),
		None => Ok(patches),
	}
}

fn rel_path(path: &str) -> Result<RelPath, RenderError> {
	RelPath::new(path).map_err(|source| RenderError::Path {
		path: path.to_owned(),
		source,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::model::Value;
	use crate::pack::Pack;

	const PACK: &str = "schema_version = 1\nname = \"p\"\nversion = \"1.0.0\"\n";

	fn pack(piece: &str, templates: &[&str]) -> Pack {
		let mut files = BTreeMap::from([
			("pack.toml".to_owned(), PACK.as_bytes().to_vec()),
			("pieces/thing/piece.toml".to_owned(), piece.as_bytes().to_vec()),
		]);
		for path in templates {
			files.insert(format!("pieces/thing/{path}"), b"x\n".to_vec());
		}
		Pack::from_files(files).unwrap()
	}

	fn paths(pack: &Pack, stack: &Stack) -> Vec<String> {
		let values = Values::from([("name".to_owned(), Value::String("UserCard".to_owned()))]);
		let rendered = render(&RenderRequest {
			piece: pack.piece("thing").unwrap(),
			values: &values,
			stack,
			variant: None,
			target: None,
		})
		.unwrap();
		rendered.files.iter().map(|file| file.path.to_string()).collect()
	}

	#[test]
	fn render_should_use_the_variant_target() {
		let pack = pack(
			"schema_version = 1\nname = \"thing\"\ntarget = \"src/ui\"\n[inputs.name]\ntype = \"string\"\nexample = \"MyThing\"\n\
			 [[variants]]\nname = \"flutter\"\ntarget = \"lib/ui\"\nwhen = { framework = \"flutter\" }\n",
			&["files/flutter/my_thing.dart"],
		);
		let mut stack = Stack::default();
		stack.insert(Fact::Framework, "flutter");

		assert_eq!(paths(&pack, &stack), ["lib/ui/user_card.dart"]);
	}

	#[test]
	fn render_should_default_to_the_first_target_alternative_and_count_each_as_its_own() {
		let pack = pack(
			"schema_version = 1\nname = \"thing\"\ntarget = \"src\"\n[inputs.name]\ntype = \"string\"\nexample = \"MyThing\"\n\
			 [[variants]]\nname = \"kmp\"\ntarget = [\"shared\", \"composeApp\"]\n",
			&["files/kmp/my_thing.kt"],
		);
		let values = Values::from([("name".to_owned(), Value::String("UserCard".to_owned()))]);
		let stack = Stack::default();
		let request = |target| RenderRequest {
			piece: pack.piece("thing").unwrap(),
			values: &values,
			stack: &stack,
			variant: None,
			target,
		};
		let (shared, legacy, other) = (
			RelPath::new("shared").unwrap(),
			RelPath::new("composeApp").unwrap(),
			RelPath::new("other").unwrap(),
		);

		let first = render(&request(None)).unwrap();
		let second = render(&request(Some(&legacy))).unwrap();
		let elsewhere = render(&request(Some(&other))).unwrap();

		assert_eq!(
			own_targets(&request(None)).unwrap(),
			[Some(shared.clone()), Some(legacy.clone())]
		);
		assert_eq!(first.target, Some(shared));
		assert_eq!(second.files[0].path.as_str(), "composeApp/user_card.kt");
		assert!(first.at_default_target && second.at_default_target && !elsewhere.at_default_target);
	}

	fn render_named(pack: &Pack, name: &str) -> Result<Rendered, RenderError> {
		let values = Values::from([("name".to_owned(), Value::String(name.to_owned()))]);
		render(&RenderRequest {
			piece: pack.piece("thing").unwrap(),
			values: &values,
			stack: &Stack::default(),
			variant: None,
			target: None,
		})
	}

	const THING: &str =
		"schema_version = 1\nname = \"thing\"\ntarget = \"src\"\n[inputs.name]\ntype = \"string\"\nexample = \"MyThing\"\n";

	#[test]
	fn render_should_replace_examples_in_every_file_alternative() {
		let pack = pack(
			&format!(
				"{THING}[[steps]]\ntext = \"Wire `MyThing`.\"\n[[steps.patches]]\nkind = \"import\"\n\
				 file = [\"src/my-thing/index.ts\", \"src/my-thing/index.tsx\"]\nimport = \"import {{ myThing }} from './my-thing';\"\n"
			),
			&["files/my-thing.ts"],
		);

		let rendered = render_named(&pack, "UserCard").unwrap();
		let patch = &rendered.steps[0].patches[0];

		let tsx = RelPath::new("src/user-card/index.tsx").unwrap();
		assert_eq!(
			patch.files(),
			[RelPath::new("src/user-card/index.ts").unwrap(), tsx.clone()]
		);
		assert_eq!(patch.preferred_file().as_str(), "src/user-card/index.ts");
		assert_eq!(
			patch.at(&tsx),
			Patch::Import {
				file: tsx,
				import: "import { userCard } from './user-card';".to_owned()
			}
		);
	}

	#[test]
	fn render_should_refuse_alternatives_that_share_a_file_only_once_examples_are_replaced() {
		let pack = pack(
			&format!(
				"{THING}[[steps]]\ntext = \"x\"\n[[steps.patches]]\nkind = \"import\"\nfile = [\"src/my-thing.ts\", \"src/my-thing.tsx\"]\n\
				 import = \"import a from 'a';\"\n[[steps.patches]]\nkind = \"import\"\nfile = \"src/index.tsx\"\nimport = \"import b from 'b';\"\n"
			),
			&["files/my-thing.ts"],
		);

		assert!(render_named(&pack, "UserCard").is_ok());
		assert!(matches!(
			render_named(&pack, "Index"),
			Err(RenderError::SharedAlternative(file)) if file.as_str() == "src/index.tsx"
		));
	}

	#[test]
	fn no_variant_should_name_the_framework_major_its_variants_need() {
		let pack = pack(
			"schema_version = 1\nname = \"thing\"\ntarget = \"src\"\n\
			 [[variants]]\nname = \"solid\"\nwhen = { framework = \"solid-v2\", language = \"typescript\" }\n\
			 [[variants]]\nname = \"echo\"\nwhen = { framework = \"echo-v4\" }\n\
			 [[variants]]\nname = \"echo-v5\"\nwhen = { framework = \"echo-v5\" }\n",
			&["files/solid/thing.tsx", "files/echo/thing.go", "files/echo-v5/thing.go"],
		);
		let message = |facts: &[(Fact, &str)]| {
			let mut stack = Stack::default();
			for (fact, value) in facts {
				stack.insert(*fact, value);
			}
			render(&RenderRequest {
				piece: pack.piece("thing").unwrap(),
				values: &Values::new(),
				stack: &stack,
				variant: None,
				target: None,
			})
			.unwrap_err()
			.to_string()
		};
		let typescript = (Fact::Language, "typescript");

		assert_eq!(
			message(&[(Fact::Framework, "solid"), (Fact::Framework, "solid-v1"), typescript]),
			"no variant matches this project: the variants for solid need solid-v2, and the project has solid-v1; \
			 available: solid, echo, echo-v5"
		);
		assert_eq!(
			message(&[(Fact::Framework, "echo")]),
			"no variant matches this project: the variants for echo need echo-v4 or echo-v5, and cuilo cannot read \
			 which echo version the project has; available: solid, echo, echo-v5"
		);
		assert_eq!(
			message(&[(Fact::Framework, "solid"), (Fact::Framework, "solid-v1")]),
			"no variant matches this project; available: solid, echo, echo-v5"
		);
		assert_eq!(
			message(&[(Fact::Framework, "react"), typescript]),
			"no variant matches this project; available: solid, echo, echo-v5"
		);
	}

	#[test]
	fn render_should_write_from_the_root_when_target_is_empty() {
		let pack = pack(
			"schema_version = 1\nname = \"thing\"\ntarget = \"\"\n[inputs.name]\ntype = \"string\"\nexample = \"MyThing\"\n",
			&["files/lib/my_thing.dart", "files/test/my_thing_test.dart"],
		);

		assert_eq!(
			paths(&pack, &Stack::default()),
			["lib/user_card.dart", "test/user_card_test.dart"]
		);
	}
}
