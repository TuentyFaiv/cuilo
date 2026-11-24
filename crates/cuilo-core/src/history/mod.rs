//! Transactions: every apply is journaled in `.cuilo/history` and can be undone.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::event::{Event, EventSink};
use crate::model::{Digest, RelPath};
use crate::plan::is_forbidden_in;
use crate::workspace::{Lockfile, ObjectStore, StoreError, Workspace, WorkspaceError};

const HISTORY_DIR: &str = "history";
const UNDONE_DIR: &str = "history/undone";
const BUSY_FILE: &str = "busy";

/// One file a transaction writes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Edit<'a> {
	/// Project-relative path.
	pub path: &'a RelPath,
	/// Digest of the content the plan saw, or `None` if the file did not exist.
	pub expected: Option<Digest>,
	/// New content, or `None` to remove the file.
	pub contents: Option<&'a [u8]>,
}

/// One journaled apply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
	/// Sequential id.
	pub id: u64,
	/// Seconds since the Unix epoch.
	pub time: u64,
	/// What the transaction did.
	pub summary: String,
	/// Whether it finished.
	pub state: TransactionState,
	/// Lockfile before the transaction.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub lock_before: Option<Digest>,
	/// Lockfile after the transaction.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub lock_after: Option<Digest>,
	/// Folders the transaction created.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub created_dirs: Vec<RelPath>,
	/// Files changed.
	#[serde(default, rename = "change", skip_serializing_if = "Vec::is_empty")]
	pub changes: Vec<ChangeRecord>,
}

/// Whether a transaction finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
	/// Interrupted before finishing; `undo` rolls it back.
	Pending,
	/// Finished.
	Applied,
}

/// One file a transaction changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeRecord {
	/// Project-relative path.
	pub path: RelPath,
	/// Content before, or `None` if the file did not exist.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub before: Option<Digest>,
	/// Content after, or `None` if the transaction removed the file.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub after: Option<Digest>,
}

/// Why a transaction failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HistoryError {
	/// Another cuilo process is working in this project.
	#[error("another cuilo process is changing this project (remove `.cuilo/busy` if it crashed)")]
	Busy,
	/// A file changed after the plan was made.
	#[error("`{0}` changed after the plan was made; plan again")]
	Changed(RelPath),
	/// The last transaction never finished.
	#[error("transaction {0} was interrupted; run undo to roll it back first")]
	Interrupted(u64),
	/// Files changed after the transaction; undo would lose those edits.
	#[error("these files changed after transaction {id}; undo with force to discard the edits: {}", .paths.iter().map(RelPath::as_str).collect::<Vec<_>>().join(", "))]
	Diverged {
		/// Transaction id.
		id: u64,
		/// Files that changed.
		paths: Vec<RelPath>,
	},
	/// A journal entry names a path cuilo never writes, so cuilo did not record it.
	#[error(
		"history entry {id} names `{path}`, which cuilo never writes; delete the entry from `.cuilo/history` if you did not create it"
	)]
	Forbidden {
		/// Transaction id.
		id: u64,
		/// The refused path.
		path: RelPath,
	},
	/// A blob needed to undo is gone from the cache.
	#[error("cannot undo: cached content {0} is missing from `.cuilo/`")]
	MissingObject(Digest),
	/// A journal entry is unreadable.
	#[error("history entry `{path}` is invalid: {message}")]
	Corrupt {
		/// The entry file.
		path: RelPath,
		/// What is wrong.
		message: String,
	},
	/// A project file operation failed.
	#[error(transparent)]
	Workspace(#[from] WorkspaceError),
	/// The object cache failed.
	#[error(transparent)]
	Store(#[from] StoreError),
}

/// The journal in `.cuilo/history`.
#[derive(Clone, Debug)]
pub(crate) struct History<'a> {
	workspace: &'a Workspace,
}

impl<'a> History<'a> {
	pub(crate) fn new(workspace: &'a Workspace) -> Self {
		Self { workspace }
	}

	/// Every transaction that can still be undone, oldest first.
	pub(crate) fn entries(&self) -> Result<Vec<Transaction>, HistoryError> {
		let mut entries = self.read_dir(HISTORY_DIR)?;
		entries.sort_by_key(|entry| entry.id);
		Ok(entries)
	}

	fn last(&self) -> Result<Option<Transaction>, HistoryError> {
		Ok(self.entries()?.pop())
	}

	fn next_id(&self) -> Result<u64, HistoryError> {
		let undone = self.read_dir(UNDONE_DIR)?;
		let max = self
			.entries()?
			.iter()
			.chain(&undone)
			.map(|entry| entry.id)
			.max()
			.unwrap_or(0);
		Ok(max + 1)
	}

	fn save(&self, transaction: &Transaction) -> Result<(), HistoryError> {
		let path = entry_path(HISTORY_DIR, transaction.id);
		let text = toml::to_string(transaction).map_err(|error| HistoryError::Corrupt {
			path: path.clone(),
			message: error.to_string(),
		})?;
		self.workspace.write(&path, text.as_bytes())?;
		Ok(())
	}

	fn discard(&self, id: u64) {
		let _ = self.workspace.remove(&entry_path(HISTORY_DIR, id));
	}

	fn archive(&self, id: u64) -> Result<(), HistoryError> {
		Ok(
			self
				.workspace
				.rename(&entry_path(HISTORY_DIR, id), &entry_path(UNDONE_DIR, id))?,
		)
	}

	fn read_dir(&self, dir: &str) -> Result<Vec<Transaction>, HistoryError> {
		let dir = Workspace::state_path(dir);
		let mut transactions = Vec::new();
		for name in self.workspace.list(Some(&dir))? {
			let is_toml = Path::new(&name)
				.extension()
				.is_some_and(|extension| extension.eq_ignore_ascii_case("toml"));
			let Some(path) = is_toml.then(|| RelPath::new(&format!("{dir}/{name}")).ok()).flatten() else {
				continue;
			};
			let Some(bytes) = self.workspace.read(&path)? else {
				continue;
			};
			let transaction = toml::from_str(&String::from_utf8_lossy(&bytes)).map_err(|error| HistoryError::Corrupt {
				path: path.clone(),
				message: error.message().to_owned(),
			})?;
			transactions.push(transaction);
		}
		Ok(transactions)
	}
}

fn entry_path(dir: &str, id: u64) -> RelPath {
	Workspace::state_path(&format!("{dir}/{id:08}.toml"))
}

/// Holds `.cuilo/busy` so only one process changes the project at a time.
struct Busy<'a> {
	workspace: &'a Workspace,
}

impl<'a> Busy<'a> {
	fn acquire(workspace: &'a Workspace) -> Result<Self, HistoryError> {
		workspace.init_state()?;
		let pid = format!("{}\n", std::process::id());
		if workspace.create_new(&Workspace::state_path(BUSY_FILE), pid.as_bytes())? {
			Ok(Self { workspace })
		} else {
			Err(HistoryError::Busy)
		}
	}
}

impl Drop for Busy<'_> {
	fn drop(&mut self) {
		let _ = self.workspace.remove(&Workspace::state_path(BUSY_FILE));
	}
}

/// Writes `edits` and the new lockfile as one transaction.
pub(crate) fn apply(
	workspace: &Workspace,
	summary: String,
	edits: &[Edit<'_>],
	lock_after: Option<&[u8]>,
	sink: &dyn EventSink,
) -> Result<Transaction, HistoryError> {
	let _busy = Busy::acquire(workspace)?;
	let (store, history) = (ObjectStore::new(workspace), History::new(workspace));

	if let Some(last) = history.last()?
		&& last.state == TransactionState::Pending
	{
		return Err(HistoryError::Interrupted(last.id));
	}

	let mut befores = Vec::with_capacity(edits.len());
	for edit in edits {
		let before = workspace.read(edit.path)?;
		if before.as_deref().map(Digest::of) != edit.expected {
			return Err(HistoryError::Changed(edit.path.clone()));
		}
		befores.push(before);
	}

	let lock_path = Lockfile::path();
	let lock_before = workspace.read(&lock_path)?;
	let mut transaction = Transaction {
		id: history.next_id()?,
		time: SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map_or(0, |since| since.as_secs()),
		summary,
		state: TransactionState::Pending,
		lock_before: lock_before.as_deref().map(|bytes| store.put(bytes)).transpose()?,
		lock_after: lock_after.map(|bytes| store.put(bytes)).transpose()?,
		created_dirs: Vec::new(),
		changes: Vec::new(),
	};
	for (edit, before) in edits.iter().zip(&befores) {
		transaction.changes.push(ChangeRecord {
			path: edit.path.clone(),
			before: before.as_deref().map(|bytes| store.put(bytes)).transpose()?,
			after: edit.contents.map(|bytes| store.put(bytes)).transpose()?,
		});
	}
	history.save(&transaction)?;

	let mut written = 0;
	let result = (|| -> Result<(), HistoryError> {
		for edit in edits {
			if let Some(bytes) = edit.contents {
				transaction.created_dirs.extend(workspace.write(edit.path, bytes)?);
				sink.emit(&Event::Wrote(edit.path));
			} else {
				workspace.remove(edit.path)?;
				sink.emit(&Event::Removed(edit.path));
			}
			written += 1;
		}
		if let Some(lock) = lock_after {
			transaction.created_dirs.extend(workspace.write(&lock_path, lock)?);
		}
		Ok(())
	})();

	if let Err(error) = result {
		rollback(workspace, &store, &transaction, written, lock_before.as_deref());
		history.discard(transaction.id);
		sink.emit(&Event::RolledBack(transaction.id));
		return Err(error);
	}

	transaction.state = TransactionState::Applied;
	history.save(&transaction)?;
	sink.emit(&Event::Applied {
		transaction: transaction.id,
		changes: transaction.changes.len(),
	});
	Ok(transaction)
}

/// Undoes the last transaction; returns it, or `None` when there is nothing to undo.
///
/// The journal is project state that a cloned project may carry, so it is checked before anything is
/// written: no path may be one cuilo never writes, and every file must still hold what the
/// transaction left there, or for an interrupted one what it found, unless `force`.
pub(crate) fn undo(
	workspace: &Workspace,
	force: bool,
	sink: &dyn EventSink,
) -> Result<Option<Transaction>, HistoryError> {
	let _busy = Busy::acquire(workspace)?;
	let (store, history) = (ObjectStore::new(workspace), History::new(workspace));
	let Some(transaction) = history.last()? else {
		return Ok(None);
	};

	let named = transaction.changes.iter().map(|change| &change.path);
	for path in named.chain(&transaction.created_dirs) {
		if is_forbidden_in(workspace, path)? {
			return Err(HistoryError::Forbidden {
				id: transaction.id,
				path: path.clone(),
			});
		}
	}

	let lock_path = Lockfile::path();
	if !force {
		let interrupted = transaction.state == TransactionState::Pending;
		let left = |current: Option<Digest>, before: Option<Digest>, after: Option<Digest>| {
			current == after || (interrupted && current == before)
		};
		let mut diverged = Vec::new();
		for change in &transaction.changes {
			let current = workspace.read(&change.path)?.as_deref().map(Digest::of);
			if !left(current, change.before, change.after) {
				diverged.push(change.path.clone());
			}
		}
		if transaction.lock_after.is_some()
			&& !left(
				workspace.read(&lock_path)?.as_deref().map(Digest::of),
				transaction.lock_before,
				transaction.lock_after,
			) {
			diverged.push(lock_path.clone());
		}
		if !diverged.is_empty() {
			return Err(HistoryError::Diverged {
				id: transaction.id,
				paths: diverged,
			});
		}
	}

	for change in transaction.changes.iter().rev() {
		restore(workspace, &store, &change.path, change.before)?;
		sink.emit(&Event::Removed(&change.path));
	}
	if transaction.lock_after.is_some() {
		restore(workspace, &store, &lock_path, transaction.lock_before)?;
	}
	for dir in transaction.created_dirs.iter().rev() {
		workspace.remove_empty_dir(dir)?;
	}

	history.archive(transaction.id)?;
	sink.emit(&Event::Undone(transaction.id));
	Ok(Some(transaction))
}

fn restore(
	workspace: &Workspace,
	store: &ObjectStore<'_>,
	path: &RelPath,
	digest: Option<Digest>,
) -> Result<(), HistoryError> {
	match digest {
		Some(digest) => {
			let bytes = store.get(&digest)?.ok_or(HistoryError::MissingObject(digest))?;
			workspace.write(path, &bytes)?;
		}
		None => {
			workspace.remove(path)?;
		}
	}
	Ok(())
}

fn rollback(
	workspace: &Workspace,
	store: &ObjectStore<'_>,
	transaction: &Transaction,
	written: usize,
	lock_before: Option<&[u8]>,
) {
	for change in transaction.changes.iter().take(written).rev() {
		let _ = restore(workspace, store, &change.path, change.before);
	}
	let lock_path = Lockfile::path();
	let _ = match (transaction.lock_after.is_some(), lock_before) {
		(false, _) => Ok(()),
		(true, Some(bytes)) => workspace.write(&lock_path, bytes).map(|_| ()),
		(true, None) => workspace.remove(&lock_path).map(|_| ()),
	};
	for dir in transaction.created_dirs.iter().rev() {
		let _ = workspace.remove_empty_dir(dir);
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::sync::Arc;

	use super::*;
	use crate::event::NoopSink;
	use crate::workspace::MemoryFs;

	fn rel(path: &str) -> RelPath {
		RelPath::new(path).unwrap()
	}

	fn workspace() -> (Arc<MemoryFs>, Workspace) {
		let fs = Arc::new(MemoryFs::new());
		(fs.clone(), Workspace::new(fs))
	}

	/// A path, the content undo restores there, and the content the entry says it wrote.
	type Planted<'a> = (&'a str, &'a [u8], Option<&'a [u8]>);

	/// Saves an interrupted journal entry, as a cloned project could carry one.
	fn plant(workspace: &Workspace, changes: &[Planted<'_>], created_dirs: &[&str]) {
		let store = ObjectStore::new(workspace);
		let transaction = Transaction {
			id: 1,
			time: 0,
			summary: "planted".to_owned(),
			state: TransactionState::Pending,
			lock_before: None,
			lock_after: None,
			created_dirs: created_dirs.iter().map(|dir| rel(dir)).collect(),
			changes: changes
				.iter()
				.map(|(path, before, after)| ChangeRecord {
					path: rel(path),
					before: Some(store.put(before).unwrap()),
					after: after.map(|bytes| store.put(bytes).unwrap()),
				})
				.collect(),
		};
		History::new(workspace).save(&transaction).unwrap();
	}

	#[test]
	fn undo_should_refuse_a_journal_entry_that_names_git_internals() {
		for path in [".git/config", ".GIT/config"] {
			let (fs, workspace) = workspace();
			workspace.write(&rel(".git/config"), b"[core]\n").unwrap();
			plant(&workspace, &[(path, b"[core]\n\tfsmonitor = planted\n", None)], &[]);

			let result = undo(&workspace, true, &NoopSink);

			assert!(
				matches!(result, Err(HistoryError::Forbidden { id: 1, .. })),
				"{path}: {result:?}"
			);
			assert_eq!(fs.files()[&rel(".git/config")], b"[core]\n");
		}
	}

	#[test]
	fn undo_should_refuse_a_journal_entry_that_created_folders_in_state() {
		let (_fs, workspace) = workspace();
		plant(&workspace, &[("src/a.ts", b"old", Some(b"new"))], &[".cuilo/objects"]);

		let result = undo(&workspace, false, &NoopSink);

		assert!(matches!(result, Err(HistoryError::Forbidden { .. })), "{result:?}");
	}

	#[test]
	fn undo_should_roll_back_an_interrupted_transaction() {
		let (_fs, workspace) = workspace();
		workspace.write(&rel("src/a.ts"), b"new").unwrap();
		workspace.write(&rel("src/b.ts"), b"untouched").unwrap();
		plant(
			&workspace,
			&[
				("src/a.ts", b"old", Some(b"new")),
				("src/b.ts", b"untouched", Some(b"next")),
			],
			&[],
		);

		undo(&workspace, false, &NoopSink).unwrap();

		assert_eq!(workspace.read(&rel("src/a.ts")).unwrap().as_deref(), Some(&b"old"[..]));
		assert_eq!(
			workspace.read(&rel("src/b.ts")).unwrap().as_deref(),
			Some(&b"untouched"[..])
		);
	}

	#[test]
	fn undo_should_keep_files_an_interrupted_transaction_never_saw() {
		let (fs, workspace) = workspace();
		workspace.write(&rel("src/mine.ts"), b"my own work").unwrap();
		plant(&workspace, &[("src/mine.ts", b"planted", Some(b"as shipped"))], &[]);

		let result = undo(&workspace, false, &NoopSink);

		assert!(matches!(result, Err(HistoryError::Diverged { .. })), "{result:?}");
		assert_eq!(fs.files()[&rel("src/mine.ts")], b"my own work");
	}

	#[test]
	fn apply_then_undo_should_restore_the_tree() {
		let (fs, workspace) = workspace();
		workspace.write(&rel("keep.txt"), b"old").unwrap();
		let before = fs.files();
		let path = rel("src/new.txt");
		let keep = rel("keep.txt");

		apply(
			&workspace,
			"test".to_owned(),
			&[
				Edit {
					path: &path,
					expected: None,
					contents: Some(b"new"),
				},
				Edit {
					path: &keep,
					expected: Some(Digest::of(b"old")),
					contents: Some(b"changed"),
				},
			],
			Some(b"lock"),
			&NoopSink,
		)
		.unwrap();
		undo(&workspace, false, &NoopSink).unwrap();

		let after: BTreeMap<_, _> = fs
			.files()
			.into_iter()
			.filter(|(path, _)| !path.as_str().starts_with(".cuilo"))
			.collect();
		assert_eq!(after, before);
	}

	#[test]
	fn undo_should_restore_a_removed_file() {
		let (_fs, workspace) = workspace();
		let path = rel("src/old.txt");
		workspace.write(&path, b"old").unwrap();

		apply(
			&workspace,
			"test".to_owned(),
			&[Edit {
				path: &path,
				expected: Some(Digest::of(b"old")),
				contents: None,
			}],
			None,
			&NoopSink,
		)
		.unwrap();
		let removed = workspace.read(&path).unwrap();
		undo(&workspace, false, &NoopSink).unwrap();

		assert_eq!(removed, None);
		assert_eq!(workspace.read(&path).unwrap().as_deref(), Some(&b"old"[..]));
	}

	#[test]
	fn undo_should_ignore_a_lockfile_the_transaction_did_not_write() {
		let (_fs, workspace) = workspace();
		let path = rel("packs/local/pack.toml");
		apply(&workspace, "first".to_owned(), &[], Some(b"lock v1"), &NoopSink).unwrap();
		apply(
			&workspace,
			"capture".to_owned(),
			&[Edit {
				path: &path,
				expected: None,
				contents: Some(b"pack"),
			}],
			None,
			&NoopSink,
		)
		.unwrap();
		workspace.write(&Lockfile::path(), b"lock v2").unwrap();

		undo(&workspace, false, &NoopSink).unwrap();

		assert_eq!(workspace.read(&path).unwrap(), None);
		assert_eq!(
			workspace.read(&Lockfile::path()).unwrap().as_deref(),
			Some(&b"lock v2"[..])
		);
	}

	#[test]
	fn apply_should_refuse_files_changed_since_planning() {
		let (_fs, workspace) = workspace();
		let path = rel("a.txt");
		workspace.write(&path, b"edited").unwrap();

		let result = apply(
			&workspace,
			"test".to_owned(),
			&[Edit {
				path: &path,
				expected: None,
				contents: Some(b"x"),
			}],
			None,
			&NoopSink,
		);

		assert!(matches!(result, Err(HistoryError::Changed(_))));
	}

	#[test]
	fn apply_should_refuse_while_busy() {
		let (_fs, workspace) = workspace();
		let _busy = Busy::acquire(&workspace).unwrap();

		assert!(matches!(
			apply(&workspace, String::new(), &[], None, &NoopSink),
			Err(HistoryError::Busy)
		));
	}
}
