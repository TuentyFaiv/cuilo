//! Pack sources for `cuilo`: official packs, folders, GitHub tags, HTTPS tarballs and git repositories,
//! verified against the lockfile hash and the user's trusted minisign keys.
//!
//! Network sources need the default `remote` feature; without it they load only from the cache.

mod error;
mod format;
mod resolver;
mod signing;
mod spec;
mod store;
pub mod transport;

pub use error::SourceError;
pub use resolver::Resolver;
pub use signing::{Trust, signer_id};
pub use spec::{PackSpec, SpecError};
pub use store::{Cache, Library};
pub use transport::Credentials;
