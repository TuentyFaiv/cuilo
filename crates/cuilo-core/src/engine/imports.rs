//! Writing the relative imports of generated JavaScript and TypeScript the way the project resolves them.

use std::collections::BTreeSet;

use super::Engine;
use crate::imports::{self, Rewritten};
use crate::model::{Fact, ImportStyle, RelPath, Stack};
use crate::pack::RenderedFile;
use crate::patch::Patch;
use crate::plan::{Draft, Plan};
use crate::template;
use crate::workspace;

impl Engine {
	/// `stack` with the import style of code generated in `target`, unless the project config sets one.
	pub(super) fn piece_stack(&self, stack: &Stack, target: Option<&RelPath>) -> Stack {
		let mut stack = stack.clone();
		if !self.config.stack.contains_key(&Fact::ImportStyle)
			&& let Some(style) = workspace::import_style(&self.workspace, target, &stack)
		{
			stack.set_import_style(style);
		}
		stack
	}

	/// Rewrites the relative imports of `files` in `stack`'s import style, looking for the files they name
	/// among `files`, the plan's and the project's; with `plan`, notes the ones that name no file.
	pub(super) fn adapt_files(&self, stack: &Stack, files: &mut [RenderedFile], mut plan: Option<&mut Plan<Draft>>) {
		let style = stack.import_style();
		if style == ImportStyle::Extensionless {
			return;
		}
		let generated: BTreeSet<RelPath> = files.iter().map(|file| file.path.clone()).collect();
		for file in files.iter_mut() {
			if !imports::is_code_file(&file.path) || template::is_binary(&file.contents) {
				continue;
			}
			let text = String::from_utf8_lossy(&file.contents).into_owned();
			let Rewritten { text: next, unresolved } = {
				let planned = plan.as_deref();
				let exists = |path: &RelPath| generated.contains(path) || self.known(planned, path);
				imports::rewrite(
					&text,
					&file.path,
					style,
					self.commonjs(stack, &file.path, &text),
					&exists,
				)
			};
			if next != text {
				file.contents = next.into_bytes();
			}
			if let Some(plan) = plan.as_deref_mut() {
				note_unresolved(plan, &file.path, &unresolved);
			}
		}
	}

	/// Rewrites the relative imports in the text an `import` or `anchor` patch inserts into a code file, in
	/// `stack`'s import style, noting the ones that name no file.
	pub(super) fn adapt_patch(&self, stack: &Stack, patch: &mut Patch, plan: &mut Plan<Draft>) {
		let style = stack.import_style();
		let (file, text) = match patch {
			Patch::Import { file, import } => (file, import),
			Patch::Anchor { file, insert, .. } => (file, insert),
			Patch::CssVars { .. } => return,
		};
		if style == ImportStyle::Extensionless || !imports::is_code_file(file) {
			return;
		}
		let Rewritten { text: next, unresolved } = {
			let planned: &Plan<Draft> = plan;
			let exists = |path: &RelPath| self.known(Some(planned), path);
			imports::rewrite(text, file, style, self.commonjs(stack, file, text), &exists)
		};
		*text = next;
		note_unresolved(plan, file, &unresolved);
	}

	/// Whether `path` is a file the plan writes or the project has.
	fn known(&self, plan: Option<&Plan<Draft>>, path: &RelPath) -> bool {
		plan.is_some_and(|plan| plan.writes(path)) || self.workspace.exists(path).unwrap_or(false)
	}

	/// Whether `require()` calls in `text`, code of the file at `path`, name modules: the file is `CommonJS`.
	fn commonjs(&self, stack: &Stack, path: &RelPath, text: &str) -> bool {
		text.contains("require")
			&& imports::commonjs_by_extension(path).unwrap_or_else(|| {
				!stack.has(Fact::Runtime, "deno") && workspace::is_commonjs(&self.workspace, path.parent().as_ref())
			})
	}
}

fn note_unresolved(plan: &mut Plan<Draft>, file: &RelPath, unresolved: &[String]) {
	for specifier in unresolved {
		plan.note(
			file.clone(),
			format!("no file matches the import `{specifier}`, so cuilo left it as the pack wrote it"),
		);
	}
}
