//! Finds module specifiers in JavaScript and TypeScript without parsing it: a lexer that knows comments,
//! strings, template literals and regular expressions, and the statement forms that name a module.

use std::ops::Range;

/// Keywords after which a `/` starts a regular expression rather than a division.
const BEFORE_EXPRESSION: [&[u8]; 14] = [
	b"return",
	b"typeof",
	b"instanceof",
	b"in",
	b"of",
	b"new",
	b"delete",
	b"void",
	b"throw",
	b"case",
	b"do",
	b"else",
	b"yield",
	b"await",
];

/// What came before the current position, which decides how a `/` reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Previous {
	/// An operator, an opening bracket or the start: a `/` here starts a regular expression.
	Operator,
	/// A value or a closing bracket: a `/` here divides.
	Value,
	/// A `.` or `?.`: the next word is a property name.
	Dot,
}

/// The byte ranges between the quotes of every module specifier in `code`: of `import` and `export … from`
/// statements, dynamic `import()` and, when `commonjs`, `require()` calls.
pub(super) fn specifiers(code: &str, commonjs: bool) -> Vec<Range<usize>> {
	let bytes = code.as_bytes();
	let mut found = Vec::new();
	let mut templates: Vec<bool> = Vec::new();
	let mut previous = Previous::Operator;
	let mut i = if code.starts_with("#!") { line_end(bytes, 0) } else { 0 };
	while let Some(&byte) = bytes.get(i) {
		match byte {
			b if b.is_ascii_whitespace() => i += 1,
			b'/' if matches!(bytes.get(i + 1), Some(b'/' | b'*')) => i = skip_comment(bytes, i),
			b'\'' | b'"' => {
				i = string_end(bytes, i);
				previous = Previous::Value;
			}
			b'`' => (i, previous) = template_text(bytes, i + 1, &mut templates),
			b'{' => {
				templates.push(false);
				previous = Previous::Operator;
				i += 1;
			}
			b'}' => {
				if templates.pop() == Some(true) {
					(i, previous) = template_text(bytes, i + 1, &mut templates);
				} else {
					previous = Previous::Value;
					i += 1;
				}
			}
			b'/' if previous == Previous::Operator && (i == 0 || bytes[i - 1] != b'<') => {
				i = regex_end(bytes, i);
				previous = Previous::Value;
			}
			b')' | b']' => {
				previous = Previous::Value;
				i += 1;
			}
			b'.' if !bytes.get(i + 1).is_some_and(u8::is_ascii_digit) => {
				previous = if bytes[i..].starts_with(b"...") {
					i += 3;
					Previous::Operator
				} else {
					i += 1;
					Previous::Dot
				};
			}
			b'?' if bytes.get(i + 1) == Some(&b'.') && !bytes.get(i + 2).is_some_and(u8::is_ascii_digit) => {
				previous = Previous::Dot;
				i += 2;
			}
			b if is_word_start(b) => {
				let end = word_end(bytes, i + 1);
				let named = (previous != Previous::Dot)
					.then(|| named_module(bytes, i..end, commonjs))
					.flatten();
				if let Some((specifier, next)) = named {
					found.push(specifier);
					previous = Previous::Value;
					i = next;
				} else {
					previous = if BEFORE_EXPRESSION.contains(&&bytes[i..end]) {
						Previous::Operator
					} else {
						Previous::Value
					};
					i = end;
				}
			}
			b if b.is_ascii_digit() || b == b'.' => {
				i = word_end(bytes, i + 1);
				previous = Previous::Value;
			}
			_ => {
				previous = Previous::Operator;
				i += 1;
			}
		}
	}
	found
}

/// Template text from `i` on; a `${` that ends it is pushed on `templates`. Returns where code goes on and
/// how a `/` reads there.
fn template_text(bytes: &[u8], i: usize, templates: &mut Vec<bool>) -> (usize, Previous) {
	let (end, substitution) = template_end(bytes, i);
	if substitution {
		templates.push(true);
		(end, Previous::Operator)
	} else {
		(end, Previous::Value)
	}
}

/// The specifier of the statement or call that the word at `word` starts, if it names a module, and where
/// scanning goes on after it.
fn named_module(bytes: &[u8], word: Range<usize>, commonjs: bool) -> Option<(Range<usize>, usize)> {
	match &bytes[word.clone()] {
		b"import" => import(bytes, word.end),
		b"export" => export(bytes, word.end),
		b"require" if commonjs => call(bytes, word.end),
		_ => None,
	}
}

/// The specifier of the `import` whose keyword ends at `start`, and where scanning goes on after it.
fn import(bytes: &[u8], start: usize) -> Option<(Range<usize>, usize)> {
	let mut i = skip_trivia(bytes, start);
	match bytes.get(i)? {
		b'(' => call(bytes, start),
		b'\'' | b'"' => quoted(bytes, i),
		_ => loop {
			match bytes.get(i)? {
				b'{' => i = braces_end(bytes, i),
				b'*' | b',' => i += 1,
				b'=' => {
					let word = skip_trivia(bytes, i + 1);
					let end = word_end(bytes, word);
					return (&bytes[word..end] == b"require").then(|| call(bytes, end)).flatten();
				}
				&b if is_word_start(b) => {
					let end = word_end(bytes, i + 1);
					if &bytes[i..end] == b"from"
						&& let Some(found) = quoted(bytes, skip_trivia(bytes, end))
					{
						return Some(found);
					}
					i = end;
				}
				_ => return None,
			}
			i = skip_trivia(bytes, i);
		},
	}
}

/// The specifier of the `export … from` whose keyword ends at `start`, and where scanning goes on after it.
fn export(bytes: &[u8], start: usize) -> Option<(Range<usize>, usize)> {
	let mut i = skip_trivia(bytes, start);
	if word_at(bytes, i, b"type") {
		i = skip_trivia(bytes, i + 4);
	}
	match bytes.get(i)? {
		b'{' => i = braces_end(bytes, i),
		b'*' => {
			i = skip_trivia(bytes, i + 1);
			if word_at(bytes, i, b"as") {
				i = skip_trivia(bytes, i + 2);
				i = match bytes.get(i)? {
					b'\'' | b'"' => string_end(bytes, i),
					&b if is_word_start(b) => word_end(bytes, i + 1),
					_ => return None,
				};
			}
		}
		_ => return None,
	}
	i = skip_trivia(bytes, i);
	word_at(bytes, i, b"from")
		.then(|| quoted(bytes, skip_trivia(bytes, i + 4)))
		.flatten()
}

/// The specifier of a `(` call with one string argument right after `start`, as in `import("./a")` or
/// `require("./a")`.
fn call(bytes: &[u8], start: usize) -> Option<(Range<usize>, usize)> {
	let open = skip_trivia(bytes, start);
	if bytes.get(open) != Some(&b'(') {
		return None;
	}
	let quote = skip_trivia(bytes, open + 1);
	let (specifier, end) = match bytes.get(quote)? {
		b'`' => match template_end(bytes, quote + 1) {
			(end, false) if bytes.get(end - 1) == Some(&b'`') => (quote + 1..end - 1, end),
			_ => return None,
		},
		_ => quoted(bytes, quote)?,
	};
	matches!(bytes.get(skip_trivia(bytes, end)), Some(b')' | b',')).then_some((specifier, end))
}

/// The contents of the string literal opening at `start`, and the position after it.
fn quoted(bytes: &[u8], start: usize) -> Option<(Range<usize>, usize)> {
	let quote = *bytes.get(start)?;
	if quote != b'\'' && quote != b'"' {
		return None;
	}
	let end = string_end(bytes, start);
	(bytes.get(end - 1) == Some(&quote) && end - 1 > start).then_some((start + 1..end - 1, end))
}

/// Whether the word `word` starts at `i`, as a whole word.
fn word_at(bytes: &[u8], i: usize, word: &[u8]) -> bool {
	bytes[i.min(bytes.len())..].starts_with(word) && word_end(bytes, i) == i + word.len()
}

fn is_word_start(byte: u8) -> bool {
	byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$' | b'#' | b'@') || byte >= 0x80
}

fn word_end(bytes: &[u8], mut i: usize) -> usize {
	while bytes
		.get(i)
		.is_some_and(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$') || b >= 0x80)
	{
		i += 1;
	}
	i
}

fn line_end(bytes: &[u8], from: usize) -> usize {
	bytes[from..]
		.iter()
		.position(|&b| b == b'\n')
		.map_or(bytes.len(), |p| from + p)
}

/// The position after the comment opening at `start`.
fn skip_comment(bytes: &[u8], start: usize) -> usize {
	if bytes.get(start + 1) == Some(&b'/') {
		return line_end(bytes, start);
	}
	bytes[start + 2..]
		.windows(2)
		.position(|window| window == b"*/")
		.map_or(bytes.len(), |p| start + 2 + p + 2)
}

/// The position after whitespace and comments from `i`.
fn skip_trivia(bytes: &[u8], mut i: usize) -> usize {
	loop {
		match bytes.get(i) {
			Some(b) if b.is_ascii_whitespace() => i += 1,
			Some(b'/') if matches!(bytes.get(i + 1), Some(b'/' | b'*')) => i = skip_comment(bytes, i),
			_ => return i,
		}
	}
}

/// The position after the `'` or `"` string opening at `start`; one left open ends at the line's end, as
/// text in JSX such as `don't` does.
fn string_end(bytes: &[u8], start: usize) -> usize {
	let quote = bytes[start];
	let mut i = start + 1;
	while let Some(&b) = bytes.get(i) {
		match b {
			b'\\' => i += 2,
			b'\n' => return i,
			b if b == quote => return i + 1,
			_ => i += 1,
		}
	}
	bytes.len()
}

/// The position after the template text from `i` and whether a `${` substitution ended it rather than
/// the closing backtick.
fn template_end(bytes: &[u8], mut i: usize) -> (usize, bool) {
	while let Some(&b) = bytes.get(i) {
		match b {
			b'\\' => i += 2,
			b'`' => return (i + 1, false),
			b'$' if bytes.get(i + 1) == Some(&b'{') => return (i + 2, true),
			_ => i += 1,
		}
	}
	(bytes.len(), false)
}

/// The position after the regular expression opening at `start`, flags included.
fn regex_end(bytes: &[u8], start: usize) -> usize {
	let mut i = start + 1;
	let mut class = false;
	while let Some(&b) = bytes.get(i) {
		match b {
			b'\\' => i += 1,
			b'\n' => return i,
			b'[' => class = true,
			b']' => class = false,
			b'/' if !class => return word_end(bytes, i + 1),
			_ => {}
		}
		i += 1;
	}
	bytes.len()
}

/// The position after the `{ … }` opening at `start`, skipping strings and comments inside.
fn braces_end(bytes: &[u8], start: usize) -> usize {
	let mut depth = 0_usize;
	let mut i = start;
	while let Some(&b) = bytes.get(i) {
		match b {
			b'{' => depth += 1,
			b'}' => {
				depth -= 1;
				if depth == 0 {
					return i + 1;
				}
			}
			b'\'' | b'"' => {
				i = string_end(bytes, i);
				continue;
			}
			b'/' if matches!(bytes.get(i + 1), Some(b'/' | b'*')) => {
				i = skip_comment(bytes, i);
				continue;
			}
			_ => {}
		}
		i += 1;
	}
	bytes.len()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn found(code: &str, commonjs: bool) -> Vec<&str> {
		specifiers(code, commonjs)
			.into_iter()
			.map(|range| &code[range])
			.collect()
	}

	#[test]
	fn should_find_every_import_and_export_form() {
		let code = r#"import a from "./a";
import { b, type B } from './b';
import type { C } from "./c";
import * as d from "./d";
import e, { f } from "./e";
import "./side-effect";
import {
	g, // the g
	h,
} from "./gh";
import from from "./from";
import { from as other } from "./from-name";
import { "string name" as s } from "./string-name";
export * from "./star";
export * as ns from "./ns";
export { i } from "./i";
export type { J } from "./j";
export type * from "./type-star";
export { default } from "./default";
const lazy = () => import("./lazy");
type T = typeof import("./type-query");
import x = require("./import-equals");
"#;

		assert_eq!(
			found(code, false),
			[
				"./a",
				"./b",
				"./c",
				"./d",
				"./e",
				"./side-effect",
				"./gh",
				"./from",
				"./from-name",
				"./string-name",
				"./star",
				"./ns",
				"./i",
				"./j",
				"./type-star",
				"./default",
				"./lazy",
				"./type-query",
				"./import-equals"
			]
		);
	}

	#[test]
	fn should_skip_exports_and_imports_that_name_no_module() {
		let code = "export const a = 1;\nexport { a as b };\nexport default function () {}\nconst url = import.meta.url;\nconst m = { import: \"./x\", from: \"./y\" };\nclass C { import() {} }\nobj.import(\"./z\");\n";

		assert!(found(code, true).is_empty(), "{:?}", found(code, true));
	}

	#[test]
	fn should_find_require_calls_only_in_commonjs() {
		let code = "const a = require(\"./a\");\nconst b = x.require(\"./b\");\nconst c = require.resolve(\"./c\");\n";

		assert_eq!(found(code, true), ["./a"]);
		assert_eq!(found(code, false), Vec::<&str>::new());
	}

	#[test]
	fn should_ignore_comments_and_strings_that_look_like_imports() {
		let code = r#"// import a from "./comment";
/* export * from "./block"; */
const text = "import b from './string'";
const other = 'require("./single")';
const template = `import c from "./template" ${ { lazy: import("./substitution") }.lazy } import("./text")`;
const regex = /import d from "\.\/regex"/;
import real from "./real";
"#;

		assert_eq!(found(code, true), ["./substitution", "./real"]);
	}

	#[test]
	fn should_find_dynamic_imports_with_options_and_plain_templates() {
		let code = "await import(`./plain`);\nawait import(`./${name}`);\nawait import(\"./json.json\", { with: { type: \"json\" } });\nawait import(name);\n";

		assert_eq!(found(code, false), ["./plain", "./json.json"]);
	}

	#[test]
	fn should_keep_scanning_after_an_unclosed_quote_in_jsx_text() {
		let code = "const page = <p>Don't panic</p>;\nconst later = import(\"./later\");\n";

		assert_eq!(found(code, false), ["./later"]);
	}

	#[test]
	fn should_not_read_a_closing_jsx_tag_as_a_regular_expression() {
		let code = "const a = <p>one</p><Lazy load={() => import(\"./lazy\")} />;\n";

		assert_eq!(found(code, false), ["./lazy"]);
	}

	#[test]
	fn should_skip_a_hashbang_and_decorators() {
		let code = "#!/usr/bin/env node\n@Component({ selector: \"x\" })\nclass A { #secret = 1; }\nimport \"./after\";\n";

		assert_eq!(found(code, false), ["./after"]);
	}

	#[test]
	fn should_tell_division_from_regular_expressions() {
		let code =
			"const half = total / 2; const re = /\"/; import(\"./after\");\nconst ratio = (a) / b / c; import(\"./next\");\n";

		assert_eq!(found(code, false), ["./after", "./next"]);
	}
}
