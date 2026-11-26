//! `cuilo` command-line entry point.

mod cli;
mod commands;
mod context;
mod inputs;
mod ui;
mod upgrade;

use std::io::{self, IsTerminal, Write as _};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use cuilo_core::{EngineError, brand};
use serde_json::json;

use crate::cli::{Cli, Command};
use crate::commands::Session;
use crate::context::Context;
use crate::ui::Output;

/// Exit codes beyond 0 (done).
pub(crate) const FAILED: u8 = 1;
pub(crate) const USAGE: u8 = 2;
pub(crate) const REJECTED: u8 = 3;

/// A failure that has already been reported, ending with `code`.
#[derive(Debug, thiserror::Error)]
#[error("exit {0}")]
pub(crate) struct Exit(pub(crate) u8);

fn main() -> ExitCode {
	let cli = Cli::parse();
	match run(&cli) {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			if let Some(Exit(code)) = error.downcast_ref::<Exit>() {
				return ExitCode::from(*code);
			}
			if let Some(EngineError::Rejected(rejected)) = error.downcast_ref::<EngineError>() {
				if cli.json {
					Output::json(&json!({ "error": "plan rejected", "problems": rejected.problems }));
				} else {
					eprint!("{}", Output::new(false, false).rejected(rejected));
				}
				return ExitCode::from(REJECTED);
			}
			if cli.json {
				Output::json(&json!({ "error": format!("{error:#}") }));
			} else {
				eprintln!("error: {}", ui::printable(&format!("{error:#}")));
			}
			ExitCode::FAILURE
		}
	}
}

fn run(cli: &Cli) -> anyhow::Result<()> {
	let root = match &cli.root {
		Some(root) => root.clone(),
		None => std::env::current_dir()?,
	};
	let interactive = !cli.json && !cli.no_input && io::stdin().is_terminal() && io::stderr().is_terminal();
	let Some(command) = &cli.command else {
		Cli::command().print_help()?;
		return Ok(());
	};
	match command {
		Command::Completions { shell } => {
			let mut script = Vec::new();
			clap_complete::generate(*shell, &mut Cli::command(), brand::BINARY_NAME, &mut script);
			io::stdout().write_all(&script)?;
			Ok(())
		}
		Command::Man => {
			clap_mangen::Man::new(Cli::command()).render(&mut io::stdout())?;
			Ok(())
		}
		Command::Upgrade { check, version } => {
			let exe = std::env::current_exe()?;
			let exe = exe.canonicalize().unwrap_or(exe);
			let outcome = upgrade::run(
				&cuilo_packs::transport::HttpsFetch::default(),
				&exe,
				*check,
				version.as_ref(),
			)?;
			if cli.json {
				Output::json(&outcome);
			} else if outcome.upgraded {
				println!("Upgraded {} {} → {}.", brand::NAME, outcome.current, outcome.available);
			} else if outcome.available == outcome.current {
				println!("{} {} is the newest release.", brand::NAME, outcome.current);
			} else if *check {
				let how = if outcome.install == upgrade::Install::Script {
					format!("run `{} upgrade`", brand::BINARY_NAME)
				} else {
					outcome.install.hint()
				};
				println!(
					"{} {} is available (you have {}); {how}.",
					brand::NAME,
					outcome.available,
					outcome.current
				);
			} else {
				println!(
					"{} {} is newer than the newest release, {}.",
					brand::NAME,
					outcome.current,
					outcome.available
				);
			}
			Ok(())
		}
		Command::Mcp => {
			let user = cuilo_core::UserConfig::load()?;
			cuilo_mcp::serve(&root, user, io::stdin().lock(), io::stdout())?;
			Ok(())
		}
		Command::Rpc => {
			cuilo_rpc::serve(&root, io::stdin().lock(), io::stdout())?;
			Ok(())
		}
		command => {
			let mut context = Context::open(
				&root,
				interactive && (command.plans() || command.verifies()),
				command.verifies(),
			)?;
			let session = Session {
				json: cli.json,
				interactive,
			};
			session.run(&mut context, command)
		}
	}
}
