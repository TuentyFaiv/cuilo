//! `--latest` for Gradle builds, which have no command that updates their dependencies: the version catalog and the
//! wrapper move to their newest releases.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::process::{Command, Stdio};

use serde_json::Value;

use super::Project;

/// The version catalog, which pins every plugin and library a fixture builds with.
const CATALOG: &str = "gradle/libs.versions.toml";
const WRAPPER: &str = "gradle/wrapper/gradle-wrapper.properties";
/// Repositories that publish the catalog's libraries and plugin markers, searched in order.
const REPOSITORIES: [&str; 3] = [
	"https://dl.google.com/android/maven2",
	"https://repo.maven.apache.org/maven2",
	"https://plugins.gradle.org/m2",
];
/// The newest Gradle release, with the SHA-256 of its `-bin.zip`.
const GRADLE_CURRENT: &str = "https://services.gradle.org/versions/current";
/// Words that mark a pre-release, from the earliest kind to the latest.
const PRERELEASES: [&str; 10] = [
	"snapshot",
	"dev",
	"eap",
	"alpha",
	"a",
	"beta",
	"b",
	"m",
	"milestone",
	"rc",
];

/// Moves each version of the catalog's `[versions]` that `hold` does not list to the newest release of the
/// library or plugin using it, and the wrapper to the newest Gradle unless `hold` lists `gradle`. A stable version
/// moves to the newest stable release, a pre-release to the newest release of any kind.
pub(crate) fn update(project: &mut Project, hold: &[&str]) -> Result<(), String> {
	let path = project.dir.join(CATALOG);
	let text = fs::read_to_string(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
	let catalog: toml::Table = toml::from_str(&text).map_err(|error| format!("{CATALOG} is not TOML: {error}"))?;
	let modules = modules(&catalog);
	let mut newer = BTreeMap::new();
	for (key, current) in catalog_versions(&catalog) {
		if hold.contains(&key.as_str()) {
			continue;
		}
		let Some(module) = modules.get(&key) else {
			continue;
		};
		if let Some(version) = newest(&published(module)?, &current) {
			newer.insert(key, version);
		}
	}
	if !newer.is_empty() {
		let moved: Vec<String> = newer
			.iter()
			.map(|(key, version)| format!("{key} to {version}"))
			.collect();
		project.edit(CATALOG, &format!("move {}", moved.join(", ")), |text| {
			Ok(with_versions(&text, &newer))
		})?;
	}
	if !hold.contains(&"gradle") {
		update_wrapper(project)?;
	}
	Ok(())
}

/// Each version key of `catalog` with the module whose releases it follows, `group:name`, preferring libraries
/// over plugins, whose marker is `id:id.gradle.plugin`.
fn modules(catalog: &toml::Table) -> BTreeMap<String, String> {
	let mut modules = BTreeMap::new();
	let entries = |table: &str| {
		catalog
			.get(table)
			.and_then(toml::Value::as_table)
			.into_iter()
			.flat_map(|entries| entries.values())
	};
	let reference = |entry: &toml::Value| {
		entry
			.get("version")
			.and_then(|version| version.get("ref"))
			.and_then(toml::Value::as_str)
			.map(str::to_owned)
	};
	for library in entries("libraries") {
		let text = |key: &str| library.get(key).and_then(toml::Value::as_str);
		let module = text("module").map(str::to_owned).or_else(|| {
			text("group")
				.zip(text("name"))
				.map(|(group, name)| format!("{group}:{name}"))
		});
		if let Some((key, module)) = reference(library).zip(module) {
			modules.entry(key).or_insert(module);
		}
	}
	for plugin in entries("plugins") {
		let id = plugin.get("id").and_then(toml::Value::as_str);
		if let Some((key, id)) = reference(plugin).zip(id) {
			modules.entry(key).or_insert(format!("{id}:{id}.gradle.plugin"));
		}
	}
	modules
}

fn catalog_versions(catalog: &toml::Table) -> Vec<(String, String)> {
	catalog
		.get("versions")
		.and_then(toml::Value::as_table)
		.into_iter()
		.flatten()
		.filter_map(|(key, version)| version.as_str().map(|version| (key.clone(), version.to_owned())))
		.collect()
}

/// Every version of `module`, `group:name`, the first repository that publishes it lists.
fn published(module: &str) -> Result<Vec<String>, String> {
	let (group, name) = module
		.split_once(':')
		.ok_or_else(|| format!("`{module}` is not `group:name`"))?;
	for repository in REPOSITORIES {
		let url = format!("{repository}/{}/{name}/maven-metadata.xml", group.replace('.', "/"));
		if let Some(metadata) = fetch(&url)? {
			let versions = metadata_versions(&metadata);
			if !versions.is_empty() {
				return Ok(versions);
			}
		}
	}
	Err(format!("no repository publishes `{module}`"))
}

/// The body at `url`, or `None` when the server has nothing there.
fn fetch(url: &str) -> Result<Option<String>, String> {
	let output = Command::new("curl")
		.args([
			"--silent",
			"--show-error",
			"--location",
			"--retry",
			"3",
			"--write-out",
			"\n%{http_code}",
			url,
		])
		.stdin(Stdio::null())
		.output()
		.map_err(|error| format!("could not run curl: {error}"))?;
	let body = String::from_utf8_lossy(&output.stdout);
	let (body, status) = body.rsplit_once('\n').unwrap_or((body.as_ref(), ""));
	match status {
		"200" => Ok(Some(body.to_owned())),
		"403" | "404" => Ok(None),
		_ => Err(format!(
			"could not fetch {url} (HTTP {status}): {}",
			String::from_utf8_lossy(&output.stderr).trim()
		)),
	}
}

fn metadata_versions(metadata: &str) -> Vec<String> {
	metadata
		.split("<version>")
		.skip(1)
		.filter_map(|rest| rest.split_once("</version>"))
		.map(|(version, _)| version.trim().to_owned())
		.collect()
}

/// The newest of `published` past `current`: stable only, unless `current` is a pre-release.
fn newest(published: &[String], current: &str) -> Option<String> {
	let any = !is_stable(current);
	published
		.iter()
		.filter(|version| any || is_stable(version))
		.max_by(|a, b| compare(a, b))
		.filter(|version| compare(version, current) == Ordering::Greater)
		.cloned()
}

fn is_stable(version: &str) -> bool {
	!tokens(version)
		.iter()
		.any(|token| matches!(token, Token::Word(word) if PRERELEASES.contains(&word.as_str())))
}

/// One run of digits or letters of a version.
#[derive(Debug, PartialEq, Eq)]
enum Token {
	Number(u64),
	Word(String),
}

fn tokens(version: &str) -> Vec<Token> {
	let mut tokens = Vec::new();
	let mut current = String::new();
	let mut push = |current: &mut String| {
		if !current.is_empty() {
			tokens.push(match current.parse() {
				Ok(number) => Token::Number(number),
				Err(_) => Token::Word(current.to_lowercase()),
			});
			current.clear();
		}
	};
	for c in version.chars() {
		let boundary = current
			.chars()
			.last()
			.is_some_and(|last| last.is_ascii_digit() != c.is_ascii_digit());
		if !c.is_ascii_alphanumeric() || boundary {
			push(&mut current);
		}
		if c.is_ascii_alphanumeric() {
			current.push(c);
		}
	}
	push(&mut current);
	tokens
}

/// Orders versions as Maven does for the common cases: numbers numerically, a pre-release before its release.
fn compare(a: &str, b: &str) -> Ordering {
	let (a, b) = (tokens(a), tokens(b));
	for pair in a.iter().zip(&b) {
		let order = match pair {
			(Token::Number(a), Token::Number(b)) => a.cmp(b),
			(Token::Number(_), Token::Word(_)) => Ordering::Greater,
			(Token::Word(_), Token::Number(_)) => Ordering::Less,
			(Token::Word(a), Token::Word(b)) => rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)),
		};
		if order != Ordering::Equal {
			return order;
		}
	}
	let tail = |longer: &[Token]| match longer.get(a.len().min(b.len())) {
		Some(Token::Word(word)) if PRERELEASES.contains(&word.as_str()) => Ordering::Less,
		Some(_) => Ordering::Greater,
		None => Ordering::Equal,
	};
	match a.len().cmp(&b.len()) {
		Ordering::Greater => tail(&a),
		Ordering::Less => tail(&b).reverse(),
		Ordering::Equal => Ordering::Equal,
	}
}

/// Where a word puts a version among pre-releases; other words come after them.
fn rank(word: &str) -> usize {
	PRERELEASES
		.iter()
		.position(|prerelease| *prerelease == word)
		.unwrap_or(PRERELEASES.len())
}

/// `catalog` with the given keys of its `[versions]` table set to new versions.
fn with_versions(catalog: &str, versions: &BTreeMap<String, String>) -> String {
	let mut in_versions = false;
	let mut out = String::with_capacity(catalog.len());
	for line in catalog.split_inclusive('\n') {
		let trimmed = line.trim();
		if trimmed.starts_with('[') {
			in_versions = trimmed == "[versions]";
		}
		let key = trimmed.split_once('=').map(|(key, _)| key.trim().trim_matches('"'));
		match key.and_then(|key| versions.get(key).map(|version| (key, version))) {
			Some((key, version)) if in_versions => {
				let indent = &line[..line.len() - line.trim_start().len()];
				let end = if line.ends_with('\n') { "\n" } else { "" };
				let _ = write!(out, "{indent}{key} = \"{version}\"{end}");
			}
			_ => out.push_str(line),
		}
	}
	out
}

/// Points the wrapper at the newest Gradle release, with its checksum.
fn update_wrapper(project: &mut Project) -> Result<(), String> {
	let current = fetch(GRADLE_CURRENT)?.ok_or_else(|| format!("{GRADLE_CURRENT} has no release"))?;
	let current: Value =
		serde_json::from_str(&current).map_err(|error| format!("{GRADLE_CURRENT} is not JSON: {error}"))?;
	let (Some(version), Some(checksum)) = (current["version"].as_str(), current["checksum"].as_str()) else {
		return Err(format!("{GRADLE_CURRENT} names no version and checksum"));
	};
	let path = project.dir.join(WRAPPER);
	let text = fs::read_to_string(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
	let pinned = text
		.lines()
		.find_map(|line| line.strip_prefix("distributionUrl="))
		.and_then(|url| url.rsplit_once("gradle-"))
		.and_then(|(_, file)| file.strip_suffix("-bin.zip"))
		.ok_or_else(|| format!("{WRAPPER} has no `distributionUrl` of a `-bin.zip`"))?;
	if compare(version, pinned) != Ordering::Greater {
		return Ok(());
	}
	project.edit(WRAPPER, &format!("move Gradle to {version}"), |text| {
		Ok(
			text
				.lines()
				.map(|line| {
					if line.starts_with("distributionUrl=") {
						format!("distributionUrl=https\\://services.gradle.org/distributions/gradle-{version}-bin.zip")
					} else if line.starts_with("distributionSha256Sum=") {
						format!("distributionSha256Sum={checksum}")
					} else {
						line.to_owned()
					}
				})
				.collect::<Vec<_>>()
				.join("\n")
				+ "\n",
		)
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn owned(versions: &[&str]) -> Vec<String> {
		versions.iter().map(|version| (*version).to_owned()).collect()
	}

	#[test]
	fn compare_should_order_numbers_numerically_and_pre_releases_first() {
		assert_eq!(compare("1.10.0", "1.9.2"), Ordering::Greater);
		assert_eq!(compare("1.12.0-alpha03", "1.12.0"), Ordering::Less);
		assert_eq!(compare("1.12.0-rc01", "1.12.0-beta02"), Ordering::Greater);
		assert_eq!(compare("2026.11.01", "2026.09.00"), Ordering::Greater);
		assert_eq!(compare("9.5.1", "9.5"), Ordering::Greater);
	}

	#[test]
	fn newest_should_keep_stable_versions_stable() {
		let published = owned(&["2.4.20", "2.4.21", "2.5.0-RC2", "2.3.0"]);

		assert_eq!(newest(&published, "2.4.20").as_deref(), Some("2.4.21"));
		assert_eq!(newest(&published, "2.4.21"), None);
	}

	#[test]
	fn newest_should_move_a_pre_release_to_the_newest_release_of_any_kind() {
		let published = owned(&["1.9.0", "1.12.0-alpha03", "1.12.0-alpha04", "1.10.0"]);

		assert_eq!(newest(&published, "1.12.0-alpha03").as_deref(), Some("1.12.0-alpha04"));
	}

	#[test]
	fn modules_should_follow_version_references_of_libraries_and_plugins() -> Result<(), String> {
		let catalog: toml::Table = toml::from_str(
			"[versions]\nkotlin = \"2.4.21\"\nagp = \"9.5.3\"\n[libraries]\nkotlin-test = { module = \"org.jetbrains.kotlin:kotlin-test\", version.ref = \"kotlin\" }\n[plugins]\nkotlin = { id = \"org.jetbrains.kotlin.jvm\", version.ref = \"kotlin\" }\nandroid = { id = \"com.android.application\", version.ref = \"agp\" }\n",
		)
		.map_err(|error| error.to_string())?;

		let modules = modules(&catalog);

		assert_eq!(modules["kotlin"], "org.jetbrains.kotlin:kotlin-test");
		assert_eq!(
			modules["agp"],
			"com.android.application:com.android.application.gradle.plugin"
		);
		Ok(())
	}

	#[test]
	fn with_versions_should_change_only_the_versions_table() {
		let catalog = "[versions]\nagp = \"9.5.3\"\njunit = \"4.13.2\"\n\n[libraries]\nagp = { id = \"x\" }\n";
		let versions = BTreeMap::from([("agp".to_owned(), "9.6.0".to_owned())]);

		assert_eq!(
			with_versions(catalog, &versions),
			"[versions]\nagp = \"9.6.0\"\njunit = \"4.13.2\"\n\n[libraries]\nagp = { id = \"x\" }\n"
		);
	}

	#[test]
	fn metadata_versions_should_list_every_version() {
		let metadata =
			"<metadata><versioning><versions><version>1.0</version><version>1.1</version></versions></versioning></metadata>";

		assert_eq!(metadata_versions(metadata), ["1.0", "1.1"]);
	}
}
