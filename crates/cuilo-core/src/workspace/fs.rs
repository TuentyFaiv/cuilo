//! File access behind [`Fs`]: the real disk, or an in-memory tree for previews and tests.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read as _, Write as _};
use std::path::{Component, Components, Path, PathBuf, Prefix};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::model::RelPath;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
/// The most links one link resolution follows, as an OS does.
const MAX_LINK_HOPS: usize = 40;
/// The most names one link resolution walks, those its links lead to included.
const MAX_WALK_NAMES: usize = 1024;

/// What a path is, without following symlinks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
	/// A regular file.
	File,
	/// A folder.
	Dir,
	/// A symlink or a Windows junction. Reads and writes never go through one; only
	/// [`Fs::resolve_links_inside_root`] follows it.
	Symlink,
	/// A FIFO, socket or device, which cuilo never reads or writes.
	Special,
}

/// Raw file operations below a project root; the engine adds the symlink and confinement checks.
pub trait Fs: Send + Sync + fmt::Debug {
	/// What `path` is, or `None` when nothing is there.
	fn entry(&self, path: &RelPath) -> io::Result<Option<EntryKind>>;
	/// Reads a file.
	fn read(&self, path: &RelPath) -> io::Result<Vec<u8>>;
	/// Reads a file of at most `limit` bytes; `None` when it is larger.
	fn read_limited(&self, path: &RelPath, limit: u64) -> io::Result<Option<Vec<u8>>> {
		let bytes = self.read(path)?;
		Ok((bytes.len() as u64 <= limit).then_some(bytes))
	}
	/// Creates or replaces a file atomically; its folder exists.
	fn write(&self, path: &RelPath, bytes: &[u8]) -> io::Result<()>;
	/// Creates a file, failing with [`ErrorKind::AlreadyExists`] if something is there.
	fn create_new(&self, path: &RelPath, bytes: &[u8]) -> io::Result<()>;
	/// Creates one folder; its parent exists.
	fn create_dir(&self, path: &RelPath) -> io::Result<()>;
	/// Removes a file or symlink.
	fn remove_file(&self, path: &RelPath) -> io::Result<()>;
	/// Removes an empty folder.
	fn remove_dir(&self, path: &RelPath) -> io::Result<()>;
	/// Moves a file.
	fn rename(&self, from: &RelPath, to: &RelPath) -> io::Result<()>;
	/// Sorted names in `dir`, or in the root when `dir` is `None`.
	fn list(&self, dir: Option<&RelPath>) -> io::Result<Vec<String>>;
	/// Whether the existing entries `a` and `b` are one entry, as `.GIT` and `.git` are on a disk that
	/// ignores case.
	fn same_entry(&self, a: &RelPath, b: &RelPath) -> io::Result<bool> {
		Ok(a == b)
	}
	/// Read-only, inside-root link resolution: the real path of `path` with the links in every segment
	/// followed, or `None` as soon as a link or `..` would leave the root, even to come back; an error when
	/// nothing is there, a link is broken or links loop. Nothing outside the root is looked at. Only import
	/// detection uses it, to read a tsconfig or `package.json`. The default follows no links.
	fn resolve_links_inside_root(&self, path: &RelPath) -> io::Result<Option<RelPath>> {
		Ok(Some(path.clone()))
	}
}

/// A project folder on disk.
#[derive(Clone, Debug)]
pub struct DiskFs {
	/// The canonical root.
	root: PathBuf,
	/// The root as opened, made absolute, which a link's absolute target may name instead.
	opened: PathBuf,
}

impl DiskFs {
	/// Opens the folder at `root`.
	pub fn open(root: &Path) -> io::Result<Self> {
		let canonical = root.canonicalize()?;
		if !canonical.is_dir() {
			return Err(io::Error::new(
				ErrorKind::NotADirectory,
				format!("`{}` is not a folder", canonical.display()),
			));
		}
		Ok(Self {
			opened: std::path::absolute(root)?,
			root: canonical,
		})
	}

	/// The canonical root.
	#[must_use]
	pub fn root(&self) -> &Path {
		&self.root
	}

	fn native(&self, path: &RelPath) -> PathBuf {
		path.under(&self.root)
	}

	/// What the in-root `path`, whose ancestors are real folders, is, without following it.
	fn look(&self, path: &RelPath) -> io::Result<Found> {
		let native = self.native(path);
		let meta = fs::symlink_metadata(&native)?;
		Ok(if meta.file_type().is_symlink() {
			Found::Link(self.link_target(&fs::read_link(&native)?))
		} else if meta.is_dir() {
			Found::Dir
		} else {
			Found::Leaf
		})
	}

	/// Where a link to `target` leads: names from its folder for a relative target, names from the root for
	/// an absolute one that starts with the root, canonical or as opened, and outside for anything else.
	fn link_target(&self, target: &Path) -> LinkTarget {
		let Some(text) = target.to_str() else {
			return LinkTarget::Outside;
		};
		let text = if cfg!(windows) {
			verbatim(text)
		} else {
			Cow::Borrowed(text)
		};
		let target = Path::new(text.as_ref());
		if !matches!(
			target.components().next(),
			Some(Component::Prefix(_) | Component::RootDir)
		) {
			return names(target.components()).map_or(LinkTarget::Outside, LinkTarget::Relative);
		}
		[&self.root, &self.opened]
			.into_iter()
			.find_map(|root| strip_root(target, root))
			.and_then(names)
			.map_or(LinkTarget::Outside, LinkTarget::FromRoot)
	}
}

impl Fs for DiskFs {
	fn entry(&self, path: &RelPath) -> io::Result<Option<EntryKind>> {
		match fs::symlink_metadata(self.native(path)) {
			Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
			Err(error) => Err(error),
			Ok(meta) if meta.file_type().is_symlink() => Ok(Some(EntryKind::Symlink)),
			Ok(meta) if meta.is_dir() => Ok(Some(EntryKind::Dir)),
			Ok(meta) if meta.is_file() => Ok(Some(EntryKind::File)),
			Ok(_) => Ok(Some(EntryKind::Special)),
		}
	}

	fn read(&self, path: &RelPath) -> io::Result<Vec<u8>> {
		fs::read(self.native(path))
	}

	/// Reads at most `limit + 1` bytes, so a file that grows or reports no size cannot make the read run on.
	fn read_limited(&self, path: &RelPath, limit: u64) -> io::Result<Option<Vec<u8>>> {
		let file = File::open(self.native(path))?;
		if !file.metadata()?.is_file() {
			return Err(io::Error::new(ErrorKind::InvalidInput, "not a regular file"));
		}
		let mut bytes = Vec::new();
		file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
		Ok((bytes.len() as u64 <= limit).then_some(bytes))
	}

	fn write(&self, path: &RelPath, bytes: &[u8]) -> io::Result<()> {
		let target = self.native(path);
		let parent = target.parent().unwrap_or(&self.root);
		let temp = parent.join(format!(
			".{}.cuilo-{}-{}.tmp",
			path.file_name(),
			std::process::id(),
			TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
		));
		let result = (|| {
			let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
			file.write_all(bytes)?;
			file.sync_all()?;
			fs::rename(&temp, &target)
		})();
		if result.is_err() {
			let _ = fs::remove_file(&temp);
		}
		result
	}

	fn create_new(&self, path: &RelPath, bytes: &[u8]) -> io::Result<()> {
		OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(self.native(path))?
			.write_all(bytes)
	}

	fn create_dir(&self, path: &RelPath) -> io::Result<()> {
		fs::create_dir(self.native(path))
	}

	fn remove_file(&self, path: &RelPath) -> io::Result<()> {
		fs::remove_file(self.native(path))
	}

	fn remove_dir(&self, path: &RelPath) -> io::Result<()> {
		fs::remove_dir(self.native(path))
	}

	fn rename(&self, from: &RelPath, to: &RelPath) -> io::Result<()> {
		fs::rename(self.native(from), self.native(to))
	}

	fn list(&self, dir: Option<&RelPath>) -> io::Result<Vec<String>> {
		let native = dir.map_or_else(|| self.root.clone(), |dir| self.native(dir));
		let mut names = Vec::new();
		for entry in fs::read_dir(native)? {
			if let Ok(name) = entry?.file_name().into_string() {
				names.push(name);
			}
		}
		names.sort();
		Ok(names)
	}

	fn same_entry(&self, a: &RelPath, b: &RelPath) -> io::Result<bool> {
		same_file(&self.native(a), &self.native(b))
	}

	/// Walks `path` from the canonical root as [`MemoryFs`] does, looking at each name without following it,
	/// so the OS never follows a link and nothing outside the root is looked at.
	fn resolve_links_inside_root(&self, path: &RelPath) -> io::Result<Option<RelPath>> {
		resolve_inside_root(path, |current| self.look(current))
	}
}

/// What one in-root path is to [`resolve_inside_root`], without following it.
enum Found {
	/// A folder the walk can enter.
	Dir,
	/// A file, or anything else the walk cannot enter.
	Leaf,
	/// A link, with where it leads.
	Link(LinkTarget),
}

/// Where a link leads, for [`resolve_inside_root`].
#[derive(Debug, PartialEq, Eq)]
enum LinkTarget {
	/// Names from the link's folder, `.` and `..` included.
	Relative(Vec<String>),
	/// Names from the root, for an absolute target that starts with it.
	FromRoot(Vec<String>),
	/// Anywhere else, or a target that is not UTF-8.
	Outside,
}

/// Walks `path` from the root one name at a time, as an OS resolves it: `look` tells what each in-root path
/// is, a link's target takes its place, and `..` leaves the folder reached. `None` as soon as the walk would
/// leave the root, even to come back; an error when a name is missing, a file is in the way, or there are
/// too many links or names.
fn resolve_inside_root(
	path: &RelPath,
	mut look: impl FnMut(&RelPath) -> io::Result<Found>,
) -> io::Result<Option<RelPath>> {
	let mut pending: Vec<String> = path.as_str().rsplit('/').map(str::to_owned).collect();
	let mut real: Vec<String> = Vec::new();
	let mut names = pending.len();
	let mut hops = 0;
	while let Some(name) = pending.pop() {
		match name.as_str() {
			"" | "." => {}
			".." => {
				if real.pop().is_none() {
					return Ok(None);
				}
			}
			name if name.contains('/') => return Ok(None),
			_ => {
				real.push(name);
				let Ok(current) = RelPath::new(&real.join("/")) else {
					return Ok(None);
				};
				match look(&current)? {
					Found::Link(target) => {
						real.pop();
						let spliced = match target {
							LinkTarget::Relative(spliced) => spliced,
							LinkTarget::FromRoot(spliced) => {
								real.clear();
								spliced
							}
							LinkTarget::Outside => return Ok(None),
						};
						hops += 1;
						names += spliced.len();
						if hops > MAX_LINK_HOPS || names > MAX_WALK_NAMES {
							return Err(io::Error::new(ErrorKind::InvalidInput, "too many links or names"));
						}
						pending.extend(spliced.into_iter().rev());
					}
					Found::Leaf if !pending.is_empty() => return Err(ErrorKind::NotADirectory.into()),
					Found::Leaf | Found::Dir => {}
				}
			}
		}
	}
	Ok(RelPath::new(&real.join("/")).ok())
}

/// `components` as names for [`resolve_inside_root`]; `None` for a prefix, a root or a name that is not UTF-8.
fn names(components: Components<'_>) -> Option<Vec<String>> {
	components
		.map(|component| match component {
			Component::Normal(name) => name.to_str().map(str::to_owned),
			Component::CurDir => Some(".".to_owned()),
			Component::ParentDir => Some("..".to_owned()),
			Component::Prefix(_) | Component::RootDir => None,
		})
		.collect()
}

/// What follows `root` in `target`, when `target` starts with every component of `root`.
fn strip_root<'a>(target: &'a Path, root: &Path) -> Option<Components<'a>> {
	let mut rest = target.components();
	for expected in root.components() {
		if !same_component(rest.next()?, expected) {
			return None;
		}
	}
	Some(rest)
}

/// Whether two components of absolute paths are one; `.` and `..` never are.
fn same_component(a: Component<'_>, b: Component<'_>) -> bool {
	match (a, b) {
		(Component::Prefix(a), Component::Prefix(b)) => same_prefix(a.kind(), b.kind()),
		(Component::RootDir, Component::RootDir) => true,
		(Component::Normal(a), Component::Normal(b)) => same_name(a, b),
		_ => false,
	}
}

/// Whether two Windows prefixes name one volume: `\\?\C:` is `C:` and `\\?\UNC\server\share` is
/// `\\server\share`, ignoring ASCII case.
fn same_prefix(a: Prefix<'_>, b: Prefix<'_>) -> bool {
	match (plain_prefix(a), plain_prefix(b)) {
		(Prefix::Disk(a), Prefix::Disk(b)) => a.eq_ignore_ascii_case(&b),
		(Prefix::UNC(a_server, a_share), Prefix::UNC(b_server, b_share)) => {
			a_server.eq_ignore_ascii_case(b_server) && a_share.eq_ignore_ascii_case(b_share)
		}
		(Prefix::Verbatim(a), Prefix::Verbatim(b)) | (Prefix::DeviceNS(a), Prefix::DeviceNS(b)) => {
			a.eq_ignore_ascii_case(b)
		}
		_ => false,
	}
}

/// `prefix` with a verbatim disk or UNC prefix in its plain form.
fn plain_prefix(prefix: Prefix<'_>) -> Prefix<'_> {
	match prefix {
		Prefix::VerbatimDisk(disk) => Prefix::Disk(disk),
		Prefix::VerbatimUNC(server, share) => Prefix::UNC(server, share),
		other => other,
	}
}

/// Whether two path names are one, ignoring ASCII case on Windows as its file systems do.
fn same_name(a: &OsStr, b: &OsStr) -> bool {
	if cfg!(windows) {
		a.eq_ignore_ascii_case(b)
	} else {
		a == b
	}
}

/// `text` with the NT prefix `\??\` written as the verbatim prefix `\\?\` it stands for.
fn verbatim(text: &str) -> Cow<'_, str> {
	text
		.strip_prefix(r"\??\")
		.map_or(Cow::Borrowed(text), |rest| Cow::Owned(format!(r"\\?\{rest}")))
}

/// Makes a FIFO at `path` with `mkfifo`; `false` when that is not available.
#[cfg(test)]
#[cfg(unix)]
pub(crate) fn make_fifo(path: &Path) -> bool {
	std::process::Command::new("mkfifo")
		.arg(path)
		.status()
		.is_ok_and(|status| status.success())
}

/// What `run` returns, failing the test when it takes more than ten seconds, as a blocked read would.
#[cfg(test)]
#[cfg(unix)]
pub(crate) fn within_seconds<T: Send + 'static>(run: impl FnOnce() -> T + Send + 'static) -> T {
	let (send, receive) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let _ = send.send(run());
	});
	receive.recv_timeout(std::time::Duration::from_secs(10)).unwrap()
}

/// Whether `a` and `b` are one file or folder, not following a final symlink.
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> io::Result<bool> {
	use std::os::unix::fs::MetadataExt as _;
	let (a, b) = (fs::symlink_metadata(a)?, fs::symlink_metadata(b)?);
	Ok(a.dev() == b.dev() && a.ino() == b.ino())
}

/// Whether `a` and `b` are one file or folder; Windows resolves case and short names such as `GIT~1`.
#[cfg(not(unix))]
fn same_file(a: &Path, b: &Path) -> io::Result<bool> {
	Ok(fs::canonicalize(a)? == fs::canonicalize(b)?)
}

/// An in-memory project tree.
#[derive(Debug, Default)]
pub struct MemoryFs {
	nodes: Mutex<BTreeMap<RelPath, Node>>,
}

#[derive(Clone, Debug)]
enum Node {
	File(Vec<u8>),
	Dir,
	/// A link to its target, written as a link on disk holds it.
	Symlink(String),
}

impl MemoryFs {
	/// An empty tree.
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// Adds a file, creating its folders.
	pub fn insert(&self, path: &RelPath, bytes: impl Into<Vec<u8>>) {
		let mut nodes = self.nodes();
		for ancestor in path.ancestors() {
			nodes.insert(ancestor, Node::Dir);
		}
		nodes.insert(path.clone(), Node::File(bytes.into()));
	}

	/// Adds a symlink that leads out of the tree, to exercise symlink handling.
	pub fn insert_symlink(&self, path: &RelPath) {
		self.insert_symlink_to(path, "/");
	}

	/// Adds a symlink to `target`, creating its folders. `target` is relative to the link's folder and may
	/// hold `..`; an absolute one leads out of the tree.
	pub fn insert_symlink_to(&self, path: &RelPath, target: &str) {
		let mut nodes = self.nodes();
		for ancestor in path.ancestors() {
			nodes.insert(ancestor, Node::Dir);
		}
		nodes.insert(path.clone(), Node::Symlink(target.to_owned()));
	}

	/// Every file, by path.
	#[must_use]
	pub fn files(&self) -> BTreeMap<RelPath, Vec<u8>> {
		self
			.nodes()
			.iter()
			.filter_map(|(path, node)| match node {
				Node::File(bytes) => Some((path.clone(), bytes.clone())),
				Node::Dir | Node::Symlink(_) => None,
			})
			.collect()
	}

	fn nodes(&self) -> MutexGuard<'_, BTreeMap<RelPath, Node>> {
		self.nodes.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

fn require_parent(nodes: &BTreeMap<RelPath, Node>, path: &RelPath) -> io::Result<()> {
	match path.parent() {
		None => Ok(()),
		Some(parent) => match nodes.get(&parent) {
			Some(Node::Dir) => Ok(()),
			Some(_) => Err(ErrorKind::NotADirectory.into()),
			None => Err(ErrorKind::NotFound.into()),
		},
	}
}

fn is_child(dir: Option<&RelPath>, path: &RelPath) -> bool {
	path.parent().as_ref() == dir
}

impl Fs for MemoryFs {
	fn entry(&self, path: &RelPath) -> io::Result<Option<EntryKind>> {
		Ok(self.nodes().get(path).map(|node| match node {
			Node::File(_) => EntryKind::File,
			Node::Dir => EntryKind::Dir,
			Node::Symlink(_) => EntryKind::Symlink,
		}))
	}

	fn read(&self, path: &RelPath) -> io::Result<Vec<u8>> {
		match self.nodes().get(path) {
			Some(Node::File(bytes)) => Ok(bytes.clone()),
			Some(Node::Dir) => Err(ErrorKind::IsADirectory.into()),
			Some(Node::Symlink(_)) => Err(ErrorKind::InvalidInput.into()),
			None => Err(ErrorKind::NotFound.into()),
		}
	}

	fn write(&self, path: &RelPath, bytes: &[u8]) -> io::Result<()> {
		let mut nodes = self.nodes();
		require_parent(&nodes, path)?;
		if matches!(nodes.get(path), Some(Node::Dir)) {
			return Err(ErrorKind::IsADirectory.into());
		}
		nodes.insert(path.clone(), Node::File(bytes.to_vec()));
		Ok(())
	}

	fn create_new(&self, path: &RelPath, bytes: &[u8]) -> io::Result<()> {
		let mut nodes = self.nodes();
		require_parent(&nodes, path)?;
		if nodes.contains_key(path) {
			return Err(ErrorKind::AlreadyExists.into());
		}
		nodes.insert(path.clone(), Node::File(bytes.to_vec()));
		Ok(())
	}

	fn create_dir(&self, path: &RelPath) -> io::Result<()> {
		let mut nodes = self.nodes();
		require_parent(&nodes, path)?;
		if nodes.contains_key(path) {
			return Err(ErrorKind::AlreadyExists.into());
		}
		nodes.insert(path.clone(), Node::Dir);
		Ok(())
	}

	fn remove_file(&self, path: &RelPath) -> io::Result<()> {
		let mut nodes = self.nodes();
		match nodes.get(path) {
			Some(Node::File(_) | Node::Symlink(_)) => {
				nodes.remove(path);
				Ok(())
			}
			Some(Node::Dir) => Err(ErrorKind::IsADirectory.into()),
			None => Err(ErrorKind::NotFound.into()),
		}
	}

	fn remove_dir(&self, path: &RelPath) -> io::Result<()> {
		let mut nodes = self.nodes();
		match nodes.get(path) {
			Some(Node::Dir) if nodes.keys().any(|other| is_child(Some(path), other)) => {
				Err(ErrorKind::DirectoryNotEmpty.into())
			}
			Some(Node::Dir) => {
				nodes.remove(path);
				Ok(())
			}
			Some(_) => Err(ErrorKind::NotADirectory.into()),
			None => Err(ErrorKind::NotFound.into()),
		}
	}

	fn rename(&self, from: &RelPath, to: &RelPath) -> io::Result<()> {
		let mut nodes = self.nodes();
		require_parent(&nodes, to)?;
		match nodes.get(from) {
			Some(Node::File(_) | Node::Symlink(_)) => {
				let node = nodes.remove(from).ok_or(ErrorKind::NotFound)?;
				nodes.insert(to.clone(), node);
				Ok(())
			}
			Some(Node::Dir) => Err(ErrorKind::IsADirectory.into()),
			None => Err(ErrorKind::NotFound.into()),
		}
	}

	fn list(&self, dir: Option<&RelPath>) -> io::Result<Vec<String>> {
		let nodes = self.nodes();
		if let Some(dir) = dir {
			match nodes.get(dir) {
				Some(Node::Dir) => {}
				Some(_) => return Err(ErrorKind::NotADirectory.into()),
				None => return Err(ErrorKind::NotFound.into()),
			}
		}
		Ok(
			nodes
				.keys()
				.filter(|path| is_child(dir, path))
				.map(|path| path.file_name().to_owned())
				.collect(),
		)
	}

	/// Walks `path` one name at a time, replacing each link with its target, as [`DiskFs`] does; an
	/// absolute target leads out of the tree.
	fn resolve_links_inside_root(&self, path: &RelPath) -> io::Result<Option<RelPath>> {
		let nodes = self.nodes();
		resolve_inside_root(path, |current| match nodes.get(current) {
			None => Err(ErrorKind::NotFound.into()),
			Some(Node::Dir) => Ok(Found::Dir),
			Some(Node::File(_)) => Ok(Found::Leaf),
			Some(Node::Symlink(target)) if target.starts_with('/') => Ok(Found::Link(LinkTarget::Outside)),
			Some(Node::Symlink(target)) => Ok(Found::Link(LinkTarget::Relative(
				target.split('/').map(str::to_owned).collect(),
			))),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn rel(path: &str) -> RelPath {
		RelPath::new(path).unwrap()
	}

	fn round_trip(fs: &dyn Fs) {
		fs.create_dir(&rel("a")).unwrap();
		fs.write(&rel("a/b.txt"), b"one").unwrap();
		fs.write(&rel("a/b.txt"), b"two").unwrap();
		fs.create_new(&rel("a/c.txt"), b"x").unwrap();

		assert_eq!(fs.read(&rel("a/b.txt")).unwrap(), b"two");
		assert_eq!(fs.list(Some(&rel("a"))).unwrap(), ["b.txt", "c.txt"]);
		assert_eq!(fs.list(None).unwrap(), ["a"]);
		assert_eq!(
			fs.create_new(&rel("a/c.txt"), b"y").unwrap_err().kind(),
			ErrorKind::AlreadyExists
		);
		assert!(fs.remove_dir(&rel("a")).is_err());

		fs.rename(&rel("a/c.txt"), &rel("a/d.txt")).unwrap();
		fs.remove_file(&rel("a/b.txt")).unwrap();
		fs.remove_file(&rel("a/d.txt")).unwrap();
		fs.remove_dir(&rel("a")).unwrap();

		assert_eq!(fs.entry(&rel("a")).unwrap(), None);
	}

	#[test]
	fn disk_fs_should_follow_the_contract() {
		let dir = tempfile::tempdir().unwrap();

		round_trip(&DiskFs::open(dir.path()).unwrap());
	}

	#[test]
	fn memory_fs_should_follow_the_contract() {
		round_trip(&MemoryFs::new());
	}

	#[test]
	fn memory_fs_should_require_parent_folder() {
		let fs = MemoryFs::new();

		assert_eq!(fs.write(&rel("a/b.txt"), b"x").unwrap_err().kind(), ErrorKind::NotFound);
	}

	#[test]
	fn disk_fs_write_should_leave_no_temp_files() {
		let dir = tempfile::tempdir().unwrap();
		let fs = DiskFs::open(dir.path()).unwrap();
		fs.write(&rel("a.txt"), b"x").unwrap();

		assert_eq!(fs.list(None).unwrap(), ["a.txt"]);
	}

	#[test]
	fn memory_fs_should_resolve_links_like_a_disk_without_leaving_the_tree() {
		let fs = MemoryFs::new();
		fs.insert(&rel("store/pkg/tsconfig.json"), "{}");
		for (link, target) in [
			("node_modules/pkg", "../store/pkg"),
			("node_modules/alias", "./pkg/tsconfig.json"),
			("absolute", "/store/pkg"),
			("up", "../store/pkg"),
			("broken", "store/missing"),
			("loop", "loop"),
			("through_a_file", "store/pkg/tsconfig.json/x"),
		] {
			fs.insert_symlink_to(&rel(link), target);
		}
		let resolve = |path: &str| fs.resolve_links_inside_root(&rel(path));
		let real = Some(rel("store/pkg/tsconfig.json"));

		assert_eq!(resolve("node_modules/pkg/tsconfig.json").unwrap(), real);
		assert_eq!(resolve("node_modules/alias").unwrap(), real);
		assert_eq!(resolve("store/pkg/tsconfig.json").unwrap(), real);
		assert_eq!(resolve("absolute/tsconfig.json").unwrap(), None);
		assert_eq!(resolve("up/tsconfig.json").unwrap(), None);
		assert_eq!(resolve("broken").unwrap_err().kind(), ErrorKind::NotFound);
		assert!(resolve("loop").is_err());
		assert_eq!(resolve("through_a_file").unwrap_err().kind(), ErrorKind::NotADirectory);
	}

	#[test]
	fn memory_fs_should_follow_at_most_max_link_hops() {
		let fs = MemoryFs::new();
		fs.insert(&rel("real.json"), "{}");
		fs.insert_symlink_to(&rel("link0"), "real.json");
		for hop in 1..=MAX_LINK_HOPS {
			fs.insert_symlink_to(&rel(&format!("link{hop}")), &format!("link{}", hop - 1));
		}

		assert_eq!(
			fs.resolve_links_inside_root(&rel(&format!("link{}", MAX_LINK_HOPS - 1)))
				.unwrap(),
			Some(rel("real.json"))
		);
		assert!(
			fs.resolve_links_inside_root(&rel(&format!("link{MAX_LINK_HOPS}")))
				.is_err()
		);
	}

	#[cfg(unix)]
	#[test]
	fn disk_fs_should_resolve_links_only_to_real_paths_inside_the_root() {
		let parent = tempfile::tempdir().unwrap();
		let root = parent.path().join("app");
		std::fs::create_dir_all(root.join("store/pkg")).unwrap();
		std::fs::write(root.join("store/pkg/tsconfig.json"), "{}").unwrap();
		std::fs::create_dir(parent.path().join("app-sibling")).unwrap();
		std::fs::write(parent.path().join("app-sibling/tsconfig.json"), "{}").unwrap();
		let absolute = parent.path().join("app-sibling");
		for (link, target) in [
			("pkg", Path::new("store/pkg")),
			("round_trip", Path::new("../app/store/pkg")),
			("sibling", Path::new("../app-sibling")),
			("absolute", absolute.as_path()),
			("broken", Path::new("store/missing")),
			("loop", Path::new("loop")),
		] {
			std::os::unix::fs::symlink(target, root.join(link)).unwrap();
		}
		let fs = DiskFs::open(&root).unwrap();
		let resolve = |path: &str| fs.resolve_links_inside_root(&rel(path));
		let real = Some(rel("store/pkg/tsconfig.json"));

		assert_eq!(resolve("pkg/tsconfig.json").unwrap(), real);
		assert_eq!(resolve("round_trip/tsconfig.json").unwrap(), None);
		assert_eq!(resolve("store/pkg/tsconfig.json").unwrap(), real);
		assert_eq!(resolve("sibling/tsconfig.json").unwrap(), None);
		assert_eq!(resolve("absolute/tsconfig.json").unwrap(), None);
		assert_eq!(resolve("broken").unwrap_err().kind(), ErrorKind::NotFound);
		assert!(resolve("loop").is_err());
	}

	#[test]
	fn memory_fs_should_walk_at_most_max_walk_names() {
		let fs = MemoryFs::new();
		fs.insert(&rel("real.json"), "{}");
		fs.insert_symlink_to(&rel("short"), &format!("{}real.json", "./".repeat(MAX_WALK_NAMES / 2)));
		fs.insert_symlink_to(&rel("long"), &format!("{}real.json", "./".repeat(MAX_WALK_NAMES)));

		assert_eq!(
			fs.resolve_links_inside_root(&rel("short")).unwrap(),
			Some(rel("real.json"))
		);
		assert!(fs.resolve_links_inside_root(&rel("long")).is_err());
	}

	#[cfg(unix)]
	#[test]
	fn disk_fs_should_follow_absolute_links_only_when_they_start_with_the_root() {
		let parent = tempfile::tempdir().unwrap();
		let root = parent.path().join("app");
		std::fs::create_dir_all(root.join("store/pkg")).unwrap();
		std::fs::write(root.join("store/pkg/tsconfig.json"), "{}").unwrap();
		let fs = DiskFs::open(&root).unwrap();
		for (link, target) in [
			("canonical", fs.root().join("store/pkg")),
			("as_opened", root.join("store/pkg")),
			("out_and_back", root.join("../app/store/pkg")),
			("up_through_the_root", fs.root().join("store/../../app/store/pkg")),
		] {
			std::os::unix::fs::symlink(target, root.join(link)).unwrap();
		}
		let resolve = |path: &str| fs.resolve_links_inside_root(&rel(path)).unwrap();
		let real = Some(rel("store/pkg/tsconfig.json"));

		assert_eq!(resolve("canonical/tsconfig.json"), real);
		assert_eq!(resolve("as_opened/tsconfig.json"), real);
		assert_eq!(resolve("out_and_back/tsconfig.json"), None);
		assert_eq!(resolve("up_through_the_root/tsconfig.json"), None);
	}

	#[cfg(unix)]
	#[test]
	fn disk_fs_should_refuse_round_trips_out_of_the_root_without_looking_outside() {
		let parent = tempfile::tempdir().unwrap();
		let root = parent.path().join("app");
		let probe = parent.path().join("probe");
		std::fs::create_dir_all(root.join("store/pkg")).unwrap();
		std::fs::write(root.join("store/pkg/tsconfig.json"), "{}").unwrap();
		std::os::unix::fs::symlink("../probe/../app/store/pkg", root.join("relative")).unwrap();
		std::os::unix::fs::symlink(probe.join("../app/store/pkg"), root.join("absolute")).unwrap();
		let fs = DiskFs::open(&root).unwrap();
		for probe_exists in [false, true] {
			if probe_exists {
				std::fs::create_dir(&probe).unwrap();
			}
			for link in ["relative", "absolute"] {
				let mut looked = Vec::new();
				let resolved = resolve_inside_root(&rel(&format!("{link}/tsconfig.json")), |current| {
					looked.push(fs.native(current));
					fs.look(current)
				});

				assert_eq!(resolved.unwrap(), None, "{link} {probe_exists}");
				assert_eq!(looked, [fs.native(&rel(link))], "{link} {probe_exists}");
			}
		}
		assert!(std::fs::canonicalize(root.join("relative/tsconfig.json")).is_ok());
	}

	#[cfg(unix)]
	#[test]
	fn disk_fs_should_read_only_regular_files_and_at_most_the_limit() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("ten.json"), "0123456789").unwrap();
		std::os::unix::fs::symlink("/dev/zero", dir.path().join("zero")).unwrap();
		let fs = DiskFs::open(dir.path()).unwrap();

		assert_eq!(
			fs.read_limited(&rel("ten.json"), 10).unwrap().as_deref(),
			Some(&b"0123456789"[..])
		);
		assert_eq!(fs.read_limited(&rel("ten.json"), 9).unwrap(), None);
		assert_eq!(
			fs.read_limited(&rel("zero"), 10).unwrap_err().kind(),
			ErrorKind::InvalidInput
		);
		if make_fifo(&dir.path().join("pipe")) {
			assert_eq!(fs.entry(&rel("pipe")).unwrap(), Some(EntryKind::Special));
		}
	}

	#[test]
	fn windows_prefixes_should_match_in_their_verbatim_and_plain_forms() {
		let (server, share) = (OsStr::new("Server"), OsStr::new("share"));
		for (a, b, same) in [
			(Prefix::VerbatimDisk(b'C'), Prefix::Disk(b'c'), true),
			(Prefix::Disk(b'C'), Prefix::Disk(b'D'), false),
			(
				Prefix::VerbatimUNC(server, share),
				Prefix::UNC(OsStr::new("server"), OsStr::new("SHARE")),
				true,
			),
			(
				Prefix::UNC(server, share),
				Prefix::UNC(server, OsStr::new("other")),
				false,
			),
			(Prefix::Verbatim(OsStr::new("Volume{0}")), Prefix::Disk(b'C'), false),
			(Prefix::DeviceNS(OsStr::new("C:")), Prefix::Disk(b'C'), false),
		] {
			assert_eq!(same_prefix(a, b), same, "{a:?} {b:?}");
		}
		assert_eq!(verbatim(r"\??\C:\app"), r"\\?\C:\app");
		assert_eq!(verbatim(r"C:\app"), r"C:\app");
	}

	#[cfg(windows)]
	#[test]
	fn disk_fs_should_match_absolute_windows_targets_in_every_form_of_the_root() {
		let dir = tempfile::tempdir().unwrap();
		let fs = DiskFs::open(dir.path()).unwrap();
		let root = fs.root().to_str().unwrap().to_owned();
		let plain = root.strip_prefix(r"\\?\").unwrap().to_owned();
		let opened = dir.path().to_str().unwrap().to_owned();
		let inside = LinkTarget::FromRoot(vec!["store".to_owned()]);
		for target in [
			format!(r"{root}\store"),
			format!(r"{plain}\store"),
			format!(r"\??\{plain}\store"),
			format!(r"{}\store", plain.to_ascii_lowercase()),
			format!("{plain}/store"),
			format!(r"{opened}\store"),
		] {
			assert_eq!(fs.link_target(Path::new(&target)), inside, "{target}");
		}
		for target in [
			r"\\server\share\store",
			r"\\?\UNC\server\share\store",
			r"\\?\Volume{0}\store",
			r"\store",
			"C:store",
		] {
			assert_eq!(fs.link_target(Path::new(target)), LinkTarget::Outside, "{target}");
		}
	}

	#[cfg(unix)]
	#[test]
	fn disk_fs_should_report_symlinks_without_following() {
		let dir = tempfile::tempdir().unwrap();
		std::os::unix::fs::symlink("/etc/hosts", dir.path().join("link")).unwrap();

		assert_eq!(
			DiskFs::open(dir.path()).unwrap().entry(&rel("link")).unwrap(),
			Some(EntryKind::Symlink)
		);
	}
}
