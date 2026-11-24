//! The project a command runs in: its engine, its user config and the packs it can use.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use cuilo_core::format::{ORDER, label};
use cuilo_core::model::Fact;
use cuilo_core::workspace::TrustedProjects;
use cuilo_core::{Engine, Pack, UserConfig};
use cuilo_packs::{Library, PackSpec, Resolver};

use crate::ui::confirm;

/// An opened project.
pub struct Context {
	/// Project root.
	pub root: PathBuf,
	/// The user's config.
	pub user: UserConfig,
	/// The engine.
	pub engine: Engine,
	/// Official packs, the packs the lockfile pins, and any loaded by the command.
	pub library: Library,
}

impl Context {
	/// Opens the project at `root`, loading the official packs and, from the cache or their source,
	/// the pack versions its lockfile pins. With `ask`, it asks once whether to trust the project
	/// when the user has not decided yet and it matters: the project uses formatters that come from
	/// it, or the command `verifies` with the project's own checks.
	pub fn open(root: &Path, ask: bool, verifies: bool) -> anyhow::Result<Self> {
		let user = UserConfig::load()?;
		let mut trusted_projects = TrustedProjects::load()?;
		let trusted = trusted_projects.allows(user.tools.trust, root);
		let mut engine = open_engine(&user, root, trusted == Some(true))?;
		if trusted.is_none() && ask {
			let tools = project_tools(&engine)?;
			let question = if verifies {
				"--verify runs this project's own checks (its scripts, cargo, go, pytest…), as you. Trust this project? Your answer is remembered (`cuilo trust` changes it).".to_owned()
			} else {
				format!(
					"This project uses {}. Let cuilo run them on the files it generates? They come from the project and run as you; your answer is remembered (`cuilo trust` changes it).",
					tools.join(", ")
				)
			};
			if verifies || !tools.is_empty() {
				let yes = confirm(&question)?;
				trusted_projects.decide(root, yes)?;
				if yes {
					engine = open_engine(&user, root, true)?;
				}
			}
		}
		let mut context = Self {
			root: root.to_path_buf(),
			user,
			engine,
			library: Library::official()?,
		};
		let (pinned, _unavailable) = context.resolver()?.pinned(&context.engine.lockfile()?);
		for pack in pinned {
			context.library.add(pack);
		}
		Ok(context)
	}

	/// A resolver with the project's namespaces and the user's credentials and trusted keys.
	pub fn resolver(&self) -> anyhow::Result<Resolver> {
		Ok(Resolver::for_project(&self.root, self.engine.config(), &self.user)?)
	}

	/// Loads `source`; `pinned` uses the version the lockfile recorded for it.
	pub fn load(&mut self, source: &str, pinned: bool) -> anyhow::Result<Pack> {
		let canonical = source.parse::<PackSpec>()?.to_string();
		let digest = if pinned {
			self
				.engine
				.lockfile()?
				.packs
				.iter()
				.find(|record| record.source.as_deref() == Some(canonical.as_str()))
				.map(|record| record.digest)
		} else {
			None
		};
		let pack = self.resolver()?.resolve(source, digest)?;
		self.library.add(pack.clone());
		Ok(pack)
	}

	/// The pack and piece named by `name`, `pack/piece` or a bare piece name found in exactly one pack.
	pub fn piece<'a>(&'a self, name: &'a str, from: Option<&'a Pack>) -> anyhow::Result<(&'a Pack, &'a str)> {
		if let Some(pack) = from {
			let piece = name.rsplit('/').next().unwrap_or(name);
			return pack
				.piece(piece)
				.map(|_| (pack, piece))
				.ok_or_else(|| anyhow!("pack `{}` has no piece `{piece}`", pack.manifest().name));
		}
		if let Some((pack, piece)) = name.split_once('/') {
			let found = self
				.library
				.get(pack)
				.ok_or_else(|| anyhow!("no pack `{pack}` is loaded; use --from <source> to load it"))?;
			return found
				.piece(piece)
				.map(|_| (found, piece))
				.ok_or_else(|| anyhow!("pack `{pack}` has no piece `{piece}`"));
		}
		let matches: Vec<&Pack> = self.library.iter().filter(|pack| pack.piece(name).is_some()).collect();
		match matches.as_slice() {
			[pack] => Ok((pack, name)),
			[] => bail!("no loaded pack has a piece `{name}`; see `cuilo pack list`"),
			many => bail!(
				"several packs have a piece `{name}` ({}); write it as `pack/{name}`",
				many
					.iter()
					.map(|pack| pack.manifest().name.as_str())
					.collect::<Vec<_>>()
					.join(", ")
			),
		}
	}
}

fn open_engine(user: &UserConfig, root: &Path, trusted: bool) -> anyhow::Result<Engine> {
	Engine::builder()
		.user_config(user.clone())
		.project_tools(trusted)
		.open(root)
		.with_context(|| format!("cannot open the project at {}", root.display()))
}

/// The names of the formatters the project uses, all of which run only once it is trusted; only names
/// cuilo knows, so the trust question holds no text the project wrote.
pub fn project_tools(engine: &Engine) -> anyhow::Result<Vec<String>> {
	Ok(
		engine
			.project()?
			.stack
			.values(Fact::Formatter)
			.filter(|tool| ORDER.contains(tool))
			.map(|tool| label(tool).to_owned())
			.collect(),
	)
}
