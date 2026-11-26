//! Questions on stderr, keeping stdout for results, and untrusted text made safe to show there.

use std::io::{self, BufRead, Write};

/// `text` with control characters other than line feeds and tabs, and text-reordering characters,
/// escaped like `\u{1b}`, so packs and projects cannot rewrite the terminal.
pub fn printable(text: &str) -> String {
	let mut shown = String::with_capacity(text.len());
	for c in text.chars() {
		let reorders = matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
		if (c.is_control() && !matches!(c, '\n' | '\t')) || reorders {
			shown.extend(c.escape_unicode());
		} else {
			shown.push(c);
		}
	}
	shown
}

/// [`printable`] for text that must stay on one line, with line feeds shown as `\n`.
pub fn printable_line(text: &str) -> String {
	printable(text).replace('\n', "\\n")
}

/// One line typed at the terminal, trimmed. `question` is shown through [`printable`].
pub fn ask(question: &str) -> anyhow::Result<String> {
	let mut stderr = io::stderr();
	write!(stderr, "{}", printable(question))?;
	stderr.flush()?;
	let mut answer = String::new();
	io::stdin().lock().read_line(&mut answer)?;
	Ok(answer.trim().to_owned())
}

/// Whether the user answered yes to `question`.
pub fn confirm(question: &str) -> anyhow::Result<bool> {
	Ok(matches!(
		ask(&format!("{question} [y/N] "))?.to_ascii_lowercase().as_str(),
		"y" | "yes"
	))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn printable_should_escape_terminal_controls_but_keep_lines() {
		assert_eq!(
			printable("a\u{1b}[2K\rb\u{7}\u{9b}c\u{202e}d\n\te"),
			"a\\u{1b}[2K\\u{d}b\\u{7}\\u{9b}c\\u{202e}d\n\te"
		);
	}

	#[test]
	fn printable_line_should_also_escape_line_feeds() {
		assert_eq!(printable_line("one\ntwo\u{1b}"), "one\\ntwo\\u{1b}");
	}
}
