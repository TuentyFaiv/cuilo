//! Formatting rendered files with the project's own tools before they are planned.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use super::Engine;
use crate::format::{Format, FormatError, ORDER, label};
use crate::model::{Fact, RelPath, Stack};
use crate::pack::RenderedFile;
use crate::plan::{Draft, Plan};
use crate::template;
use crate::workspace::Lockfile;

/// How an engine formats the files it generates.
#[derive(Clone)]
pub(crate) struct Formatting {
	/// The formatter, or `None` when there is no project folder to run tools in.
	pub formatter: Option<Arc<dyn Format>>,
	/// Whether the user trusts the project, so tools that come from it may run.
	pub trusted: bool,
}

impl fmt::Debug for Formatting {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Formatting")
			.field("formatter", &self.formatter)
			.field("trusted", &self.trusted)
			.finish()
	}
}

impl Engine {
	/// Formats `files` with the formatters in `stack`, in [`ORDER`]. With `plan`, it notes what could
	/// not be formatted and keeps each formatted file as the original later updates merge from.
	pub(super) fn format_files(&self, stack: &Stack, files: &mut [RenderedFile], mut plan: Option<&mut Plan<Draft>>) {
		let tools: Vec<&str> = ORDER
			.iter()
			.copied()
			.filter(|tool| stack.has(Fact::Formatter, tool))
			.chain(stack.values(Fact::Formatter).filter(|tool| !ORDER.contains(tool)))
			.collect();
		if tools.is_empty() || files.is_empty() {
			return;
		}
		let Some(formatter) = &self.formatting.formatter else {
			return;
		};
		// Even system tools resolve through project files, like rustup reading `rust-toolchain.toml`.
		if !self.formatting.trusted {
			if let Some(plan) = plan {
				plan.note(
					Lockfile::path(),
					format!(
						"not formatted with {}: cuilo runs a project's formatters and linters only once you trust it; run `cuilo trust` here",
						names(&tools)
					),
				);
			}
			return;
		}
		let mut skipped: BTreeSet<&str> = BTreeSet::new();
		let mut failures: BTreeMap<(&str, String), (RelPath, usize)> = BTreeMap::new();
		for file in files.iter_mut() {
			if template::is_binary(&file.contents) {
				continue;
			}
			let original = String::from_utf8_lossy(&file.contents).into_owned();
			let mut text = original.clone();
			for tool in &tools {
				if skipped.contains(tool) {
					continue;
				}
				match formatter.format(tool, &file.path, &text) {
					Ok(Some(next)) => text = next,
					Ok(None) => {}
					Err(error @ (FormatError::Missing | FormatError::Unknown)) => {
						skipped.insert(tool);
						if let Some(plan) = plan.as_deref_mut() {
							plan.note(Lockfile::path(), skipped_message(tool, &error));
						}
					}
					Err(FormatError::Failed(message)) => {
						failures.entry((tool, message)).or_insert((file.path.clone(), 0)).1 += 1;
					}
				}
			}
			if text != original {
				file.contents = text.into_bytes();
				if let Some(plan) = plan.as_deref_mut() {
					plan.keep(file.contents.clone());
				}
			}
		}
		if let Some(plan) = plan {
			for ((tool, message), (path, count)) in failures {
				let others = match count {
					1 => String::new(),
					2 => " (and 1 more file)".to_owned(),
					count => format!(" (and {} more files)", count - 1),
				};
				plan.note(
					path,
					format!("{} could not format this file{others}: {message}", label(tool)),
				);
			}
		}
	}
}

fn skipped_message(tool: &str, error: &FormatError) -> String {
	match error {
		FormatError::Missing => format!(
			"{} is set up in this project but not installed, so generated files were not formatted with it; install the project's dependencies",
			label(tool)
		),
		_ => format!(
			"cuilo cannot run the formatter `{tool}`; supported: {}",
			ORDER.join(", ")
		),
	}
}

fn names(tools: &[&str]) -> String {
	tools.iter().map(|tool| label(tool)).collect::<Vec<_>>().join(", ")
}
