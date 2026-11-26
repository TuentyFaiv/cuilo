//! [`EngineBuilder`]: where an engine gets its files, user config, patchers and event sink.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{Engine, EngineError, Formatting};
use crate::event::{EventSink, NoopSink};
use crate::format::{Format, ProjectTools};
use crate::migrate::{AstGrep, AstGrepCommand};
use crate::patch::{CommandPatcher, PatchKind, Patcher, Patchers};
use crate::workspace::{Fs, ProjectConfig, UserConfig, Workspace};

/// Configures and opens an [`Engine`].
#[must_use]
pub struct EngineBuilder {
	user: Option<UserConfig>,
	sink: Arc<dyn EventSink>,
	patchers: Vec<(PatchKind, Box<dyn Patcher>)>,
	ast_grep: Option<Arc<dyn AstGrep>>,
	formatter: Option<Arc<dyn Format>>,
	project_tools: bool,
	verify_dir: Option<PathBuf>,
}

impl Default for EngineBuilder {
	fn default() -> Self {
		Self {
			user: None,
			sink: Arc::new(NoopSink),
			patchers: Vec::new(),
			ast_grep: None,
			formatter: None,
			project_tools: false,
			verify_dir: None,
		}
	}
}

impl fmt::Debug for EngineBuilder {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("EngineBuilder")
			.field("user", &self.user)
			.field(
				"patchers",
				&self.patchers.iter().map(|(kind, _)| kind).collect::<Vec<_>>(),
			)
			.finish_non_exhaustive()
	}
}

impl EngineBuilder {
	/// Uses `user` instead of loading the user's config file.
	pub fn user_config(mut self, user: UserConfig) -> Self {
		self.user = Some(user);
		self
	}

	/// Sends engine events to `sink`.
	pub fn sink(mut self, sink: impl EventSink + 'static) -> Self {
		self.sink = Arc::new(sink);
		self
	}

	/// Uses `patcher` for `kind`, over both the built-in and the user's configured one.
	pub fn patcher(mut self, kind: PatchKind, patcher: impl Patcher + 'static) -> Self {
		self.patchers.push((kind, Box::new(patcher)));
		self
	}

	/// Uses `ast_grep` for ast-grep migrations instead of the configured program.
	pub fn ast_grep(mut self, ast_grep: impl AstGrep + 'static) -> Self {
		self.ast_grep = Some(Arc::new(ast_grep));
		self
	}

	/// Runs the formatters and linters the project uses on generated files, and its checks with
	/// [`verify`](Engine::verify); only for a project the user trusts. Without it, none of them runs:
	/// some come from the project, like `node_modules/.bin/biome`, and project files pick even a
	/// system tool, like `rust-toolchain.toml` picks the `rustfmt` that rustup runs.
	pub fn project_tools(mut self, trusted: bool) -> Self {
		self.project_tools = trusted;
		self
	}

	/// Keeps the throwaway copies [`verify`](Engine::verify) makes in `dir` instead of the `verify`
	/// folder of the user's cache; the same checks as for that folder apply.
	pub fn verify_dir(mut self, dir: impl Into<PathBuf>) -> Self {
		self.verify_dir = Some(dir.into());
		self
	}

	/// Formats generated files with `formatter` instead of the project's tools.
	pub fn formatter(mut self, formatter: impl Format + 'static) -> Self {
		self.formatter = Some(Arc::new(formatter));
		self
	}

	/// Opens the project folder at `root`.
	pub fn open(self, root: &Path) -> Result<Engine, EngineError> {
		let workspace = Workspace::open(root)?;
		let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
		let formatting = Formatting {
			trusted: self.project_tools || self.formatter.is_some(),
			formatter: Some(
				self
					.formatter
					.clone()
					.unwrap_or_else(|| Arc::new(ProjectTools::new(&root))),
			),
		};
		self.build(workspace, formatting, Some(root))
	}

	/// Opens a project on any [`Fs`], such as a [`MemoryFs`](crate::workspace::MemoryFs) preview; there
	/// is no folder to run the project's tools in, so only a [`formatter`](Self::formatter) formats.
	pub fn open_fs(self, fs: Arc<dyn Fs>) -> Result<Engine, EngineError> {
		let formatting = Formatting {
			trusted: true,
			formatter: self.formatter.clone(),
		};
		self.build(Workspace::new(fs), formatting, None)
	}

	fn build(
		self,
		workspace: Workspace,
		formatting: Formatting,
		root: Option<std::path::PathBuf>,
	) -> Result<Engine, EngineError> {
		let user = match self.user {
			Some(user) => user,
			None => UserConfig::load()?,
		};
		let mut patchers = Patchers::builtin();
		for (kind, external) in &user.patchers {
			let patcher = CommandPatcher::new(&kind.to_string(), &external.command)
				.map_err(|source| EngineError::Patcher { kind: *kind, source })?;
			patchers.set(*kind, Box::new(patcher));
		}
		for (kind, patcher) in self.patchers {
			patchers.set(kind, patcher);
		}
		let config = ProjectConfig::load(&workspace)?;
		let ast_grep = self.ast_grep.unwrap_or_else(|| {
			Arc::new(
				user
					.ast_grep
					.as_ref()
					.map_or_else(AstGrepCommand::default, |tool| AstGrepCommand::new(&tool.command)),
			)
		});
		Ok(Engine {
			workspace,
			config,
			patchers,
			ast_grep,
			formatting,
			checks_trusted: self.project_tools,
			verify_dir: self.verify_dir,
			root,
			sink: self.sink,
		})
	}
}
