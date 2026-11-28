//! Project config (`cuilo.json`/`cuilo.toml`) and the user's own config.
//! Anything that runs programs or sends credentials lives only in the user config.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Workspace, WorkspaceError};
use crate::brand;
use crate::model::{Fact, ImportStyle, RelPath, Values, or_list};
use crate::patch::PatchKind;

const USER_CONFIG_FILE: &str = "config.toml";

/// Project settings in `cuilo.json` or `cuilo.toml`, committed with the project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
	/// JSON Schema URL for editor completion.
	#[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
	pub schema: Option<String>,
	/// Output folder per piece, replacing the piece's target.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub paths: BTreeMap<String, RelPath>,
	/// Stack facts that replace detection, e.g. `framework = ["svelte"]`. `import_style` takes one of
	/// `imports-extensionless` (`./foo`), `imports-js` (`./foo.js`) or `imports-ts` (`./foo.ts`) and sets how
	/// generated JavaScript and TypeScript write relative imports, e.g. `import_style = ["imports-js"]`;
	/// without it, the `tsconfig.json` that governs each piece's folder decides.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub stack: BTreeMap<Fact, Vec<String>>,
	/// Default input values per piece.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub defaults: BTreeMap<String, Values>,
	/// Pack namespaces, e.g. `"@acme": "gh:acme/packs@v1.2.0"`, so `@acme/ui` names a pack in it.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub sources: BTreeMap<String, String>,
	/// Where `registry:` items go and how their imports read.
	#[serde(default, skip_serializing_if = "RegistryConfig::is_empty")]
	pub registry: RegistryConfig,
	/// Commands `--verify` runs instead of the detected checks, e.g. `[["bun", "run", "check"]]`; trusted projects only.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub verify: Vec<Vec<String>>,
}

/// Placement of `registry:` items, which follow the `registry-item.json` format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegistryConfig {
	/// Import aliases; unset ones use their defaults.
	#[serde(default, skip_serializing_if = "RegistryAliases::is_empty")]
	pub aliases: RegistryAliases,
	/// Stylesheet that items' CSS variables go into, e.g. `src/app/globals.css`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub css: Option<RelPath>,
	/// Keep `"use client"` directives, for projects with React Server Components.
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub server_components: bool,
	/// Item URLs by namespace, e.g. `"@acme": "https://acme.dev/r/{name}.json"`, so `registry:@acme/card` works.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub namespaces: BTreeMap<String, String>,
}

/// Import aliases for registry items.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegistryAliases {
	/// Components, default `@/components`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub components: Option<String>,
	/// UI components, default `<components>/ui`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ui: Option<String>,
	/// Library code, default `@/lib`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub lib: Option<String>,
	/// The class-name helper module, default `<lib>/utils`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub utils: Option<String>,
	/// Hooks, default `@/hooks`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub hooks: Option<String>,
}

impl RegistryConfig {
	/// Whether nothing is configured.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		*self == Self::default()
	}
}

impl RegistryAliases {
	/// Whether no alias is set.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		*self == Self::default()
	}
}

/// The user's own config, outside every project: `<config dir>/cuilo/config.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UserConfig {
	/// External tools that replace the built-in patchers, by patch kind.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub patchers: BTreeMap<PatchKind, ExternalPatcher>,
	/// Credentials for pack downloads by host, e.g. `github.com`; the token comes from an environment variable.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub auth: BTreeMap<String, HostAuth>,
	/// The ast-grep program for pack migrations, e.g. `["bunx", "@ast-grep/cli"]`; default `ast-grep`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ast_grep: Option<ExternalPatcher>,
	/// Trusted signing keys, and whether packs must be signed.
	#[serde(default, skip_serializing_if = "SigningConfig::is_empty")]
	pub signing: SigningConfig,
	/// When cuilo may run a project's own formatters and linters on the files it generates.
	#[serde(default, skip_serializing_if = "ToolsConfig::is_empty")]
	pub tools: ToolsConfig,
	/// What cuilo does about new releases, which it looks for at most once a day in a terminal: `auto`, `notify` or `off`; `CUILO_UPDATES` overrides it. Default `auto`.
	#[serde(default)]
	pub updates: UpdatePolicy,
	/// Whether `cuilo add` applies pieces' wiring steps, such as mounting new routes in the app's entry
	/// file, instead of listing them as next steps; `--wire` and `--no-wire` override it.
	#[serde(default)]
	pub wire: bool,
}

/// What cuilo does about new releases; checks run in the background, never outside a terminal or in CI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePolicy {
	/// Install them in the background, signature checked, when the install script installed cuilo; otherwise print a notice.
	#[default]
	Auto,
	/// Print a notice, at most once a day, saying to run `cuilo upgrade`.
	Notify,
	/// Never look for them; `cuilo upgrade` still works.
	Off,
}

/// Project tools such as Biome or Prettier.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolsConfig {
	/// Default `ask`.
	#[serde(default)]
	pub trust: ToolTrust,
}

impl ToolsConfig {
	/// Whether nothing is configured.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		*self == Self::default()
	}
}

/// Whether cuilo may run a project's tools, which run with the user's rights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolTrust {
	/// Ask once per project in a terminal and remember the answer; elsewhere, only in trusted projects.
	#[default]
	Ask,
	/// In every project.
	Always,
	/// In no project.
	Never,
}

/// Which pack signatures to trust; official packs ship inside cuilo and need none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SigningConfig {
	/// Refuse packs without a valid signature from a trusted key.
	#[serde(default)]
	pub require: bool,
	/// Trusted minisign public keys by name, e.g. `acme = "RWQ…"`.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub keys: BTreeMap<String, String>,
	/// Ids of keys whose signatures are refused even when valid, e.g. `"F7C5EF0A9B1D2E3C"`.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub revoked: Vec<String>,
}

impl SigningConfig {
	/// Whether nothing is configured.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		*self == Self::default()
	}
}

/// How to authenticate pack downloads from one host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostAuth {
	/// Environment variable holding the token, e.g. `GITHUB_TOKEN`.
	pub env: String,
	/// Header that carries the token; default `Authorization`.
	#[serde(default = "HostAuth::default_header")]
	pub header: String,
	/// Text before the token, default `Bearer`; empty sends the token alone.
	#[serde(default = "HostAuth::default_scheme")]
	pub scheme: String,
}

impl HostAuth {
	fn default_header() -> String {
		"Authorization".to_owned()
	}

	fn default_scheme() -> String {
		"Bearer".to_owned()
	}
}

/// An external patcher command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalPatcher {
	/// Program and arguments.
	pub command: Vec<String>,
}

/// Why a config could not be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
	/// Both project config files exist.
	#[error(
		"both `{}` and `{}` exist; keep only one",
		brand::CONFIG_FILE_JSON,
		brand::CONFIG_FILE_TOML
	)]
	Both,
	/// A config is invalid.
	#[error("`{file}`: {message}")]
	Parse {
		/// Config file.
		file: String,
		/// Parser message.
		message: String,
	},
	/// Reading the project config failed.
	#[error(transparent)]
	Workspace(#[from] WorkspaceError),
	/// Reading the user config failed.
	#[error("could not read `{path}`: {source}")]
	Io {
		/// The file.
		path: PathBuf,
		/// The cause.
		source: std::io::Error,
	},
}

impl ProjectConfig {
	/// Loads the project's config, or the default when there is none; `stack` values must be plain
	/// names, since they end up in the trust question and in plans.
	pub(crate) fn load(workspace: &Workspace) -> Result<Self, ConfigError> {
		let json = workspace.read(&RelPath::constant(brand::CONFIG_FILE_JSON))?;
		let toml = workspace.read(&RelPath::constant(brand::CONFIG_FILE_TOML))?;
		let (file, config): (&str, Self) = match (json, toml) {
			(Some(_), Some(_)) => return Err(ConfigError::Both),
			(Some(bytes), None) => (
				brand::CONFIG_FILE_JSON,
				serde_json::from_slice(&bytes).map_err(|error| ConfigError::Parse {
					file: brand::CONFIG_FILE_JSON.to_owned(),
					message: error.to_string(),
				})?,
			),
			(None, Some(bytes)) => (brand::CONFIG_FILE_TOML, parse_toml(brand::CONFIG_FILE_TOML, &bytes)?),
			(None, None) => return Ok(Self::default()),
		};
		let error = |message: String| ConfigError::Parse {
			file: file.to_owned(),
			message,
		};
		if let Some(value) = config.stack.values().flatten().find(|value| !is_plain_name(value)) {
			return Err(error(format!(
				"stack value {value:?} must be a plain name such as `react`, `next.js` or `c++`"
			)));
		}
		match config.stack.get(&Fact::ImportStyle).map(Vec::as_slice) {
			Some([_, _, ..]) => Err(error("stack `import_style` takes one value".to_owned())),
			Some([value]) if ImportStyle::from_value(value).is_none() => Err(error(format!(
				"stack `import_style` value {value:?} must be {}",
				or_list(&ImportStyle::ALL.map(ImportStyle::value))
			))),
			_ => Ok(config),
		}
	}
}

/// Whether `value` is a plain name, as stack values are: at most 64 ASCII letters, digits and `-_.+#`.
pub(crate) fn is_plain_name(value: &str) -> bool {
	(1..=64).contains(&value.len())
		&& value
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '#'))
}

impl UserConfig {
	/// The user config file: `$CUILO_CONFIG_DIR`, else the OS config folder.
	#[must_use]
	pub fn path() -> Option<PathBuf> {
		let env = |name: &str| {
			std::env::var_os(name)
				.filter(|value| !value.is_empty())
				.map(PathBuf::from)
		};
		let dir = env(&format!("{}CONFIG_DIR", brand::ENV_PREFIX)).or_else(|| {
			let base = if cfg!(windows) {
				env("APPDATA")
			} else {
				env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|home| home.join(".config")))
			};
			base.map(|base| base.join(brand::NAME))
		})?;
		Some(dir.join(USER_CONFIG_FILE))
	}

	/// Loads the user config, or the default when there is none.
	pub fn load() -> Result<Self, ConfigError> {
		Self::path().map_or_else(|| Ok(Self::default()), |path| Self::load_from(&path))
	}

	/// Loads the user config from `path`, or the default when it does not exist.
	pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
		match std::fs::read(path) {
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
			Err(source) => Err(ConfigError::Io {
				path: path.to_path_buf(),
				source,
			}),
			Ok(bytes) => parse_toml(&path.display().to_string(), &bytes),
		}
	}
}

/// The user's cache folder: `$CUILO_CACHE_DIR`, else the OS cache folder.
#[must_use]
pub fn user_cache_dir() -> Option<PathBuf> {
	let env = |name: &str| {
		std::env::var_os(name)
			.filter(|value| !value.is_empty())
			.map(PathBuf::from)
	};
	env(&format!("{}CACHE_DIR", brand::ENV_PREFIX)).or_else(|| {
		let base = if cfg!(windows) {
			env("LOCALAPPDATA")
		} else if cfg!(target_os = "macos") {
			env("HOME").map(|home| home.join("Library/Caches"))
		} else {
			env("XDG_CACHE_HOME").or_else(|| env("HOME").map(|home| home.join(".cache")))
		};
		base.map(|base| base.join(brand::NAME))
	})
}

fn parse_toml<T: serde::de::DeserializeOwned>(file: &str, bytes: &[u8]) -> Result<T, ConfigError> {
	let parse = |message: String| ConfigError::Parse {
		file: file.to_owned(),
		message,
	};
	let text = std::str::from_utf8(bytes).map_err(|_| parse("not valid UTF-8".to_owned()))?;
	toml::from_str(text).map_err(|error| parse(error.message().to_owned()))
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::workspace::MemoryFs;

	fn workspace_with(files: &[(&str, &str)]) -> Workspace {
		let fs = MemoryFs::new();
		for (name, text) in files {
			fs.insert(&RelPath::new(name).unwrap(), text.as_bytes());
		}
		Workspace::new(Arc::new(fs))
	}

	#[test]
	fn load_should_default_without_config() {
		let workspace = workspace_with(&[]);

		assert_eq!(ProjectConfig::load(&workspace).unwrap(), ProjectConfig::default());
	}

	#[test]
	fn load_should_read_json_with_schema() {
		let workspace = workspace_with(&[("cuilo.json", r#"{"$schema": "x", "paths": {"component": "src/ui"}}"#)]);

		assert_eq!(
			ProjectConfig::load(&workspace).unwrap().paths["component"].as_str(),
			"src/ui"
		);
	}

	#[test]
	fn load_should_read_toml_stack_override() {
		let workspace = workspace_with(&[("cuilo.toml", "[stack]\nframework = [\"svelte\"]\n")]);

		assert_eq!(
			ProjectConfig::load(&workspace).unwrap().stack[&Fact::Framework],
			["svelte"]
		);
	}

	#[test]
	fn load_should_reject_stack_values_that_are_not_plain_names() {
		for (file, text) in [
			(
				"cuilo.json",
				r#"{"stack": {"formatter": ["prettier", "\u001b[2K\rTrust? [y/N]"]}}"#,
			),
			("cuilo.toml", "[stack]\nframework = [\"my framework\"]\n"),
		] {
			let error = ProjectConfig::load(&workspace_with(&[(file, text)]))
				.unwrap_err()
				.to_string();

			assert!(
				error.contains("must be a plain name") && !error.contains('\u{1b}'),
				"{error}"
			);
		}
	}

	#[test]
	fn load_should_accept_one_known_import_style_only() {
		let load =
			|stack: &str| ProjectConfig::load(&workspace_with(&[("cuilo.json", &format!(r#"{{"stack": {stack}}}"#))]));

		assert_eq!(
			load(r#"{"import_style": ["imports-js"]}"#).unwrap().stack[&Fact::ImportStyle],
			["imports-js"]
		);
		assert!(load(r#"{"import_style": []}"#).is_ok());
		for (stack, expected) in [
			(
				r#"{"import_style": ["js"]}"#,
				"must be `imports-extensionless`, `imports-js` or `imports-ts`",
			),
			(r#"{"import_style": ["imports-js", "imports-ts"]}"#, "takes one value"),
		] {
			let error = load(stack).unwrap_err().to_string();

			assert!(error.contains(expected), "{error}");
		}
	}

	#[test]
	fn load_should_reject_both_files() {
		let workspace = workspace_with(&[("cuilo.json", "{}"), ("cuilo.toml", "")]);

		assert!(matches!(ProjectConfig::load(&workspace), Err(ConfigError::Both)));
	}

	#[test]
	fn project_config_should_reject_patchers() {
		let workspace = workspace_with(&[("cuilo.json", r#"{"patchers": {"import": {"command": ["sh"]}}}"#)]);

		assert!(matches!(
			ProjectConfig::load(&workspace),
			Err(ConfigError::Parse { .. })
		));
	}

	#[test]
	fn user_config_should_load_patchers() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("config.toml");
		std::fs::write(&path, "[patchers.import]\ncommand = [\"my-tool\", \"--json\"]\n").unwrap();

		assert_eq!(
			UserConfig::load_from(&path).unwrap().patchers[&PatchKind::Import].command,
			["my-tool", "--json"]
		);
	}

	#[test]
	fn user_config_should_leave_wiring_off_unless_set() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("config.toml");
		std::fs::write(&path, "wire = true\n").unwrap();

		assert!(!UserConfig::default().wire);
		assert!(UserConfig::load_from(&path).unwrap().wire);
	}

	#[test]
	fn user_config_should_default_when_missing() {
		let dir = tempfile::tempdir().unwrap();

		assert_eq!(
			UserConfig::load_from(&dir.path().join("none.toml")).unwrap(),
			UserConfig::default()
		);
	}

	#[test]
	fn user_config_should_reject_unknown_patch_kind() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("config.toml");
		std::fs::write(&path, "[patchers.magic]\ncommand = [\"x\"]\n").unwrap();

		assert!(matches!(UserConfig::load_from(&path), Err(ConfigError::Parse { .. })));
	}

	#[test]
	fn user_config_should_read_the_update_policy_and_default_to_auto() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("config.toml");
		std::fs::write(&path, "updates = \"notify\"\n").unwrap();

		assert_eq!(UserConfig::load_from(&path).unwrap().updates, UpdatePolicy::Notify);
		assert_eq!(UserConfig::default().updates, UpdatePolicy::Auto);
		std::fs::write(&path, "updates = \"sometimes\"\n").unwrap();
		assert!(matches!(UserConfig::load_from(&path), Err(ConfigError::Parse { .. })));
	}
}
