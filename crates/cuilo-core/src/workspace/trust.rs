//! Which projects the user allowed cuilo to run their own tools in, kept beside the user config.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ConfigError, ToolTrust, UserConfig};

const FILE: &str = "trusted-projects.toml";

/// The projects the user trusted or refused, by canonical root: `<config dir>/trusted-projects.toml`.
#[derive(Clone, Debug, Default)]
pub struct TrustedProjects {
	path: Option<PathBuf>,
	file: TrustFile,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustFile {
	#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
	trusted: BTreeSet<PathBuf>,
	#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
	refused: BTreeSet<PathBuf>,
}

impl TrustedProjects {
	/// Loads the list beside the user config, or an empty one.
	pub fn load() -> Result<Self, ConfigError> {
		match UserConfig::path().and_then(|config| Some(config.parent()?.join(FILE))) {
			Some(path) => Self::load_from(&path),
			None => Ok(Self::default()),
		}
	}

	/// Loads the list at `path`, or an empty one when it does not exist.
	pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
		let file = match std::fs::read_to_string(path) {
			Ok(text) => toml::from_str(&text).map_err(|error| ConfigError::Parse {
				file: path.display().to_string(),
				message: error.message().to_owned(),
			})?,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => TrustFile::default(),
			Err(source) => {
				return Err(ConfigError::Io {
					path: path.to_path_buf(),
					source,
				});
			}
		};
		Ok(Self {
			path: Some(path.to_path_buf()),
			file,
		})
	}

	/// Whether the user trusted (`Some(true)`) or refused (`Some(false)`) the project at `root`.
	#[must_use]
	pub fn decision(&self, root: &Path) -> Option<bool> {
		let root = canonical(root);
		if self.file.trusted.contains(&root) {
			Some(true)
		} else if self.file.refused.contains(&root) {
			Some(false)
		} else {
			None
		}
	}

	/// Remembers whether the project at `root` is trusted, and saves the list.
	pub fn decide(&mut self, root: &Path, trusted: bool) -> Result<(), ConfigError> {
		let root = canonical(root);
		self.file.trusted.remove(&root);
		self.file.refused.remove(&root);
		if trusted {
			self.file.trusted.insert(root);
		} else {
			self.file.refused.insert(root);
		}
		self.save()
	}

	/// Whether cuilo may run the project's tools at `root` without asking, following `trust`.
	#[must_use]
	pub fn allows(&self, trust: ToolTrust, root: &Path) -> Option<bool> {
		match trust {
			ToolTrust::Always => Some(true),
			ToolTrust::Never => Some(false),
			ToolTrust::Ask => self.decision(root),
		}
	}

	fn save(&self) -> Result<(), ConfigError> {
		let Some(path) = &self.path else {
			return Ok(());
		};
		let io = |source| ConfigError::Io {
			path: path.clone(),
			source,
		};
		if let Some(dir) = path.parent() {
			std::fs::create_dir_all(dir).map_err(io)?;
		}
		let text = toml::to_string(&self.file).map_err(|error| ConfigError::Parse {
			file: path.display().to_string(),
			message: error.to_string(),
		})?;
		let temp = path.with_extension("toml.tmp");
		std::fs::write(&temp, text).map_err(io)?;
		std::fs::rename(&temp, path).map_err(io)
	}
}

fn canonical(root: &Path) -> PathBuf {
	root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn decide_should_remember_each_project_across_loads() {
		let config = tempfile::tempdir().unwrap();
		let project = tempfile::tempdir().unwrap();
		let other = tempfile::tempdir().unwrap();
		let path = config.path().join(FILE);

		let mut projects = TrustedProjects::load_from(&path).unwrap();
		projects.decide(project.path(), true).unwrap();
		projects.decide(other.path(), false).unwrap();
		let loaded = TrustedProjects::load_from(&path).unwrap();

		assert_eq!(loaded.decision(project.path()), Some(true));
		assert_eq!(loaded.decision(other.path()), Some(false));
		assert_eq!(loaded.decision(config.path()), None);
	}

	#[test]
	fn allows_should_follow_the_user_setting_first() {
		let project = tempfile::tempdir().unwrap();
		let projects = TrustedProjects::default();

		assert_eq!(projects.allows(ToolTrust::Always, project.path()), Some(true));
		assert_eq!(projects.allows(ToolTrust::Never, project.path()), Some(false));
		assert_eq!(projects.allows(ToolTrust::Ask, project.path()), None);
	}
}
