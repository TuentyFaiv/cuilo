//! A JSON description of every visible command and argument, for the generated CLI reference.

use clap::{Arg, ArgAction, Command};
use serde_json::{Value, json};

/// Every visible command in `command`, with its usage and arguments.
pub(crate) fn describe(mut command: Command) -> Value {
	command.build();
	command_value(&mut command)
}

fn command_value(command: &mut Command) -> Value {
	let usage = command.render_usage().to_string();
	let usage = usage.strip_prefix("Usage: ").unwrap_or(&usage).trim().to_owned();
	let args: Vec<Value> = command
		.get_arguments()
		.filter(|arg| !arg.is_hide_set() && !built_in(arg))
		.map(arg_value)
		.collect();
	let commands: Vec<Value> = command
		.get_subcommands_mut()
		.filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
		.map(command_value)
		.collect();
	json!({
		"name": command.get_name(),
		"path": command.get_bin_name().unwrap_or(command.get_name()),
		"about": command.get_about().map(ToString::to_string),
		"usage": usage,
		"args": args,
		"commands": commands,
	})
}

/// Whether clap added `arg` to every command, like `--help` and `--version`, which the reference
/// does not list.
fn built_in(arg: &Arg) -> bool {
	matches!(
		arg.get_action(),
		ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
	)
}

fn arg_value(arg: &Arg) -> Value {
	let values: Vec<String> = arg
		.get_possible_values()
		.iter()
		.filter(|value| !value.is_hide_set())
		.map(|value| value.get_name().to_owned())
		.collect();
	json!({
		"id": arg.get_id().as_str(),
		"long": arg.get_long(),
		"short": arg.get_short().map(String::from),
		"value_names": arg.get_value_names().map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>()),
		"help": arg.get_help().map(ToString::to_string),
		"positional": arg.is_positional(),
		"required": arg.is_required_set(),
		"global": arg.is_global_set(),
		"repeatable": matches!(arg.get_action(), ArgAction::Append | ArgAction::Count),
		"takes_value": arg.get_action().takes_values(),
		"possible_values": values,
		"defaults": arg.get_default_values().iter().map(|value| value.to_string_lossy()).collect::<Vec<_>>(),
	})
}

#[cfg(test)]
mod tests {
	use clap::CommandFactory as _;

	use super::*;
	use crate::cli::Cli;

	fn names(value: &Value) -> Vec<&str> {
		value["commands"]
			.as_array()
			.into_iter()
			.flatten()
			.filter_map(|command| command["name"].as_str())
			.collect()
	}

	#[test]
	fn describe_should_list_visible_commands_and_skip_hidden_ones() {
		let cli = describe(Cli::command());

		let commands = names(&cli);
		assert!(commands.contains(&"add") && commands.contains(&"pack"), "{commands:?}");
		assert!(!commands.contains(&"man") && !commands.contains(&"rpc") && !commands.contains(&"help"));
	}

	#[test]
	fn describe_should_give_full_usage_and_nested_commands() {
		let cli = describe(Cli::command());
		let pack = cli["commands"]
			.as_array()
			.and_then(|commands| commands.iter().find(|command| command["name"] == "pack"))
			.cloned()
			.unwrap_or_default();

		assert_eq!(pack["path"], "cuilo pack");
		assert!(names(&pack).contains(&"add"));
		let add = &cli["commands"][0];
		assert_eq!(add["usage"], "cuilo add [OPTIONS] <PIECE> [NAME]");
	}

	#[test]
	fn describe_should_mark_global_flags_and_leave_out_help() {
		let cli = describe(Cli::command());
		let ids: Vec<&str> = cli["args"]
			.as_array()
			.into_iter()
			.flatten()
			.filter_map(|arg| arg["id"].as_str())
			.collect();

		assert_eq!(ids, ["root", "json", "no_input"]);
		assert!(
			cli["args"]
				.as_array()
				.into_iter()
				.flatten()
				.all(|arg| arg["global"] == true)
		);
	}

	#[test]
	fn describe_should_list_visible_flags_and_skip_hidden_ones() {
		let cli = describe(Cli::command());
		let upgrade = cli["commands"]
			.as_array()
			.and_then(|commands| commands.iter().find(|command| command["name"] == "upgrade"))
			.cloned()
			.unwrap_or_default();
		let ids: Vec<&str> = upgrade["args"]
			.as_array()
			.into_iter()
			.flatten()
			.filter_map(|arg| arg["id"].as_str())
			.collect();

		assert!(ids.contains(&"check") && ids.contains(&"version"), "{ids:?}");
		assert!(!ids.contains(&"background"), "{ids:?}");
		assert!(!ids.contains(&"help"), "{ids:?}");
	}
}
