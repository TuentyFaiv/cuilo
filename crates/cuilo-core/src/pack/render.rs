//! Renders a piece into files, patches and steps, without touching the disk.

use std::collections::BTreeSet;

use serde::Serialize;

use super::Piece;
use crate::model::{Dependency, Name, PatchSpec, PathError, RelPath, Stack, Values, Variant, shared_alternative};
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
	/// Whether `target` is the piece's or variant's own folder, which wiring patches assume.
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
	#[error("no variant matches this project; available: {}", .available.join(", "))]
	NoVariant {
		/// Variant names.
		available: Vec<String>,
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

/// The output folder `request` renders into, or `None` for the project root.
pub(crate) fn target(request: &RenderRequest<'_>) -> Result<Option<RelPath>, RenderError> {
	let (_, own_target, _) = choose(request)?;
	Ok(request.target.cloned().or(own_target))
}

/// The variant `request` uses, if the piece has variants, the piece's or that variant's own output
/// folder, and the replacer of its examples.
fn choose<'a>(request: &RenderRequest<'a>) -> Result<(Option<&'a Variant>, Option<RelPath>, Replacer), RenderError> {
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
				})?,
		)
	};
	let replacer = Replacer::new(&request.piece.examples(request.values))?;
	let own_target = default_target(
		variant
			.and_then(|variant| variant.target.as_ref())
			.unwrap_or(&manifest.target),
		&replacer,
	)?;
	Ok((variant, own_target, replacer))
}

/// Renders `request`.
pub(crate) fn render(request: &RenderRequest<'_>) -> Result<Rendered, RenderError> {
	let manifest = request.piece.manifest();
	let (variant, own_target, replacer) = choose(request)?;

	let mut conditions = request.stack.condition_values();
	conditions.extend(request.values.iter().map(|(name, value)| (name.clone(), value.clone())));

	let target = request.target.cloned().or_else(|| own_target.clone());

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
		at_default_target: target == own_target,
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
	use std::collections::BTreeMap;

	use super::*;
	use crate::model::{Fact, Value};
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
