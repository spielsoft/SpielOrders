//! The one interface every caller of a SpielOrders Package uses: named functions over the
//! C ABI, documented by the one header generated from this crate.
//!
//! Effective conventions live in that header's preamble (`cbindgen.toml`), which is the
//! documentation a caller reads. A Rust caller calls these same functions through their
//! safe signatures.
//!
//! Decision sources: Effect and adapter boundaries, Interface rules.

mod batch;
mod call;
mod constants;
mod diagnosis;
mod draft;
mod entries;
mod library;
mod list;
mod material;
mod package;
mod plan;
mod records;
mod text;

pub use batch::*;
pub use call::{spielorders_cancel_new, spielorders_cancel_release, spielorders_cancel_request};
pub use constants::*;
pub use diagnosis::*;
pub use draft::*;
pub use entries::*;
pub use library::*;
pub use list::*;
pub use material::*;
pub use package::*;
pub use plan::*;
pub use records::*;
pub use text::{OptionalPathBytes, OptionalText, PathBytes, Text};

/// What a fallible function returns: one of the `SPIELORDERS_RESULT_*` constants.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpielordersResult(pub i32);

/// A member of one of the `SPIELORDERS_*` constant families, or -1 for none.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SpielordersConstant(pub i32);

impl SpielordersConstant {
    /// What an accessor answers for none, or for misuse.
    pub(crate) const NONE: Self = Self(-1);
}
