//! Inserts text next to the line holding an anchor, or the first of its alternatives in the file, matching
//! that line's indentation; works in any language.

use crate::model::{Anchors, Position};
use crate::patch::Patch;
use crate::patch::{PatchError, Patched, Patcher, line_ending};

/// The built-in anchor patcher.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnchorPatcher;

impl Patcher for AnchorPatcher {
	fn apply(&self, text: &str, patch: &Patch) -> Result<Patched, PatchError> {
		let Patch::Anchor {
			anchor,
			position,
			insert,
			..
		} = patch
		else {
			return Err(PatchError::Invalid(
				"anchor patcher received another patch kind".to_owned(),
			));
		};

		let lines: Vec<&str> = text.split_inclusive('\n').collect();
		let index = anchor_line(&lines, anchor)?;

		let anchor_line = lines[index];
		let indent: String = anchor_line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
		let ending = line_ending(text);
		let inserted: String = insert
			.lines()
			.map(|line| {
				if line.is_empty() {
					ending.to_owned()
				} else {
					format!("{indent}{line}{ending}")
				}
			})
			.collect();

		if text.contains(&inserted) {
			return Ok(Patched::AlreadyApplied { inserted });
		}

		let mut output = String::with_capacity(text.len() + inserted.len() + ending.len());
		for (i, line) in lines.iter().enumerate() {
			if i == index && *position == Position::Before {
				output.push_str(&inserted);
			}
			output.push_str(line);
			if i == index && *position == Position::After {
				if !line.ends_with('\n') {
					output.push_str(ending);
				}
				output.push_str(&inserted);
			}
		}
		Ok(Patched::Changed { text: output, inserted })
	}
}

/// The index of the line holding the first alternative of `anchors` found in `lines`.
///
/// An alternative found on several lines is an error, and the later ones are not tried: the place the pack
/// prefers is in the file, but which line it means is unclear.
fn anchor_line(lines: &[&str], anchors: &Anchors) -> Result<usize, PatchError> {
	for anchor in anchors.iter() {
		let mut matches = lines
			.iter()
			.enumerate()
			.filter(|(_, line)| line.contains(anchor))
			.map(|(i, _)| i);
		let Some(index) = matches.next() else {
			continue;
		};
		let others = matches.count();
		if others > 0 {
			return Err(PatchError::AmbiguousAnchor {
				anchor: anchor.to_owned(),
				count: others + 1,
			});
		}
		return Ok(index);
	}
	Err(PatchError::MissingAnchor(anchors.clone()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::model::RelPath;

	fn patch(anchor: &str, position: Position, insert: &str) -> Patch {
		alternatives(&[anchor], position, insert)
	}

	fn alternatives(anchors: &[&str], position: Position, insert: &str) -> Patch {
		Patch::Anchor {
			file: RelPath::constant("src/routes.ts"),
			anchor: Anchors::new(anchors.iter().map(|&anchor| anchor.to_owned()).collect()).unwrap(),
			position,
			insert: insert.to_owned(),
		}
	}

	fn apply(text: &str, patch: &Patch) -> Result<Patched, PatchError> {
		AnchorPatcher.apply(text, patch)
	}

	#[test]
	fn after_should_insert_with_anchor_indentation() {
		let text = "const routes = [\n  // @cuilo:routes\n];\n";
		let result = apply(text, &patch("@cuilo:routes", Position::After, "{ path: '/card' },"));

		assert_eq!(
			result,
			Ok(Patched::Changed {
				text: "const routes = [\n  // @cuilo:routes\n  { path: '/card' },\n];\n".to_owned(),
				inserted: "  { path: '/card' },\n".to_owned()
			})
		);
	}

	#[test]
	fn before_should_insert_above_anchor() {
		let result = apply("a\n// end\n", &patch("// end", Position::Before, "b"));

		assert!(matches!(result, Ok(Patched::Changed { text, .. }) if text == "a\nb\n// end\n"));
	}

	#[test]
	fn second_apply_should_be_idempotent() {
		let patch = patch("// here", Position::After, "x();");
		let Ok(Patched::Changed { text, .. }) = apply("// here\n", &patch) else {
			panic!("first apply must change")
		};

		assert!(matches!(apply(&text, &patch), Ok(Patched::AlreadyApplied { .. })));
	}

	#[test]
	fn missing_anchor_should_fail() {
		assert_eq!(
			apply("nothing\n", &patch("// here", Position::After, "x")),
			Err(PatchError::MissingAnchor(Anchors::from("// here")))
		);
	}

	#[test]
	fn first_alternative_in_the_file_should_win_even_when_a_later_one_is_also_there() {
		let text = "serve(app)\nexport default app\n";
		let patch = alternatives(&["export default app", "serve("], Position::Before, "app.route();");

		assert!(matches!(
			apply(text, &patch),
			Ok(Patched::Changed { text, .. }) if text == "serve(app)\napp.route();\nexport default app\n"
		));
	}

	#[test]
	fn missing_alternatives_should_fall_back_to_the_next_one() {
		let text = "const app = new Hono()\n\nserve({\n  fetch: app.fetch,\n})\n";
		let patch = alternatives(
			&["export default app", "export const handler = handle(app)", "serve("],
			Position::Before,
			"app.route();\n\n",
		);

		assert!(matches!(
			apply(text, &patch),
			Ok(Patched::Changed { text, .. })
				if text == "const app = new Hono()\n\napp.route();\n\nserve({\n  fetch: app.fetch,\n})\n"
		));
	}

	#[test]
	fn no_alternative_in_the_file_should_fail_naming_them_all() {
		let patch = alternatives(&["export default app", "serve("], Position::Before, "x");
		let result = apply("const app = new Hono()\n", &patch);

		assert_eq!(
			result,
			Err(PatchError::MissingAnchor(
				Anchors::new(vec!["export default app".to_owned(), "serve(".to_owned()]).unwrap()
			))
		);
		assert_eq!(
			result.unwrap_err().to_string(),
			"anchor `export default app` or `serve(` not found"
		);
	}

	#[test]
	fn an_ambiguous_alternative_should_fail_without_trying_the_next_one() {
		let text = "serve(a)\nserve(b)\nexport default app\n";
		let patch = alternatives(&["serve(", "export default app"], Position::Before, "x");

		assert_eq!(
			apply(text, &patch),
			Err(PatchError::AmbiguousAnchor {
				anchor: "serve(".to_owned(),
				count: 2
			})
		);
	}

	#[test]
	fn an_alternative_after_a_missing_one_should_still_have_to_be_unique() {
		let text = "serve(a)\nserve(b)\n";
		let patch = alternatives(&["export default app", "serve("], Position::Before, "x");

		assert!(matches!(
			apply(text, &patch),
			Err(PatchError::AmbiguousAnchor { anchor, count: 2 }) if anchor == "serve("
		));
	}

	#[test]
	fn an_alternative_should_take_its_own_lines_indentation() {
		let text = "function main() {\n\tserve(app)\n}\n";
		let patch = alternatives(&["export default app", "serve("], Position::Before, "app.route();");

		assert!(matches!(
			apply(text, &patch),
			Ok(Patched::Changed { inserted, .. }) if inserted == "\tapp.route();\n"
		));
	}

	#[test]
	fn duplicated_anchor_should_fail() {
		assert!(matches!(
			apply("// here\n// here\n", &patch("// here", Position::After, "x")),
			Err(PatchError::AmbiguousAnchor { count: 2, .. })
		));
	}

	#[test]
	fn crlf_files_should_keep_crlf() {
		let result = apply("// here\r\nend\r\n", &patch("// here", Position::After, "x"));

		assert!(matches!(result, Ok(Patched::Changed { text, .. }) if text == "// here\r\nx\r\nend\r\n"));
	}
}
