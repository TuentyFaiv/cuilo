//! Repository automation: `cargo xtask <task>`, running the same checks as CI.

mod docs;
mod release;
mod verify;

use std::env;
use std::fmt;
use std::process::{Command, ExitCode};

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  fmt     Check Rust formatting
  lint    Run clippy with warnings denied
  doc     Build the docs with warnings denied
  test    Run the workspace tests
  deny    Check dependencies with cargo-deny (requires cargo-deny)
  js      Check JS/JSON formatting and lints with Biome (requires bun)
  ci      Run fmt, lint, doc, test, deny and js in order
  schema  Regenerate the JSON Schemas in schemas/
  docs    Regenerate the docs site's CLI and schema reference and pack data;
          with --check, fail if they differ from the code instead
  verify-packs [options]
          Add every official piece, in each variant, to its stack's fixture and
          run that ecosystem's checks and tests; `verify-packs --help` lists
          the options

Release tasks:
  version                   Print the version every channel ships
  release-check             Check that every channel agrees, and nothing still blocks a release
  download-url              Print the download host the channels fetch from
  channels <latest> <pre>   Print the channel files this version moves forward, given the
                            versions they name now (empty for none)
  npm <binaries> <out>      Write the npm packages from the release binaries
  formula <SHA256SUMS>      Print the Homebrew formula for this version";

/// Repository task made of one or more commands.
#[derive(Clone, Copy, Debug)]
enum Task {
	Fmt,
	Lint,
	Doc,
	Test,
	Deny,
	Js,
	Ci,
	Schema,
}

impl Task {
	fn parse(name: &str) -> Option<Self> {
		match name {
			"fmt" => Some(Self::Fmt),
			"lint" => Some(Self::Lint),
			"doc" => Some(Self::Doc),
			"test" => Some(Self::Test),
			"deny" => Some(Self::Deny),
			"js" => Some(Self::Js),
			"ci" => Some(Self::Ci),
			"schema" => Some(Self::Schema),
			_ => None,
		}
	}

	fn steps(self) -> Vec<Step> {
		match self {
			Self::Fmt => vec![Step::cargo(&["fmt", "--all", "--check"])],
			Self::Lint => vec![
				Step::cargo(&[
					"clippy",
					"--workspace",
					"--all-targets",
					"--all-features",
					"--locked",
					"--",
					"-D",
					"warnings",
				]),
				Step::cargo(&[
					"clippy",
					"--package",
					"cuilo-packs",
					"--all-targets",
					"--no-default-features",
					"--locked",
					"--",
					"-D",
					"warnings",
				]),
			],
			Self::Doc => vec![
				Step::cargo(&["doc", "--workspace", "--all-features", "--no-deps", "--locked"])
					.env("RUSTDOCFLAGS", "-D warnings"),
			],
			Self::Test => vec![Step::cargo(&["test", "--workspace", "--all-features", "--locked"])],
			Self::Deny => vec![Step::cargo(&["deny", "--all-features", "check"])],
			Self::Js => vec![Step::new("bunx", &["biome", "ci", "."])],
			Self::Schema => vec![
				Step::cargo(&["test", "--package", "cuilo-core", "--lib", "--locked", "schema::"])
					.env("CUILO_UPDATE_SCHEMAS", "1"),
			],
			Self::Ci => [Self::Fmt, Self::Lint, Self::Doc, Self::Test, Self::Deny, Self::Js]
				.into_iter()
				.flat_map(Self::steps)
				.collect(),
		}
	}
}

/// One external command to run.
#[derive(Debug)]
struct Step {
	program: String,
	args: Vec<String>,
	envs: Vec<(&'static str, &'static str)>,
}

impl Step {
	fn new(program: &str, args: &[&str]) -> Self {
		Self {
			program: program.to_owned(),
			args: args.iter().map(|&arg| arg.to_owned()).collect(),
			envs: Vec::new(),
		}
	}

	fn cargo(args: &[&str]) -> Self {
		let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
		Self::new(&cargo, args)
	}

	fn env(mut self, key: &'static str, value: &'static str) -> Self {
		self.envs.push((key, value));
		self
	}

	fn run(&self) -> Result<(), StepError> {
		let status = Command::new(&self.program)
			.args(&self.args)
			.envs(self.envs.iter().copied())
			.status()
			.map_err(|source| StepError::Spawn {
				step: self.to_string(),
				source,
			})?;

		if status.success() {
			Ok(())
		} else {
			Err(StepError::Failed { step: self.to_string() })
		}
	}
}

impl fmt::Display for Step {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		for (key, value) in &self.envs {
			write!(f, "{key}=\"{value}\" ")?;
		}
		write!(f, "{} {}", self.program, self.args.join(" "))
	}
}

/// Why a step did not succeed.
#[derive(Debug)]
enum StepError {
	Spawn { step: String, source: std::io::Error },
	Failed { step: String },
}

impl fmt::Display for StepError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Spawn { step, source } => write!(f, "could not start `{step}`: {source}"),
			Self::Failed { step } => write!(f, "`{step}` failed"),
		}
	}
}

/// Runs a release task, or returns `None` when `name` is not one.
fn release_task(name: &str, args: &[String]) -> Option<Result<(), String>> {
	let path = |index: usize| {
		args
			.get(index)
			.map(std::path::PathBuf::from)
			.ok_or_else(|| format!("`{name}` needs more arguments\n\n{USAGE}"))
	};
	Some(match name {
		"version" => {
			println!("{}", release::VERSION);
			Ok(())
		}
		"release-check" => release::check().and_then(|mut problems| {
			problems.extend(release::blockers()?);
			if problems.is_empty() {
				Ok(())
			} else {
				Err(problems.join("\n"))
			}
		}),
		"download-url" => release::download_url().map(|url| println!("{url}")),
		"channels" => {
			let current = |index: usize| args.get(index).map_or("", String::as_str);
			release::channels(current(0), current(1)).map(|moved| {
				for channel in moved {
					println!("{channel}");
				}
			})
		}
		"npm" => path(0).and_then(|binaries| {
			for dir in release::npm(&binaries, &path(1)?)? {
				println!("{}", dir.display());
			}
			Ok(())
		}),
		"formula" => path(0).and_then(|sums| {
			let text =
				std::fs::read_to_string(&sums).map_err(|error| format!("could not read {}: {error}", sums.display()))?;
			print!("{}", release::formula(&text)?);
			Ok(())
		}),
		_ => return None,
	})
}

fn main() -> ExitCode {
	let args: Vec<String> = env::args().skip(1).collect();
	let Some(name) = args.first().cloned() else {
		eprintln!("{USAGE}");
		return ExitCode::FAILURE;
	};

	let result = if name == "docs" {
		Some(docs::run(args.get(1).is_some_and(|flag| flag == "--check")))
	} else if name == "verify-packs" {
		Some(verify::run(&args[1..]))
	} else {
		release_task(&name, &args[1..])
	};
	if let Some(result) = result {
		return match result {
			Ok(()) => ExitCode::SUCCESS,
			Err(error) => {
				eprintln!("✗ {error}");
				ExitCode::FAILURE
			}
		};
	}

	let Some(task) = Task::parse(&name) else {
		eprintln!("unknown task `{name}`\n\n{USAGE}");
		return ExitCode::FAILURE;
	};

	for step in task.steps() {
		eprintln!("→ {step}");
		if let Err(error) = step.run() {
			eprintln!("✗ {error}");
			return ExitCode::FAILURE;
		}
	}

	ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_should_reject_unknown_task() {
		assert!(Task::parse("deploy").is_none());
	}

	#[test]
	fn ci_should_run_every_check_in_order() {
		let steps: Vec<String> = Task::Ci.steps().iter().map(|step| step.args.join(" ")).collect();

		assert_eq!(
			steps,
			[
				"fmt --all --check",
				"clippy --workspace --all-targets --all-features --locked -- -D warnings",
				"clippy --package cuilo-packs --all-targets --no-default-features --locked -- -D warnings",
				"doc --workspace --all-features --no-deps --locked",
				"test --workspace --all-features --locked",
				"deny --all-features check",
				"biome ci .",
			]
		);
	}

	#[test]
	fn doc_should_deny_rustdoc_warnings() {
		let steps = Task::Doc.steps();

		assert_eq!(steps[0].envs, [("RUSTDOCFLAGS", "-D warnings")]);
	}
}
