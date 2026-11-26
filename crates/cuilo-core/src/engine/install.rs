//! The package-manager commands that add the packages a plan lists; cuilo only builds them.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use super::{Engine, EngineError};
use crate::model::{Dependency, Ecosystem, Fact, Stack};
use crate::workspace::{declared_packages, normalized_python_name};

/// One command of the project's package manager, run from the project root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstallCommand {
	/// The program, e.g. `bun`.
	pub program: String,
	/// Its arguments, e.g. `["add", "-d", "vitest@^4.0"]`.
	pub args: Vec<String>,
}

impl fmt::Display for InstallCommand {
	/// The command as it would be typed in a POSIX shell.
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.program)?;
		for arg in &self.args {
			let plain = arg
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || "@^~.=/:_+,-".contains(c));
			if plain {
				write!(f, " {arg}")?;
			} else {
				write!(f, " '{}'", arg.replace('\'', r"'\''"))?;
			}
		}
		Ok(())
	}
}

/// How to add the packages a plan lists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Installs {
	/// Commands to run, in order.
	pub commands: Vec<InstallCommand>,
	/// Packages the project already declares; their versions are left as they are.
	pub present: Vec<String>,
	/// Packages to add by hand, each with how, e.g. when the project has no package manager cuilo can run.
	pub manual: Vec<String>,
}

impl Installs {
	/// Whether there is nothing to run or to add by hand.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.commands.is_empty() && self.manual.is_empty()
	}
}

impl Engine {
	/// The commands that add `dependencies` with the project's own package managers, skipping the
	/// packages the project already declares; nothing runs.
	pub fn installs(&self, dependencies: &[Dependency]) -> Result<Installs, EngineError> {
		let stack = self.project()?.stack;
		let mut by_ecosystem: BTreeMap<Ecosystem, Vec<&Dependency>> = BTreeMap::new();
		for dependency in dependencies {
			by_ecosystem.entry(dependency.ecosystem).or_default().push(dependency);
		}
		let mut installs = Installs::default();
		for (ecosystem, dependencies) in by_ecosystem {
			let declared = declared_packages(&self.workspace, ecosystem)?;
			let (present, missing): (Vec<&Dependency>, Vec<&Dependency>) = dependencies.into_iter().partition(|dependency| {
				let name = match ecosystem {
					Ecosystem::Pypi => normalized_python_name(&dependency.name),
					_ => dependency.name.clone(),
				};
				declared.contains(&name)
			});
			installs
				.present
				.extend(present.iter().map(|dependency| dependency.name.clone()));
			if missing.is_empty() {
				continue;
			}
			match commands(ecosystem, &stack, &missing) {
				Some(commands) => installs.commands.extend(commands),
				None => installs.manual.extend(missing.iter().map(|dependency| {
					format!(
						"add `{}` to {}",
						spec(ecosystem, dependency),
						match ecosystem {
							Ecosystem::Pypi => "requirements.txt or pyproject.toml",
							other => other.manifest(),
						}
					)
				})),
			}
		}
		Ok(installs)
	}
}

/// `dependency` as its package manager writes it, e.g. `zod@^4.1`, `uvicorn[standard]>=0.30`.
fn spec(ecosystem: Ecosystem, dependency: &Dependency) -> String {
	let Dependency { name, version, .. } = dependency;
	match (ecosystem, version.is_empty()) {
		(Ecosystem::Rubygems, _) | (_, true) => name.clone(),
		(Ecosystem::Pypi, false) if dependency.features.is_empty() => format!("{name}{version}"),
		(Ecosystem::Pypi, false) => format!("{name}[{}]{version}", dependency.features.join(",")),
		(Ecosystem::Pub | Ecosystem::Packagist, false) => format!("{name}:{version}"),
		(Ecosystem::Npm | Ecosystem::Cargo | Ecosystem::Go, false) => format!("{name}@{version}"),
	}
}

/// The project's manager for `ecosystem`, or `None` when cuilo cannot run one.
fn commands(ecosystem: Ecosystem, stack: &Stack, missing: &[&Dependency]) -> Option<Vec<InstallCommand>> {
	let manager = |names: &[&'static str]| names.iter().copied().find(|name| stack.has(Fact::PackageManager, name));
	let command = |program: &str, args: Vec<String>| InstallCommand {
		program: program.to_owned(),
		args,
	};
	let (normal, dev): (Vec<&Dependency>, Vec<&Dependency>) = missing.iter().partition(|dependency| !dependency.dev);
	let grouped = |program: &str, add: &[&str], dev_flag: &[&str], prefix: &str| {
		[(normal.as_slice(), &[][..]), (dev.as_slice(), dev_flag)]
			.into_iter()
			.filter(|(dependencies, _)| !dependencies.is_empty())
			.map(|(dependencies, flags)| {
				let mut args: Vec<String> = add.iter().chain(flags).map(|arg| (*arg).to_owned()).collect();
				args.extend(
					dependencies
						.iter()
						.map(|dependency| format!("{prefix}{}", spec(ecosystem, dependency))),
				);
				command(program, args)
			})
			.collect::<Vec<_>>()
	};
	let each =
		|build: &dyn Fn(&Dependency) -> InstallCommand| missing.iter().map(|dependency| build(dependency)).collect();
	Some(match ecosystem {
		Ecosystem::Npm => match manager(&["bun", "pnpm", "yarn", "deno", "npm"])? {
			"bun" => grouped("bun", &["add"], &["-d"], ""),
			"pnpm" => grouped("pnpm", &["add"], &["-D"], ""),
			"yarn" => grouped("yarn", &["add"], &["-D"], ""),
			"deno" => grouped("deno", &["add"], &["--dev"], "npm:"),
			_ => grouped("npm", &["install"], &["--save-dev"], ""),
		},
		Ecosystem::Cargo => {
			manager(&["cargo"])?;
			each(&|dependency| {
				let mut args = vec!["add".to_owned(), spec(ecosystem, dependency)];
				if dependency.dev {
					args.push("--dev".to_owned());
				}
				if !dependency.features.is_empty() {
					args.extend(["--features".to_owned(), dependency.features.join(",")]);
				}
				command("cargo", args)
			})
		}
		Ecosystem::Pypi => match manager(&["uv", "poetry", "pdm", "pipenv"])? {
			"uv" => grouped("uv", &["add"], &["--dev"], ""),
			"poetry" => grouped("poetry", &["add"], &["--group", "dev"], ""),
			"pdm" => grouped("pdm", &["add"], &["--dev"], ""),
			_ => grouped("pipenv", &["install"], &["--dev"], ""),
		},
		Ecosystem::Go => {
			manager(&["go"])?;
			let mut args = vec!["get".to_owned()];
			args.extend(missing.iter().map(|dependency| spec(ecosystem, dependency)));
			vec![command("go", args)]
		}
		Ecosystem::Pub => {
			manager(&["pub"])?;
			let program = if stack.has(Fact::Framework, "flutter") {
				"flutter"
			} else {
				"dart"
			};
			let mut args = vec!["pub".to_owned(), "add".to_owned()];
			args.extend(missing.iter().map(|dependency| {
				let prefix = if dependency.dev { "dev:" } else { "" };
				format!("{prefix}{}", spec(ecosystem, dependency))
			}));
			vec![command(program, args)]
		}
		Ecosystem::Rubygems => {
			manager(&["bundler"])?;
			each(&|dependency| {
				let mut args = vec!["add".to_owned(), dependency.name.clone()];
				if !dependency.version.is_empty() {
					args.extend(["--version".to_owned(), dependency.version.clone()]);
				}
				if dependency.dev {
					args.extend(["--group".to_owned(), "development".to_owned()]);
				}
				command("bundle", args)
			})
		}
		Ecosystem::Packagist => {
			manager(&["composer"])?;
			grouped("composer", &["require"], &["--dev"], "")
		}
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn dependency(ecosystem: Ecosystem, name: &str, version: &str, dev: bool) -> Dependency {
		Dependency {
			ecosystem,
			name: name.to_owned(),
			version: version.to_owned(),
			features: Vec::new(),
			dev,
			variants: Vec::new(),
			when: None,
		}
	}

	fn stack(manager: &str) -> Stack {
		let mut stack = Stack::default();
		stack.insert(Fact::PackageManager, manager);
		stack
	}

	fn lines(commands: Option<Vec<InstallCommand>>) -> Vec<String> {
		commands.unwrap_or_default().iter().map(ToString::to_string).collect()
	}

	#[test]
	fn commands_should_split_runtime_and_dev_packages_for_npm_managers() {
		let zod = dependency(Ecosystem::Npm, "zod", "^4.1", false);
		let vitest = dependency(Ecosystem::Npm, "vitest", "^4.0", true);

		assert_eq!(
			lines(commands(Ecosystem::Npm, &stack("bun"), &[&zod, &vitest])),
			["bun add zod@^4.1", "bun add -d vitest@^4.0"]
		);
		assert_eq!(
			lines(commands(Ecosystem::Npm, &stack("deno"), &[&vitest])),
			["deno add --dev npm:vitest@^4.0"]
		);
		assert_eq!(
			lines(commands(Ecosystem::Npm, &stack("npm"), &[&vitest])),
			["npm install --save-dev vitest@^4.0"]
		);
	}

	#[test]
	fn commands_should_follow_each_ecosystem() {
		let mut tokio = dependency(Ecosystem::Cargo, "tokio", "1", true);
		tokio.features = vec!["macros".to_owned(), "rt-multi-thread".to_owned()];
		let httpx = dependency(Ecosystem::Pypi, "httpx", ">=0.28", true);
		let uuid = dependency(Ecosystem::Go, "github.com/google/uuid", "v1.6.0", false);
		let mut flutter = stack("pub");
		flutter.insert(Fact::Framework, "flutter");

		assert_eq!(
			lines(commands(Ecosystem::Cargo, &stack("cargo"), &[&tokio])),
			["cargo add tokio@1 --dev --features macros,rt-multi-thread"]
		);
		assert_eq!(
			lines(commands(Ecosystem::Pypi, &stack("uv"), &[&httpx])),
			["uv add --dev 'httpx>=0.28'"]
		);
		assert_eq!(
			lines(commands(Ecosystem::Go, &stack("go"), &[&uuid])),
			["go get github.com/google/uuid@v1.6.0"]
		);
		assert_eq!(
			lines(commands(
				Ecosystem::Pub,
				&flutter,
				&[&dependency(Ecosystem::Pub, "drift", "^2.20", false)]
			)),
			["flutter pub add drift:^2.20"]
		);
	}

	#[test]
	fn commands_should_be_none_without_a_manager_cuilo_can_run() {
		let httpx = dependency(Ecosystem::Pypi, "httpx", ">=0.28", false);

		assert_eq!(commands(Ecosystem::Pypi, &stack("pip"), &[&httpx]), None);
	}
}
