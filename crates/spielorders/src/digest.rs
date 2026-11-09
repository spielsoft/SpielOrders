//! Naming exactly which bytes were observed, without carrying them.
//!
//! Evidence has to say *which* content it saw, because "a file is present here" is
//! true of every candidate that name has ever held. Carrying the bytes themselves
//! would say it, but a diagnostic is a report: arbitrary user content does not belong
//! inside one, and the report is built whether or not anything ever reads that field.
//!
//! A digest says the same thing in a bounded amount of space. This is the one place
//! that turns bytes into that evidence, so the diagnostic vocabulary and the repair
//! handles derived from it cannot disagree about what "the same content" means.

use sha2::{Digest, Sha256};
use std::fmt::Write;

/// A stable digest of exact bytes: evidence of *which* content was observed.
///
/// Distinct content produces a distinct digest, so comparing two of these answers
/// "is this still the thing I looked at?" without either side holding the content.
/// SHA-256 rather than a cheaper hash because the answer authorizes a write: a
/// collision here is a repair applied to content the user never confirmed.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContentDigest(String);

impl ContentDigest {
    /// The SHA-256 of exactly these bytes, so it agrees with any other tool that
    /// digests the same file.
    #[must_use]
    pub(crate) fn of(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex_digest(hasher)
    }

    /// The digest as lowercase hexadecimal.
    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContentDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Accumulates one digest over several separate parts.
///
/// Each part is length-prefixed, which [`ContentDigest::of`] has no need to do because
/// it digests one whole thing. Without the prefix, `["ab", "c"]` and `["a", "bc"]` are
/// the same byte stream and so the same digest, which would let two different sets of
/// facts claim to be the same evidence — the exact confusion this seam exists to
/// prevent. The length is fixed-width, so the parts stay uniquely decodable.
pub(crate) struct DigestParts {
    hasher: Sha256,
}

impl DigestParts {
    pub(crate) fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    pub(crate) fn part(&mut self, bytes: &[u8]) -> &mut Self {
        self.hasher.update((bytes.len() as u64).to_be_bytes());
        self.hasher.update(bytes);
        self
    }

    pub(crate) fn finish(self) -> ContentDigest {
        hex_digest(self.hasher)
    }
}

fn hex_digest(hasher: Sha256) -> ContentDigest {
    let mut text = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(text, "{byte:02x}");
    }
    ContentDigest(text)
}
