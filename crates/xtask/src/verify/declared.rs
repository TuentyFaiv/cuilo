//! Inputs whose value a fixture declares in its own files, such as the Go module or the Flutter package, so the
//! pieces get the names the project builds with.

use std::fs;
use std::path::Path;

use super::stacks::{Ecosystem, Stack};

/// The value the fixture at `dir` declares for the input `name` of a piece added to `stack`, if it declares one:
/// the Go module, the Flutter package or Swift app for `app`, or the Kotlin package.
pub(crate) fn value(name: &str, stack: &Stack, dir: &Path) -> Result<Option<String>, String> {
	match (name, stack.ecosystem) {
		("module", Ecosystem::Go) => go_module(dir).map(Some),
		("app", Ecosystem::Flutter) => pubspec_name(dir).map(Some),
		("app", Ecosystem::Xcode) => xcode_project(dir).map(Some),
		("app", Ecosystem::SwiftPm) => only_folder(&dir.join("Sources")).map(Some),
		("package", Ecosystem::Gradle | Ecosystem::Android) => {
			let sources = stack
				.sources
				.first()
				.ok_or_else(|| format!("stack `{}` lists no Kotlin sources", stack.name))?;
			kotlin_package(&dir.join(sources)).map(Some)
		}
		_ => Ok(None),
	}
}

fn read(path: &Path) -> Result<String, String> {
	fs::read_to_string(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

/// The module path `go.mod` in `dir` declares.
fn go_module(dir: &Path) -> Result<String, String> {
	let path = dir.join("go.mod");
	read(&path)?
		.lines()
		.find_map(|line| line.strip_prefix("module "))
		.map(|module| module.trim().to_owned())
		.ok_or_else(|| format!("{} declares no module", path.display()))
}

/// The package `pubspec.yaml` in `dir` names.
fn pubspec_name(dir: &Path) -> Result<String, String> {
	let path = dir.join("pubspec.yaml");
	read(&path)?
		.lines()
		.find_map(|line| line.strip_prefix("name:"))
		.map(|name| name.trim().trim_matches(['"', '\'']).to_owned())
		.ok_or_else(|| format!("{} has no `name`", path.display()))
}

/// The name of the one `.xcodeproj` in `dir`, which the template also gives the app's folder.
fn xcode_project(dir: &Path) -> Result<String, String> {
	let projects: Vec<String> = entries(dir)?
		.into_iter()
		.filter_map(|(name, _)| name.strip_suffix(".xcodeproj").map(str::to_owned))
		.collect();
	match projects.as_slice() {
		[project] => Ok(project.clone()),
		_ => Err(format!(
			"{} has {} Xcode projects, not one",
			dir.display(),
			projects.len()
		)),
	}
}

/// The name of the one folder in `dir`, like the target in a package's `Sources`.
fn only_folder(dir: &Path) -> Result<String, String> {
	let folders: Vec<String> = entries(dir)?
		.into_iter()
		.filter(|(_, folder)| *folder)
		.map(|(name, _)| name)
		.collect();
	match folders.as_slice() {
		[folder] => Ok(folder.clone()),
		_ => Err(format!("{} has {} folders, not one", dir.display(), folders.len())),
	}
}

/// The package of the shallowest Kotlin file under `dir`.
fn kotlin_package(dir: &Path) -> Result<String, String> {
	let mut level = vec![dir.to_path_buf()];
	while !level.is_empty() {
		let mut next = Vec::new();
		for folder in level {
			for (name, is_folder) in entries(&folder)? {
				let path = folder.join(&name);
				if is_folder {
					next.push(path);
				} else if Path::new(&name).extension().is_some_and(|extension| extension == "kt") {
					return read(&path)?
						.lines()
						.find_map(|line| line.strip_prefix("package "))
						.map(|package| package.trim().to_owned())
						.ok_or_else(|| format!("{} declares no package", path.display()));
				}
			}
		}
		level = next;
	}
	Err(format!("{} has no Kotlin file", dir.display()))
}

/// The names in `dir`, sorted, each with whether it is a folder.
fn entries(dir: &Path) -> Result<Vec<(String, bool)>, String> {
	let list = |error: std::io::Error| format!("could not list {}: {error}", dir.display());
	let mut entries = Vec::new();
	for entry in fs::read_dir(dir).map_err(list)? {
		let entry = entry.map_err(list)?;
		let folder = entry.file_type().map_err(list)?.is_dir();
		entries.push((entry.file_name().to_string_lossy().into_owned(), folder));
	}
	entries.sort();
	Ok(entries)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn kotlin_package_should_read_the_shallowest_kotlin_file() -> Result<(), String> {
		let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
		let deep = dir.path().join("com/example/app/ui/theme");
		fs::create_dir_all(&deep).map_err(|error| error.to_string())?;
		fs::write(deep.join("Theme.kt"), "package com.example.app.ui.theme\n").map_err(|error| error.to_string())?;
		fs::write(
			dir.path().join("com/example/app/MainActivity.kt"),
			"package com.example.app\n",
		)
		.map_err(|error| error.to_string())?;

		assert_eq!(kotlin_package(dir.path())?, "com.example.app");
		Ok(())
	}

	#[test]
	fn pubspec_name_should_read_the_package_name() -> Result<(), String> {
		let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
		fs::write(
			dir.path().join("pubspec.yaml"),
			"name: cuilo_fixture\ndescription: \"x\"\n",
		)
		.map_err(|error| error.to_string())?;

		assert_eq!(pubspec_name(dir.path())?, "cuilo_fixture");
		Ok(())
	}
}
