//! One module per first-level command family, plus what more than one of them owes.
//!
//! A command parses nothing and writes nothing itself: it receives its target and an
//! [`crate::output::Output`], asks a core capability, and returns an
//! [`crate::exit::Exit`]. It decides no Package rule — where a needed projection does
//! not exist in the core, it is added to the core and said so, never reimplemented
//! here.
//!
//! **Ordering is the rule with a pin on it.** Every ordering a Package read presents is
//! the core's, and a second place deciding one would put the CLI and the derived Package
//! Index — validated against that same rule — in disagreement about one Package.
//! `tests/package_lists_and_values.rs` asserts a listing's sequence against the *core's
//! own* sequence over the same Package, which is what holds it. A family added later has
//! no such pin until someone writes it, and nothing else will notice.
//!
//! [`shared`] is above the families rather than inside one: the load states a Package can
//! be in are the same set for every command, so the mapping from one of them to a stable
//! failure code and message is decided once. What stays with each family is the
//! *classification* — which of those states a particular command treats as an answer —
//! and each family's own projections.

pub(crate) mod index;
pub(crate) mod init;
pub(crate) mod order;
pub(crate) mod package;
pub(crate) mod repair;
pub(crate) mod repair_suggestion;
pub(crate) mod search;
pub(crate) mod shared;
pub(crate) mod upgrade;
pub(crate) mod validate;
pub(crate) mod vendor;
