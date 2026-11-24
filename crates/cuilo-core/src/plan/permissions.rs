//! Write permissions, paths no pack may touch, and paths every plan flags as risky.

use crate::brand;
use crate::model::{Glob, Permissions, RelPath};
use crate::workspace::{Workspace, WorkspaceError};

const FORBIDDEN: [&str; 3] = [".git/**", concat!(".", crate::brand_name!(), "/**"), brand::LOCK_FILE];

/// The entries at the project root that [`FORBIDDEN`] protects.
const PROTECTED: [&str; 3] = [".git", brand::STATE_DIR, brand::LOCK_FILE];

const RISKY: [(&str, &str); 22] = [
	("**/package.json", "package scripts run on install"),
	("**/.npmrc", "changes how packages install"),
	("**/*.config.js", "build config runs as code"),
	("**/*.config.ts", "build config runs as code"),
	("**/*.config.mjs", "build config runs as code"),
	("**/*.config.cjs", "build config runs as code"),
	("**/build.rs", "Cargo build scripts run as code"),
	("**/Cargo.toml", "changes Rust dependencies and build"),
	("**/pyproject.toml", "changes Python dependencies and build"),
	("**/Makefile", "build recipes run as code"),
	("**/Dockerfile", "container build runs as code"),
	(".github/**", "CI workflows run with repository secrets"),
	(".gitlab-ci.yml", "CI pipelines run with repository secrets"),
	(".husky/**", "git hooks run on commit"),
	(".vscode/**", "editor tasks and settings can run code"),
	(".idea/**", "editor settings can run code"),
	(".claude/**", "AI agent configuration"),
	(".cursor/**", "AI agent configuration"),
	(".codex/**", "AI agent configuration"),
	("**/AGENTS.md", "AI agent instructions"),
	("**/CLAUDE.md", "AI agent instructions"),
	("**/.env*", "may hold secrets"),
];

/// Whether no pack may ever write `path`, however its letters are cased.
pub(crate) fn is_forbidden(path: &RelPath) -> bool {
	folded(path).is_none_or(|path| FORBIDDEN.iter().any(|pattern| matches(pattern, &path)))
}

/// Whether `path` is forbidden by name, or its first segment is another name the disk gives an entry
/// [`FORBIDDEN`] protects, such as `GIT~1` for `.git` on Windows.
pub(crate) fn is_forbidden_in(workspace: &Workspace, path: &RelPath) -> Result<bool, WorkspaceError> {
	if is_forbidden(path) {
		return Ok(true);
	}
	let top = path.ancestors().next().unwrap_or_else(|| path.clone());
	for protected in PROTECTED {
		if workspace.same_entry(&top, &RelPath::constant(protected))? {
			return Ok(true);
		}
	}
	Ok(false)
}

/// Why writing `path` deserves the user's attention, if it does, however its letters are cased.
pub(crate) fn risk(path: &RelPath) -> Option<&'static str> {
	let path = folded(path)?;
	RISKY
		.iter()
		.find(|(pattern, _)| matches(pattern, &path))
		.map(|(_, reason)| *reason)
}

/// `path` in lower case, as a disk that ignores case sees it; `None` when that is no valid path.
fn folded(path: &RelPath) -> Option<RelPath> {
	RelPath::new(&path.as_str().to_lowercase()).ok()
}

/// Whether the lower-cased `path` matches `pattern`, compared in lower case too.
fn matches(pattern: &str, path: &RelPath) -> bool {
	Glob::new(&pattern.to_lowercase()).is_ok_and(|glob| glob.matches(path))
}

impl Permissions {
	/// Whether these permissions allow writing `path`.
	#[must_use]
	pub fn allows_write(&self, path: &RelPath) -> bool {
		self.write.iter().any(|glob| glob.matches(path))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn rel(path: &str) -> RelPath {
		RelPath::new(path).unwrap()
	}

	#[test]
	fn git_internals_should_be_forbidden() {
		assert!(is_forbidden(&rel(".git/hooks/pre-commit")));
	}

	#[test]
	fn state_dir_and_lockfile_should_be_forbidden() {
		assert!(is_forbidden(&rel(".cuilo/objects/x")) && is_forbidden(&rel("cuilo.lock")));
	}

	#[test]
	fn forbidden_paths_should_stay_forbidden_in_any_case() {
		for path in [".GIT/hooks/pre-commit", ".Git/config", ".CUILO/objects/x", "Cuilo.LOCK"] {
			assert!(is_forbidden(&rel(path)), "{path}");
		}
	}

	#[test]
	fn a_kelvin_sign_should_not_hide_the_lockfile() {
		assert!(is_forbidden(&rel("cuilo.loc\u{212a}")));
	}

	#[test]
	fn protected_entries_should_be_valid_paths() {
		for protected in PROTECTED {
			assert_eq!(RelPath::new(protected).unwrap(), RelPath::constant(protected));
		}
	}

	#[cfg(unix)]
	#[test]
	fn another_name_for_a_protected_entry_should_be_forbidden() {
		let root = tempfile::tempdir().unwrap();
		std::fs::write(root.path().join("cuilo.lock"), "version = 1\n").unwrap();
		std::fs::hard_link(root.path().join("cuilo.lock"), root.path().join("alias.lock")).unwrap();
		std::fs::write(root.path().join("other.lock"), "version = 1\n").unwrap();
		let workspace = Workspace::open(root.path()).unwrap();

		assert!(is_forbidden_in(&workspace, &rel("alias.lock")).unwrap());
		assert!(!is_forbidden_in(&workspace, &rel("other.lock")).unwrap());
		assert!(!is_forbidden_in(&workspace, &rel("src/a.ts")).unwrap());
	}

	#[test]
	fn nested_package_json_should_be_risky() {
		assert!(risk(&rel("apps/web/package.json")).is_some());
	}

	#[test]
	fn risky_paths_should_stay_risky_in_any_case() {
		for path in [
			".GITHUB/workflows/ci.yml",
			".Husky/pre-commit",
			"src/MAKEFILE",
			"Apps/Web/Package.JSON",
		] {
			assert!(risk(&rel(path)).is_some(), "{path}");
		}
	}

	#[test]
	fn plain_source_file_should_not_be_risky() {
		assert_eq!(risk(&rel("src/components/Card.tsx")), None);
	}

	#[test]
	fn allows_write_should_follow_globs() {
		let permissions = Permissions {
			write: vec![Glob::new("src/**").unwrap()],
			..Permissions::default()
		};

		assert!(permissions.allows_write(&rel("src/a.ts")) && !permissions.allows_write(&rel("lib/a.ts")));
	}
}
