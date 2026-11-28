//! Formatting generated files on stdin with the project's own formatters and fixing linters, never
//! installing them.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::model::RelPath;
use crate::process;

const TIMEOUT: Duration = Duration::from_secs(60);

/// The order tools run in: fixing linters first, so a formatter has the last word.
pub const ORDER: [&str; 9] = [
	"eslint", "ruff", "biome", "prettier", "oxfmt", "dprint", "black", "rustfmt", "gofmt",
];

const SCRIPTS: &[&str] = &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];
const BIOME: &[&str] = &[
	"json", "jsonc", "css", "graphql", "gql", "astro", "vue", "svelte", "html",
];
const PRETTIER: &[&str] = &[
	"json", "jsonc", "json5", "css", "scss", "less", "html", "vue", "md", "mdx", "yaml", "yml", "graphql", "gql",
];
const OXFMT: &[&str] = &[
	"json", "jsonc", "json5", "css", "scss", "less", "md", "vue", "yaml", "yml",
];
const ESLINT: &[&str] = &["vue", "svelte", "astro"];
const DPRINT: &[&str] = &[
	"json", "jsonc", "md", "toml", "css", "scss", "less", "html", "vue", "svelte", "astro", "yaml", "yml",
];

/// Formats one file with one tool; a trait so tests and embedders can supply their own.
pub trait Format: Send + Sync + fmt::Debug {
	/// `text` as `tool` would write `path`, or `None` when it changes nothing or skips such files.
	fn format(&self, tool: &str, path: &RelPath, text: &str) -> Result<Option<String>, FormatError>;
}

/// Why a file could not be formatted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FormatError {
	/// The project uses the tool, but it is not installed.
	#[error("not installed")]
	Missing,
	/// cuilo does not know this tool.
	#[error("not a tool cuilo can run")]
	Unknown,
	/// The tool failed, e.g. on a syntax it cannot parse.
	#[error("{0}")]
	Failed(String),
}

/// A tool's name as people write it, e.g. `Biome` for `biome`.
#[must_use]
pub fn label(tool: &str) -> &str {
	match tool {
		"biome" => "Biome",
		"prettier" => "Prettier",
		"eslint" => "ESLint",
		"oxfmt" => "oxfmt",
		"dprint" => "dprint",
		"ruff" => "Ruff",
		"black" => "Black",
		"rustfmt" => "rustfmt",
		"gofmt" => "gofmt",
		other => other,
	}
}

/// Runs the project's own tools from its root; JavaScript tools only from `node_modules/.bin`.
#[derive(Clone, Debug)]
pub struct ProjectTools {
	root: PathBuf,
	prettier_plugins: Vec<&'static str>,
	rust_edition: String,
}

/// Where a tool's program is.
#[derive(Clone, Copy)]
enum Program {
	/// `node_modules/.bin/<name>`.
	Node(&'static str),
	/// `.venv`, then `PATH`.
	Python(&'static str),
	/// `PATH`.
	System(&'static str),
	/// `node_modules/.bin`, then `PATH`.
	NodeOrSystem(&'static str),
}

/// How a tool reports the formatted file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reply {
	/// The file on stdout.
	Stdout,
	/// `ESLint`'s JSON report, whose `output` is the fixed file.
	EslintJson,
}

/// One run of a tool over the file.
struct Step {
	args: Vec<String>,
	reply: Reply,
	/// Exit codes that still mean the output is the file, e.g. 1 for "fixed, but problems remain".
	ok: &'static [i32],
}

#[derive(Deserialize)]
struct EslintResult {
	#[serde(default)]
	output: Option<String>,
	#[serde(default)]
	messages: Vec<EslintMessage>,
}

#[derive(Deserialize)]
struct EslintMessage {
	#[serde(default)]
	fatal: bool,
	#[serde(default)]
	message: String,
}

impl ProjectTools {
	/// Tools for the project at `root`, with what they need from its manifests.
	#[must_use]
	pub fn new(root: &Path) -> Self {
		let read = |name: &str| std::fs::read_to_string(root.join(name)).unwrap_or_default();
		let package = read("package.json");
		let prettier_plugins = ["astro", "svelte"]
			.into_iter()
			.filter(|language| package.contains(&format!("\"prettier-plugin-{language}\"")))
			.collect();
		Self {
			root: root.to_path_buf(),
			prettier_plugins,
			rust_edition: rust_edition(&read("Cargo.toml")),
		}
	}

	fn handles(&self, tool: &str, extension: &str) -> bool {
		let script = SCRIPTS.contains(&extension);
		match tool {
			"biome" => script || BIOME.contains(&extension),
			"prettier" => script || PRETTIER.contains(&extension) || self.prettier_plugins.contains(&extension),
			"eslint" => script || ESLINT.contains(&extension),
			"oxfmt" => script || OXFMT.contains(&extension),
			"dprint" => script || DPRINT.contains(&extension),
			"ruff" | "black" => matches!(extension, "py" | "pyi"),
			"rustfmt" => extension == "rs",
			"gofmt" => extension == "go",
			_ => false,
		}
	}

	fn program(&self, program: Program) -> Option<PathBuf> {
		let local = |dir: &str, name: &str| {
			let file = if cfg!(windows) && dir.starts_with("node_modules") {
				format!("{name}.cmd")
			} else if cfg!(windows) {
				format!("{name}.exe")
			} else {
				name.to_owned()
			};
			let path = self.root.join(dir).join(file);
			path.is_file().then_some(path)
		};
		let venv = if cfg!(windows) { ".venv/Scripts" } else { ".venv/bin" };
		match program {
			Program::Node(name) => local("node_modules/.bin", name),
			Program::Python(name) => local(venv, name).or_else(|| Some(PathBuf::from(name))),
			Program::System(name) => Some(PathBuf::from(name)),
			Program::NodeOrSystem(name) => local("node_modules/.bin", name).or_else(|| Some(PathBuf::from(name))),
		}
	}

	fn steps(&self, tool: &str, path: &str) -> Option<(Program, Vec<Step>)> {
		let args = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
		let stdout = |args: Vec<String>| Step {
			args,
			reply: Reply::Stdout,
			ok: &[0],
		};
		Some(match tool {
			"biome" => (
				Program::Node("biome"),
				vec![stdout(args(&[
					"check",
					"--write",
					&format!("--stdin-file-path={path}"),
				]))],
			),
			"prettier" => (
				Program::Node("prettier"),
				vec![stdout(args(&["--stdin-filepath", path]))],
			),
			"eslint" => (
				Program::Node("eslint"),
				vec![Step {
					args: args(&["--stdin", "--stdin-filename", path, "--fix-dry-run", "--format", "json"]),
					reply: Reply::EslintJson,
					ok: &[0, 1],
				}],
			),
			"oxfmt" => (
				Program::Node("oxfmt"),
				vec![stdout(args(&[&format!("--stdin-filepath={path}")]))],
			),
			"dprint" => (
				Program::NodeOrSystem("dprint"),
				vec![stdout(args(&["fmt", "--stdin", path]))],
			),
			"ruff" => (
				Program::Python("ruff"),
				vec![
					Step {
						args: args(&["check", "--fix", "--quiet", "--stdin-filename", path, "-"]),
						reply: Reply::Stdout,
						ok: &[0, 1],
					},
					stdout(args(&["format", "--quiet", "--stdin-filename", path, "-"])),
				],
			),
			"black" => (
				Program::Python("black"),
				vec![stdout(args(&["--quiet", "--stdin-filename", path, "-"]))],
			),
			"rustfmt" => (
				Program::System("rustfmt"),
				vec![stdout(args(&["--edition", &self.rust_edition, "--emit", "stdout"]))],
			),
			"gofmt" => (Program::System("gofmt"), vec![stdout(Vec::new())]),
			_ => return None,
		})
	}

	fn run(&self, program: &Path, step: &Step, text: &str) -> Result<Option<String>, FormatError> {
		let program_text = program.to_string_lossy();
		let output = process::run_in(
			&program_text,
			&step.args,
			text.as_bytes().to_vec(),
			TIMEOUT,
			Some(&self.root),
			&[],
		)
		.map_err(|error| {
			if error.not_found() {
				FormatError::Missing
			} else {
				FormatError::Failed(error.to_string())
			}
		})?;
		let code = output.status.code();
		if !code.is_some_and(|code| step.ok.contains(&code)) {
			return Err(FormatError::Failed(first_line(&output.stderr, &output.stdout)));
		}
		let formatted = match step.reply {
			Reply::Stdout => String::from_utf8(output.stdout)
				.map_err(|_| FormatError::Failed("wrote something that is not UTF-8".to_owned()))?,
			Reply::EslintJson => {
				let results: Vec<EslintResult> = serde_json::from_slice(&output.stdout)
					.map_err(|error| FormatError::Failed(format!("unexpected output: {error}")))?;
				let Some(result) = results.into_iter().next() else {
					return Ok(None);
				};
				if let Some(fatal) = result.messages.iter().find(|message| message.fatal) {
					return Err(FormatError::Failed(fatal.message.clone()));
				}
				match result.output {
					Some(output) => output,
					None => return Ok(None),
				}
			}
		};
		Ok((!formatted.is_empty() && formatted != text).then_some(formatted))
	}
}

impl Format for ProjectTools {
	fn format(&self, tool: &str, path: &RelPath, text: &str) -> Result<Option<String>, FormatError> {
		let extension = path.file_name().rsplit_once('.').map_or("", |(_, extension)| extension);
		let (program, steps) = self.steps(tool, path.as_str()).ok_or(FormatError::Unknown)?;
		if !self.handles(tool, &extension.to_ascii_lowercase()) {
			return Ok(None);
		}
		let program = self.program(program).ok_or(FormatError::Missing)?;
		let mut current: Option<String> = None;
		for step in &steps {
			let input = current.as_deref().unwrap_or(text);
			if let Some(next) = self.run(&program, step, input)? {
				current = Some(next);
			}
		}
		Ok(current.filter(|formatted| formatted != text))
	}
}

/// The edition rustfmt parses with: the package's, else the workspace's, else 2021.
fn rust_edition(manifest: &str) -> String {
	let table: toml::Table = toml::from_str(manifest).unwrap_or_default();
	let edition = |section: &toml::Value| section.get("edition").and_then(toml::Value::as_str).map(str::to_owned);
	table
		.get("package")
		.and_then(edition)
		.or_else(|| table.get("workspace")?.get("package").and_then(edition))
		.unwrap_or_else(|| "2021".to_owned())
}

/// The first non-empty line of `stderr`, else of `stdout`, without terminal colors.
fn first_line(stderr: &[u8], stdout: &[u8]) -> String {
	let text = |bytes: &[u8]| {
		let text = String::from_utf8_lossy(bytes);
		text
			.lines()
			.map(strip_ansi)
			.map(|line| line.trim().trim_start_matches(['×', '✖', '!']).trim().to_owned())
			.find(|line| !line.is_empty())
	};
	text(stderr)
		.or_else(|| text(stdout))
		.unwrap_or_else(|| "it exited with an error".to_owned())
}

fn strip_ansi(line: &str) -> String {
	let mut output = String::with_capacity(line.len());
	let mut chars = line.chars();
	while let Some(c) = chars.next() {
		if c == '\u{1b}' {
			for next in chars.by_ref() {
				if next.is_ascii_alphabetic() {
					break;
				}
			}
		} else {
			output.push(c);
		}
	}
	output
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rust_edition_should_follow_the_package_then_the_workspace() {
		assert_eq!(rust_edition("[package]\nedition = \"2024\"\n"), "2024");
		assert_eq!(rust_edition("[workspace.package]\nedition = \"2018\"\n"), "2018");
		assert_eq!(rust_edition(""), "2021");
	}

	#[test]
	fn first_line_should_skip_blank_lines_and_colors() {
		assert_eq!(
			first_line(b"\n\x1b[31m  \xc3\x97 Code formatting aborted\x1b[0m\nmore", b""),
			"Code formatting aborted"
		);
		assert_eq!(first_line(b"", b""), "it exited with an error");
	}

	#[test]
	fn project_tools_should_only_take_files_each_tool_handles() {
		let tools = ProjectTools::new(Path::new("/nonexistent"));

		assert!(tools.handles("biome", "tsx"));
		assert!(tools.handles("prettier", "md"));
		assert!(!tools.handles("prettier", "astro"));
		assert!(!tools.handles("rustfmt", "ts"));
		assert!(tools.handles("gofmt", "go"));
	}

	#[test]
	fn project_tools_should_report_a_missing_local_program_and_an_unknown_tool() {
		let tools = ProjectTools::new(Path::new("/nonexistent"));
		let path = RelPath::new("src/a.ts").unwrap();

		assert!(matches!(tools.format("biome", &path, "a"), Err(FormatError::Missing)));
		assert!(matches!(tools.format("nope", &path, "a"), Err(FormatError::Unknown)));
	}

	/// Writes an executable through a child process, so this multi-threaded test process never holds it
	/// open for writing: on Linux, a process another test forks meanwhile would keep it busy (`ETXTBSY`).
	#[cfg(unix)]
	fn write_executable(path: &Path, contents: &str) {
		use std::io::Write as _;

		let mut child = std::process::Command::new("/bin/sh")
			.args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
			.arg(path)
			.stdin(std::process::Stdio::piped())
			.spawn()
			.unwrap();
		child.stdin.take().unwrap().write_all(contents.as_bytes()).unwrap();
		assert!(child.wait().unwrap().success(), "could not write {}", path.display());
	}

	#[cfg(unix)]
	#[test]
	fn project_tools_should_run_the_local_program_from_the_root() {
		let root = tempfile::tempdir().unwrap();
		let bin = root.path().join("node_modules/.bin");
		std::fs::create_dir_all(&bin).unwrap();
		let script = bin.join("prettier");
		write_executable(
			&script,
			"#!/bin/sh\ncat >/dev/null\nprintf 'formatted in %s' \"$(basename \"$PWD\")\"\n",
		);
		let tools = ProjectTools::new(root.path());
		let folder = root.path().file_name().unwrap().to_string_lossy().into_owned();

		let formatted = tools
			.format("prettier", &RelPath::new("src/a.ts").unwrap(), "a")
			.unwrap();

		assert_eq!(formatted, Some(format!("formatted in {folder}")));
	}

	#[cfg(unix)]
	#[test]
	fn project_tools_should_read_eslint_fixes_and_fatal_errors() {
		let root = tempfile::tempdir().unwrap();
		let bin = root.path().join("node_modules/.bin");
		std::fs::create_dir_all(&bin).unwrap();
		let script = bin.join("eslint");
		let write = |reply: &str| {
			write_executable(
				&script,
				&format!("#!/bin/sh\ncat >/dev/null\nprintf '%s' '{reply}'\nexit 1\n"),
			);
		};
		let tools = ProjectTools::new(root.path());
		let path = RelPath::new("a.js").unwrap();

		write(r#"[{"output":"const a = 1;\n","messages":[]}]"#);
		assert_eq!(
			tools.format("eslint", &path, "let a = 1;\n").unwrap().as_deref(),
			Some("const a = 1;\n")
		);

		write(r#"[{"messages":[{"fatal":true,"message":"Parsing error"}]}]"#);
		assert!(
			matches!(tools.format("eslint", &path, "let a = ;"), Err(FormatError::Failed(message)) if message == "Parsing error")
		);
	}
}
