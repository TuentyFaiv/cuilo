//! The commands, each a method of [`Session`]; plans all end in [`Session::finish`].

mod info;
mod pack;
mod piece;
mod update;

use std::fmt::Write as _;

use anyhow::{Context as _, bail};
use cuilo_core::engine::Installs;
use cuilo_core::plan::{Action, Plan, Validated};
use serde_json::json;

use crate::cli::{ApplyArgs, Command};
use crate::context::Context;
use crate::ui::{Output, confirm, printable_line, review};
use crate::{Exit, FAILED, REJECTED, USAGE};

const VERIFY_QUESTION: &str = "--verify runs the project's checks on a copy with these files in place, so code from the pack can run as you. Run the checks?";

/// How the current command talks to the user.
#[derive(Clone, Copy)]
pub struct Session {
	/// Print JSON instead of text.
	pub json: bool,
	/// Prompts and confirmations are possible.
	pub interactive: bool,
}

impl Session {
	/// Runs `command` in the project at `context`.
	pub fn run(self, context: &mut Context, command: &Command) -> anyhow::Result<()> {
		match command {
			Command::Add(args) => self.piece(context, args, false),
			Command::Adopt(args) => self.piece(context, args, true),
			Command::Update(args) => self.update(context, args),
			Command::Status => self.status(context),
			Command::Why { location } => self.why(context, location),
			Command::Log => self.log(context),
			Command::Undo { force } => self.undo(context, *force),
			Command::Detect => self.detect(context),
			Command::Trust { revoke } => self.trust(context, *revoke),
			Command::Pack(command) => self.pack(context, command),
			Command::Completions { .. } | Command::Man | Command::Upgrade { .. } | Command::Mcp | Command::Rpc => Ok(()),
		}
	}

	/// Shows `plan`, lets the user review it, and applies it when they agree. With `--verify`, the
	/// checks run only after the plan is shown and the user agrees, or passed `--yes`: the pack's files
	/// are in place in the copy, so its code can run.
	pub(super) fn finish(self, context: &Context, mut plan: Plan<Validated>, args: &ApplyArgs) -> anyhow::Result<()> {
		let output = Output::new(self.json, args.diff);
		if args.review {
			if !self.interactive {
				bail!("--review needs a terminal");
			}
			review(&mut plan)?;
		}
		if !self.json {
			print!("{}", output.plan(&plan));
		}
		let verification = if args.verify {
			if !(args.yes || (self.interactive && confirm(VERIFY_QUESTION)?)) {
				if self.json {
					Output::json(&json!({
						"plan": plan.report(),
						"verify": null,
						"applied": null,
						"error": "--verify runs code from the pack; pass --yes to run the checks",
					}));
				} else if self.interactive {
					println!("Nothing was run or changed.");
					return Ok(());
				} else {
					eprintln!(
						"Nothing was run or changed: --verify runs code from the pack as you; pass --yes to run the checks."
					);
				}
				return Err(Exit(USAGE).into());
			}
			if !self.json {
				eprintln!("Verifying on a copy of the project…");
			}
			Some(context.engine.verify(&plan)?)
		} else {
			None
		};
		let failed = verification.as_ref().is_some_and(|verification| !verification.passed());
		if self.json && (args.dry_run || failed) {
			Output::json(&json!({ "plan": plan.report(), "verify": verification, "applied": null }));
			return if failed { Err(Exit(REJECTED).into()) } else { Ok(()) };
		}
		if !self.json
			&& let Some(verification) = &verification
		{
			print!("{}", output.verification(verification));
		}
		if failed {
			eprintln!("Nothing was changed: a check fails with this plan.");
			return Err(Exit(REJECTED).into());
		}
		if args.dry_run {
			return Ok(());
		}
		let nothing = plan.changes().iter().all(|change| change.action == Action::Unchanged) && plan.instances().is_empty();
		if nothing {
			if self.json {
				Output::json(&json!({ "plan": plan.report(), "applied": null }));
			}
			return Ok(());
		}
		let approved = args.yes || (self.interactive && confirm("Apply these changes?")?);
		if !approved {
			if self.json {
				Output::json(&json!({ "plan": plan.report(), "applied": null, "error": "pass --yes to apply" }));
			} else if self.interactive {
				println!("Nothing was changed.");
				return Ok(());
			} else {
				eprintln!("Nothing was changed; pass --yes to apply without asking, or --dry-run to only plan.");
			}
			return Err(Exit(USAGE).into());
		}
		let report = serde_json::to_value(plan.report())?;
		let installs = context.engine.installs(plan.dependencies())?;
		let transaction = context.engine.apply(plan)?;
		if !self.json {
			print!("{}", output.applied(&transaction));
		}
		let ran = self.install(context, &installs, args);
		if !self.json {
			return ran.map(|_| ());
		}
		let mut install = json!({
			"commands": installs.commands.iter().map(ToString::to_string).collect::<Vec<_>>(),
			"present": installs.present,
			"manual": installs.manual,
			"ran": ran.as_ref().is_ok_and(|ran| *ran),
		});
		if let Err(error) = &ran {
			install["error"] = json!(format!("{error:#}"));
		}
		Output::json(&json!({ "plan": report, "verify": verification, "applied": transaction, "install": install }));
		ran.map(|_| ()).map_err(|_| Exit(FAILED).into())
	}

	/// Runs the install commands when the user agrees or passed `--install`; returns whether they ran.
	fn install(self, context: &Context, installs: &Installs, args: &ApplyArgs) -> anyhow::Result<bool> {
		if installs.is_empty() {
			return Ok(false);
		}
		let listing: String = installs.commands.iter().fold(String::new(), |mut listing, command| {
			let _ = write!(listing, "\n  {command}");
			listing
		});
		if !self.json {
			for manual in &installs.manual {
				println!("Add by hand: {}", printable_line(manual));
			}
		}
		if installs.commands.is_empty() || args.no_install {
			return Ok(false);
		}
		let run =
			args.install || (self.interactive && confirm(&format!("The plan lists packages to add:{listing}\nRun this?"))?);
		if !run {
			if !self.json {
				println!("Add the packages with:{listing}");
			}
			return Ok(false);
		}
		for command in &installs.commands {
			eprintln!("$ {command}");
			let status = std::process::Command::new(&command.program)
				.args(&command.args)
				.current_dir(&context.root)
				.stdin(std::process::Stdio::null())
				.stdout(std::io::stderr())
				.status()
				.with_context(|| format!("could not run `{}`", command.program))?;
			if !status.success() {
				bail!("`{command}` failed ({status}); the generated files are applied, so add the packages yourself");
			}
		}
		Ok(true)
	}
}
