//! Why a pack could not be loaded from its source.

use std::path::PathBuf;

use cuilo_core::model::Digest;
use cuilo_core::pack::PackError;

use crate::format::ArchiveError;
use crate::spec::SpecError;
use crate::transport::FetchError;

/// Why a pack could not be loaded from its source.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SourceError {
	/// The source string is invalid.
	#[error(transparent)]
	Spec(#[from] SpecError),
	/// No pack where the source points.
	#[error("{0}")]
	NotFound(String),
	/// `@namespace/…` without that namespace in the project config.
	#[error("namespace `{0}` is not configured; add it to `sources` in the project config")]
	UnknownNamespace(String),
	/// Downloading failed.
	#[error(transparent)]
	Fetch(#[from] FetchError),
	/// The archive is invalid.
	#[error(transparent)]
	Archive(#[from] ArchiveError),
	/// The pack is invalid.
	#[error(transparent)]
	Pack(#[from] PackError),
	/// The user config's credentials for a host are invalid.
	#[error("{0}")]
	Auth(String),
	/// A registry item cannot be used.
	#[error("registry item: {0}")]
	Registry(String),
	/// The signature is invalid, revoked, untrusted or missing while required.
	#[error("{0}")]
	Signature(String),
	/// A pack from a source other than cuilo itself uses the name of an official pack.
	#[error(
		"`{spec}` is a pack named `{name}`, the name of a pack built into cuilo; packs from other sources need another name"
	)]
	OfficialName {
		/// The source.
		spec: String,
		/// The name it uses.
		name: String,
	},
	/// A lockfile record's source now holds another pack than the record names.
	#[error("`{spec}` holds pack `{actual}`, but the lockfile records `{expected}` from it")]
	OtherPack {
		/// The source.
		spec: String,
		/// Name and version in the lockfile.
		expected: String,
		/// Name and version of the pack found there.
		actual: String,
	},
	/// The pack's files differ from the ones the lockfile pinned.
	#[error("`{spec}` changed: the lockfile pinned {expected}, the source now has {actual}")]
	Changed {
		/// The source.
		spec: String,
		/// Hash in the lockfile.
		expected: Digest,
		/// Hash of what the source serves now.
		actual: Digest,
	},
	/// Reading or writing the cache failed.
	#[error("pack cache `{path}`: {source}")]
	Cache {
		/// File or folder involved.
		path: PathBuf,
		/// The cause.
		source: std::io::Error,
	},
}
