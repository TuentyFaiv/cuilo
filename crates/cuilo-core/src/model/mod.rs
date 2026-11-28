//! Base types every layer shares: paths, globs, digests, values and manifests.

mod case;
mod glob;
mod hash;
mod manifest;
mod path;
mod stack;
mod value;

pub(crate) use case::{Case, words};
pub use glob::{Glob, GlobError};
pub use hash::{Digest, ParseDigestError};
pub use manifest::{
	Anchors, Dependency, Ecosystem, Fact, FileRule, Files, Input, Locale, LocaleError, Migration, Name, NameError,
	PackKind, PackManifest, PackTranslation, PatchSpec, Permissions, PieceKind, PieceManifest, PieceTranslation,
	Position, Rename, Step, Targets, Variant,
};
pub(crate) use manifest::{SCHEMA_VERSION, or_list, shared_alternative};
pub use path::{PathError, RelPath};
pub use stack::{ImportStyle, Stack};
pub use value::{Value, Values};
