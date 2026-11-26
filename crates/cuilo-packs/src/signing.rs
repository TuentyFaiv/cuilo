//! Minisign signatures on packs: which keys to trust, which are revoked, and whether to require one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use cuilo_core::Pack;
use cuilo_core::workspace::SigningConfig;
use minisign_verify::{PublicKey, Signature};

use crate::SourceError;

/// Key ids revoked by the cuilo project, refused for every user.
const REVOKED: &str = include_str!("../revoked-keys.txt");

/// Trusted keys and revocations from the user config.
#[derive(Clone, Debug, Default)]
pub struct Trust {
	/// Each trusted key by key id: its name in the config, its text and the key.
	keys: BTreeMap<String, (String, String, PublicKey)>,
	revoked: BTreeSet<String>,
	require: bool,
}

impl Trust {
	/// Trust from the user config's `signing` table plus the built-in revocation list; two keys with
	/// one key id are refused, since signatures name their key only by that id.
	pub fn new(config: &SigningConfig) -> Result<Self, SourceError> {
		let mut keys = BTreeMap::new();
		for (name, encoded) in &config.keys {
			let invalid = || SourceError::Signature(format!("`signing.keys.{name}` is not a minisign public key"));
			let text = encoded.trim();
			let key = PublicKey::from_base64(text).map_err(|_| invalid())?;
			let id = key_id(text, 32).ok_or_else(invalid)?;
			match keys.get(&id) {
				Some((other, _, trusted)) if *trusted != key => {
					return Err(SourceError::Signature(format!(
						"`signing.keys.{name}` and `signing.keys.{other}` are different keys with the same key id {id}; keep only the one you meant to trust"
					)));
				}
				Some(_) => {}
				None => {
					keys.insert(id, (name.clone(), text.to_owned(), key));
				}
			}
		}
		let mut revoked: BTreeSet<String> = REVOKED
			.lines()
			.map(str::trim)
			.filter(|line| !line.is_empty() && !line.starts_with('#'))
			.map(str::to_ascii_uppercase)
			.collect();
		revoked.extend(config.revoked.iter().map(|id| id.trim().to_ascii_uppercase()));
		Ok(Self {
			keys,
			revoked,
			require: config.require,
		})
	}

	/// Checks `pack`'s signature, recording a trusted signer; bad or revoked signatures are always
	/// refused, unsigned or untrusted ones only when signatures are required.
	pub fn verify(&self, pack: Pack) -> Result<Pack, SourceError> {
		let name = pack.manifest().name.to_string();
		let Some(text) = pack.signature() else {
			return if self.require {
				Err(SourceError::Signature(format!(
					"pack `{name}` is not signed, and your user config requires signed packs"
				)))
			} else {
				Ok(pack)
			};
		};
		let refuse = |reason: String| SourceError::Signature(format!("pack `{name}`: {reason}"));
		let signature =
			Signature::decode(text).map_err(|_| refuse("`pack.minisig` is not a minisign signature".to_owned()))?;
		let id = signature_key_id(text).ok_or_else(|| refuse("`pack.minisig` is not a minisign signature".to_owned()))?;
		if self.revoked.contains(&id) {
			return Err(refuse(format!("signed by key {id}, which is revoked")));
		}
		let Some((_, text, key)) = self.keys.get(&id) else {
			return if self.require {
				Err(refuse(format!(
					"signed by key {id}, which you have not trusted; add it to `signing.keys` in your user config"
				)))
			} else {
				Ok(pack)
			};
		};
		key
			.verify(pack.signed_message().as_bytes(), &signature, false)
			.map_err(|_| refuse(format!("the signature by key {id} does not match its files")))?;
		Ok(pack.with_signer(text.clone()))
	}
}

/// The key id of `signer`, a public key as [`Trust::verify`] records it, as minisign prints it.
#[must_use]
pub fn signer_id(signer: &str) -> Option<String> {
	key_id(signer, 32)
}

/// The key id in a signature file: the second line holds the algorithm, the id and the signature.
fn signature_key_id(text: &str) -> Option<String> {
	key_id(text.lines().nth(1)?.trim(), 64)
}

/// The key id of a minisign key or signature line, as minisign prints it: 16 uppercase hex digits.
fn key_id(encoded: &str, rest: usize) -> Option<String> {
	let bytes = base64(encoded)?;
	if bytes.len() != 2 + 8 + rest {
		return None;
	}
	Some(
		bytes[2..10]
			.iter()
			.rev()
			.fold(String::with_capacity(16), |mut id, byte| {
				let _ = write!(id, "{byte:02X}");
				id
			}),
	)
}

/// Standard base64 with padding.
fn base64(text: &str) -> Option<Vec<u8>> {
	let value = |c: u8| match c {
		b'A'..=b'Z' => Some(c - b'A'),
		b'a'..=b'z' => Some(c - b'a' + 26),
		b'0'..=b'9' => Some(c - b'0' + 52),
		b'+' => Some(62),
		b'/' => Some(63),
		_ => None,
	};
	let text = text.as_bytes();
	if !text.len().is_multiple_of(4) {
		return None;
	}
	let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
	for chunk in text.chunks(4) {
		let padding = chunk.iter().rev().take_while(|c| **c == b'=').count();
		let mut group = 0_u32;
		for (index, c) in chunk.iter().enumerate() {
			let digit = if index >= 4 - padding { 0 } else { value(*c)? };
			group = (group << 6) | u32::from(digit);
		}
		let decoded = group.to_be_bytes();
		bytes.extend_from_slice(&decoded[1..4 - padding]);
	}
	Some(bytes)
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;

	pub(crate) const TRUSTED_KEY: &str = "RWRoNVPtXUMWT93TXAEzSgMQZZO8ItvIHAOjovCmL8xjnEh0Jbz/QIcL";
	pub(crate) const TRUSTED_SIG: &str = "untrusted comment: signature from minisign secret key\nRURoNVPtXUMWTwf60Z+FqIU8Kg2A247WnbQ67fKsJVTbt16ZSzT/wI2BqVqfgdY78u6apmBmwIjqJO+LXdRyGcjQFc/Kw4AV6wc=\ntrusted comment: cuilo test\n9sVbAN6oo2zXPKxKO5Vh31KImaRHODicdPG7GoGTynXPW8z6Wg60l+1Y0VMWMyNn+YpJNm+clfIx2W0K/wbQBQ==\n";
	/// The same key id as [`TRUSTED_KEY`], on another key.
	const SAME_ID_KEY: &str = "RWRoNVPtXUMWT9wCGHLqD4K3gkD+SjXlwrRy/T5OvDi5hxeTdxJ/6HVN";
	/// A signature of the same pack by the key behind [`SAME_ID_KEY`], labelled with that key id.
	const SAME_ID_SIG: &str = "untrusted comment: signature from minisign secret key\nRURoNVPtXUMWTyT4zc0VLLr16UnsUh0xb23AnMRM8fy0AJXnWnBTHqZp7a+J2uQ5EVQxGS3E53TFnW1SoDXrBENGcZIJm0t1iA8=\ntrusted comment: cuilo test\nkJWxGryoSrLgF8OObUm8hp1aeC4Sa3UbJidVhVcfFHw9niitQcyvLf+z3GD6501mfpib0qg2Evn1cAx/C/OfCQ==\n";
	pub(crate) const OTHER_SIG: &str = "untrusted comment: signature from rsign secret key\nRUS2bhPjKGEn5PvKDpUp7vihcsdInstThp3QgUjD7oB4AdjlFQfiohxVBcJ3RRaW/NyFsrhA5s6wllZ98YX69VprmJVONuLT2Qc=\ntrusted comment: cuilo test\nfaWk6i4LV8dwOqjxPvXhe/ELf2LWFPBxWDpeItmiTVbQYn7FKaQkDBNAmWMwBzKgC1EvVhVCMvQ4SarWIAIhDg==\n";

	const PACK: &str = "schema_version = 1\nname = \"acme-ui\"\nversion = \"1.0.0\"\n";
	const PIECE: &str = "schema_version = 1\nname = \"atom\"\ntarget = \"src\"\n";

	/// The pack the test signatures were made for, with `atom` as its one template and `signature`.
	fn pack(atom: &str, signature: Option<&str>) -> Pack {
		let mut files = BTreeMap::from([
			("pack.toml".to_owned(), PACK.as_bytes().to_vec()),
			("pieces/atom/piece.toml".to_owned(), PIECE.as_bytes().to_vec()),
			("pieces/atom/files/atom.ts".to_owned(), atom.as_bytes().to_vec()),
		]);
		if let Some(signature) = signature {
			files.insert("pack.minisig".to_owned(), signature.as_bytes().to_vec());
		}
		Pack::from_files(files).unwrap()
	}

	fn trust(require: bool, revoked: &[&str]) -> Trust {
		Trust::new(&SigningConfig {
			require,
			keys: BTreeMap::from([("acme".to_owned(), TRUSTED_KEY.to_owned())]),
			revoked: revoked.iter().map(|id| (*id).to_owned()).collect(),
		})
		.unwrap()
	}

	fn trusted_id() -> String {
		key_id(TRUSTED_KEY, 32).unwrap()
	}

	#[test]
	fn verify_should_record_the_whole_key_of_a_valid_signature() {
		let pack = trust(true, &[]).verify(pack("x\n", Some(TRUSTED_SIG))).unwrap();

		assert_eq!(pack.signer(), Some(TRUSTED_KEY));
		assert_eq!(signer_id(TRUSTED_KEY).as_deref(), Some("4F16435DED533568"));
		assert_eq!(trusted_id(), "4F16435DED533568");
	}

	#[test]
	fn two_keys_with_one_key_id_should_be_refused() {
		let keys = |second: &str| SigningConfig {
			require: false,
			keys: BTreeMap::from([
				("acme".to_owned(), TRUSTED_KEY.to_owned()),
				("other".to_owned(), second.to_owned()),
			]),
			revoked: Vec::new(),
		};

		let error = Trust::new(&keys(SAME_ID_KEY)).unwrap_err();

		assert!(error.to_string().contains("same key id 4F16435DED533568"), "{error}");
		assert!(Trust::new(&keys(TRUSTED_KEY)).is_ok());
	}

	#[test]
	fn a_key_that_borrows_a_key_id_should_be_recorded_as_itself() {
		let other = Trust::new(&SigningConfig {
			require: true,
			keys: BTreeMap::from([("other".to_owned(), SAME_ID_KEY.to_owned())]),
			revoked: Vec::new(),
		})
		.unwrap();

		let pack = other.verify(pack("x\n", Some(SAME_ID_SIG))).unwrap();

		assert_eq!(signer_id(SAME_ID_KEY), signer_id(TRUSTED_KEY));
		assert_eq!(pack.signer(), Some(SAME_ID_KEY));
	}

	#[test]
	fn the_signature_should_not_change_the_digest() {
		assert_eq!(pack("x\n", Some(TRUSTED_SIG)).digest(), pack("x\n", None).digest());
	}

	#[test]
	fn verify_should_refuse_a_signature_for_other_files() {
		let error = trust(false, &[])
			.verify(pack("tampered\n", Some(TRUSTED_SIG)))
			.unwrap_err();

		assert!(error.to_string().contains("does not match"), "{error}");
	}

	#[test]
	fn verify_should_refuse_a_revoked_key_even_with_a_valid_signature() {
		let error = trust(false, &[&trusted_id().to_ascii_lowercase()])
			.verify(pack("x\n", Some(TRUSTED_SIG)))
			.unwrap_err();

		assert!(error.to_string().contains("revoked"), "{error}");
	}

	#[test]
	fn unsigned_and_untrusted_packs_should_load_unless_signatures_are_required() {
		let relaxed = trust(false, &[]);
		let strict = trust(true, &[]);

		assert_eq!(relaxed.verify(pack("x\n", None)).unwrap().signer(), None);
		assert_eq!(relaxed.verify(pack("x\n", Some(OTHER_SIG))).unwrap().signer(), None);
		assert!(strict.verify(pack("x\n", None)).is_err());
		assert!(
			strict
				.verify(pack("x\n", Some(OTHER_SIG)))
				.unwrap_err()
				.to_string()
				.contains("not trusted")
		);
	}

	#[test]
	fn verify_should_refuse_garbage_signatures() {
		assert!(trust(false, &[]).verify(pack("x\n", Some("not a signature"))).is_err());
	}

	#[test]
	fn trust_should_reject_invalid_keys() {
		let config = SigningConfig {
			keys: BTreeMap::from([("bad".to_owned(), "RWQnope".to_owned())]),
			..SigningConfig::default()
		};

		assert!(matches!(Trust::new(&config), Err(SourceError::Signature(_))));
	}

	#[test]
	fn base64_should_decode_with_and_without_padding() {
		assert_eq!(base64("aGk=").unwrap(), b"hi");
		assert_eq!(base64("aGV5").unwrap(), b"hey");
		assert_eq!(base64("aA=="), Some(b"h".to_vec()));
		assert_eq!(base64("a*=="), None);
	}
}
