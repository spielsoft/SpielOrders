//! The response envelope every `--json` document carries.
//!
//! One shape for every command. Every other command consumes what is defined here and
//! none of them may redefine it, so the rules are written down rather than left to be
//! inferred from one command's fixtures.
//!
//! **The envelope.** Seven members, always all seven, in this order: `contract`,
//! `contract_version`, `ok`, `context`, `data`, `diagnostics`, `error`. A member that
//! has no value is `null` rather than absent, because "absent optional output facts
//! are `null`" is the contract's own rule and one fixed key set is what lets a
//! consumer — and the cross-command conformance ticket — check a response without
//! knowing which command produced it.
//!
//! **`ok` and the exit status are one fact.** `ok: true` if and only if the process
//! exits `0`; `ok: false` if and only if it exits `1` or `2`, and then `data` is
//! `null` and `error` is not. [`Response::answered`] is the only constructor that can
//! produce `ok: true` and [`Response::failed`] the only one that can produce
//! `ok: false`, and every process-level test asserts the document and the status
//! together so the pair cannot drift.
//!
//! **Paths.** `context` carries roots as plain absolute strings: a root's own
//! Package-relative spelling is `.` and says nothing. Every location *inside* a
//! Package is a [`LocationProjection`] carrying the absolute path and the relevant
//! Package-relative one, which is how "absolute paths plus relevant Package-relative
//! paths" is satisfied structurally instead of command by command. A path a JSON
//! document cannot carry — one that is not valid UTF-8 — is an operational failure
//! under `--json`; see [`NotRepresentable`].
//!
//! **A failure's paths are the one exception, and they say so.** Resolving a
//! caller-supplied path is the one operation that can fail before there is anything
//! absolute to name, so a failure carries [`FailedPathProjection`] rather than
//! [`LocationProjection`]: the path is always present, and `spelling` says whether this
//! application resolved it or is quoting the caller's own text back. The distinction is
//! the core's own — `FailedPath` there, projected here — and it is in the contract
//! because flattening it would put a relative path under a key named `absolute_path`,
//! which is the rule above broken silently. What a failure must never do is name no path
//! at all while the human message names one: human output is not a parsing contract, so
//! a fact only it carries is a fact the contract does not have.
//!
//! **`diagnostics`.** Present, possibly empty, and about the subject in `data`. It is
//! not a place to forward every observation the underlying Package load happened to
//! make: reporting Package health is `package show` and `validate`, and another command
//! duplicating that would give one fact two contracts.
//!
//! **Case.** `snake_case` keys, `snake_case` enum values, and stable diagnostic and
//! error codes. Every enum value here is the core's own `as_str` spelling wherever the
//! core has one, so a code cannot mean one thing in the library and another in a
//! response.
//!
//! Decision sources: Define the Rust CLI capability contract; Define durable metadata
//! schemas and version evolution.

use crate::contract::ResponseContract;
use serde::Serialize;
use spielorders_core::{
    mutation::RejectedAssignmentReason,
    operations::FailedPath,
    order_edits::ItemPositionRole,
    readme::{
        ConfigurationPathKind, ConfigurationPathReason, Diagnostic, DiagnosticEvidence,
        EvidenceScope, Location,
    },
    vendors::ContactPositionRole,
};
use std::path::Path;

/// One location list, as a diagnostic's related and conflicting locations report them.
fn locations(locations: &[Location]) -> Result<Vec<LocationProjection>, NotRepresentable> {
    locations.iter().map(LocationProjection::of).collect()
}

/// A path this response contract cannot carry.
///
/// JSON strings are UTF-8, so a filesystem path that is not is a fact `--json` has no
/// spelling for. Reporting it as a typed operational failure is the honest answer;
/// substituting replacement characters would emit a path that does not open. Human
/// output is unaffected — it writes the path's own bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NotRepresentable;

fn path_text(path: &Path) -> Result<String, NotRepresentable> {
    path.to_str().map(str::to_owned).ok_or(NotRepresentable)
}

/// One location, as every response reports one.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct LocationProjection {
    absolute_path: String,
    /// `null` where the location has no Package-relative spelling — an entry outside
    /// any Package, or one whose name this application cannot express.
    package_relative_path: Option<String>,
}

impl LocationProjection {
    pub(crate) fn of(location: &Location) -> Result<Self, NotRepresentable> {
        Ok(Self {
            absolute_path: path_text(location.absolute_path())?,
            package_relative_path: location.package_relative_path().map(str::to_owned),
        })
    }

    /// One absolute path that is genuinely outside every Package.
    ///
    /// [`Self::of`]'s counterpart for the one path a response carries that no Package
    /// locates: where the platform Trash put a disposed-of Order Directory. `null` is
    /// what the field means there rather than a spelling this adapter declined to
    /// compute — a trashed directory is not inside the Package any more, and this
    /// application did not choose where it went.
    ///
    /// Every path a Package *does* locate reaches a response through [`Self::of`], from
    /// a `Location` the core supplied. The normalized Package-relative grammar is the
    /// core's, and an adapter computing it would be a second place deciding what a
    /// Package's layout is.
    pub(crate) fn outside_a_package(path: &Path) -> Result<Self, NotRepresentable> {
        Ok(Self {
            absolute_path: path_text(path)?,
            package_relative_path: None,
        })
    }
}

/// Where a failure's path came from.
///
/// [`Self::Resolved`] means this application resolved the path, so it is absolute.
/// [`Self::AsGiven`] means it is the caller's own text and this application has no
/// absolute spelling for it — either because the resolution itself is what failed, or
/// because nothing resolved it before the failure was reported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathSpelling {
    Resolved,
    AsGiven,
}

impl PathSpelling {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::AsGiven => "as_given",
        }
    }
}

/// One path a failure is about, in the best spelling this application has for it.
///
/// `path` is always present: naming the path is the least a failure about one can do,
/// and a consumer that only wants to show the user which path failed reads this one key
/// in every case. `spelling` is what it must read before treating that path as absolute.
#[derive(Debug, Serialize)]
pub(crate) struct FailedPathProjection {
    spelling: &'static str,
    path: String,
    /// `null` for every path with no Package-relative spelling, which includes every
    /// `as_given` path: a path this application did not resolve is not known to be
    /// inside any Package.
    package_relative_path: Option<String>,
}

impl FailedPathProjection {
    /// Projects the core's own decision about how a path can be named.
    ///
    /// Matching on the core's enum rather than inspecting the path is the point: which
    /// spelling a failure has is the core's answer, and an adapter that re-derived it
    /// from `Path::is_absolute` would be a second place deciding it. A new variant in
    /// the core is a compile error here rather than a mislabelled path.
    pub(crate) fn of(path: &FailedPath) -> Result<Self, NotRepresentable> {
        Ok(match path {
            // Built from the ordinary location projection rather than beside it, so a
            // resolved failure path and a location anywhere else in a response cannot
            // come to be spelled differently.
            FailedPath::Located(location) => {
                let LocationProjection {
                    absolute_path,
                    package_relative_path,
                } = LocationProjection::of(location)?;
                Self {
                    spelling: PathSpelling::Resolved.as_str(),
                    path: absolute_path,
                    package_relative_path,
                }
            }
            FailedPath::Unlocated(given) => Self {
                spelling: PathSpelling::AsGiven.as_str(),
                path: path_text(given)?,
                package_relative_path: None,
            },
        })
    }
}

/// The command's targeting context.
///
/// **`package_root` names the Root the command was about**, and the answer does not
/// depend on the outcome — which is the whole question `context` answers. It is `null`
/// only where there is no such Root to name, and the failure's own path under
/// [`Failure::paths`] is what names the subject then.
///
/// **A failure is not automatically rootless.** A command that established its Root and
/// then could not read past it names that Root here.
///
/// For a command that *reads* a Package, the Root it was about is the one the load
/// established: an unmanaged directory is a resolved path and not a Root, so a read of
/// one names none — the Package is precisely what is absent — and so does a load that
/// resolved one and exposes none.
///
/// For `init` the subject is a directory that must *not* be a Root yet, and the Root it
/// was about is the one the plan resolved as the prospective one. It is named in every
/// outcome: a committed initialization, a `--dry-run` that established nothing, and a
/// refusal. **The narrower rule this paragraph replaces was written when every command
/// was a read**, where "the Root the load established" and "the Root the command was
/// about" are the same thing; for the first mutation they are not, and a `--dry-run`
/// under the narrower rule would have named the directory it was planning nowhere in the
/// document at all. No read's answer changes, because for a read the two readings agree.
///
/// Order commands extend this with their own root; a Package-level command does not
/// carry a member for a root it has no notion of. That is [`OrderContext`], and it is a
/// separate type rather than an optional member here for exactly that reason: an
/// `order_root: null` in every Package-level response would be a member the
/// Package-level contracts have no notion of, and adding it would move their recorded
/// bytes.
#[derive(Debug, Serialize)]
pub(crate) struct Context {
    package_root: Option<String>,
}

impl Context {
    pub(crate) fn untargeted() -> Self {
        Self { package_root: None }
    }

    pub(crate) fn rooted_at(root: &Path) -> Result<Self, NotRepresentable> {
        Ok(Self {
            package_root: Some(path_text(root)?),
        })
    }
}

/// An `order` command's targeting context: the Order Directory, and the Package it
/// belongs to.
///
/// **Two roots because an `order` command has two**, and they are established by
/// different routes. `--root` names the Order Directory exactly; the Package is reached
/// from there through the Order record's own `package_root` locator, and `--uuid` runs
/// the same pair the other way round — the Package is the current directory and the
/// Order Directory is what the identity resolves to. Reporting only one of them would
/// leave a consumer unable to say which Package the answer came from, which is the whole
/// question a locator answers.
///
/// Each is `null` exactly when that root was not established, by the rule [`Context`]
/// follows: an Order Directory whose locator reached no Package names no
/// `package_root`, and a `--root` the filesystem declined to resolve names no
/// `order_root` — the failure's own path carries the caller's text in that case.
///
/// **Each root takes its own spelling.** There used to be one constructor per
/// combination, each taking both spellings at once and failing if either had none, and
/// the caller answered that failure with the untargeted context — so one root with no
/// JSON spelling reported both as `null` and discarded a Root the command had
/// established. A root with no spelling has genuinely none to name, which is what `null`
/// already says; the root beside it is unaffected.
#[derive(Debug, Serialize)]
pub(crate) struct OrderContext {
    package_root: Option<String>,
    order_root: Option<String>,
}

impl OrderContext {
    /// The two roots, each named wherever the command established one.
    pub(crate) fn of(package_root: Option<&Path>, order_root: Option<&Path>) -> Self {
        Self {
            package_root: package_root.and_then(Path::to_str).map(str::to_owned),
            order_root: order_root.and_then(Path::to_str).map(str::to_owned),
        }
    }

    /// Both roots, for the answers that established both.
    pub(crate) fn rooted_at(package_root: &Path, order_root: &Path) -> Self {
        Self::of(Some(package_root), Some(order_root))
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticProjection {
    code: &'static str,
    severity: &'static str,
    scope: &'static str,
    primary_location: LocationProjection,
    related_locations: Vec<LocationProjection>,
    /// The evidence for this `code`. Its members depend on the code, which is the
    /// discriminator the core itself uses — see [`evidence`].
    evidence: Evidence,
}

impl DiagnosticProjection {
    pub(crate) fn of(diagnostic: &Diagnostic) -> Result<Self, NotRepresentable> {
        Ok(Self {
            code: diagnostic.code().as_str(),
            severity: diagnostic.severity().as_str(),
            scope: diagnostic.scope().as_str(),
            primary_location: LocationProjection::of(diagnostic.primary_location())?,
            related_locations: locations(diagnostic.related_locations())?,
            evidence: evidence(diagnostic.evidence())?,
        })
    }
}

/// The detail beside a diagnostic's `code`.
///
/// The `code` is the discriminator and the stable thing to branch on; these members are
/// what the code is *about*. Three of the core's variants — a disallowed Orders
/// Directory entry, one whose name has no Package-relative spelling, and unexpected
/// reserved content — carry an observed entry kind and nothing else, so they share
/// [`Self::ObservedEntryKind`]: the `code` already tells them apart, and three identical
/// shapes would be three names for one thing.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum Evidence {
    ManagedArtifact {
        artifact: &'static str,
        observed_kind: Option<&'static str>,
        operation: Option<&'static str>,
        field: Option<String>,
        observed_version: Option<u64>,
        expected_version: Option<u64>,
    },
    ConfigurationPathInvalid {
        /// *Which configured key* is invalid, not a filesystem path. Named
        /// `configuration_path` rather than the core's own `path` because `path` means a
        /// filesystem path everywhere else in this envelope.
        configuration_path: &'static str,
        configured_value: String,
        reason: &'static str,
    },
    ConfigurationPathsOverlap {
        orders_path: String,
        removed_path: String,
    },
    ConfiguredOrdersDirectory {
        configured_value: String,
        /// `null` for the name-unusable case, whose whole condition is that the
        /// resolved name has no Package-relative spelling — the `code` is the reason.
        reason: Option<&'static str>,
    },
    ObservedEntryKind {
        observed_kind: &'static str,
    },
    StagingArtifactPresent {
        destination_valid: Option<bool>,
        candidate_digest: Option<String>,
    },
    VendorIdDuplicate {
        vendor_id: String,
    },
    VendorNameDuplicate {
        vendor_ids: Vec<String>,
        observed_names: Vec<String>,
    },
    VendorReferenceUnresolved {
        order_id: String,
        vendor_id: String,
    },
    OrderSemanticInvalid {
        order_id: String,
        field: String,
        reason: &'static str,
    },
    OrderPackageLocatorMismatch {
        order_id: String,
        /// `null` where the record stores no locator text to quote back — an absent
        /// member, or one that is not text. The `reason` is what says which.
        package_root: Option<String>,
        reason: &'static str,
    },
    OrderIdDuplicate {
        order_id: String,
        conflict_locations: Vec<LocationProjection>,
    },
    ReadmeUnavailable {
        document_kind: &'static str,
        reason: &'static str,
    },
}

/// The evidence projection for one core diagnostic.
///
/// Every variant is listed rather than covered by a wildcard, so adding a diagnostic to
/// the core is a compile error here rather than silently unreported detail. Every enum
/// value is the core's own `as_str` spelling, so a reason cannot mean one thing in the
/// library and another in a response.
fn evidence(evidence: &DiagnosticEvidence) -> Result<Evidence, NotRepresentable> {
    Ok(match evidence {
        DiagnosticEvidence::ManagedArtifact {
            artifact,
            observed_kind,
            operation,
            field,
            observed_version,
            expected_version,
        } => Evidence::ManagedArtifact {
            artifact: artifact.as_str(),
            observed_kind: observed_kind.map(|kind| kind.as_str()),
            operation: operation.map(|operation| operation.as_str()),
            field: field.clone(),
            observed_version: *observed_version,
            expected_version: *expected_version,
        },
        DiagnosticEvidence::ConfigurationPathInvalid {
            path,
            configured_value,
            reason,
        } => Evidence::ConfigurationPathInvalid {
            configuration_path: path.as_str(),
            configured_value: configured_value.clone(),
            reason: reason.as_str(),
        },
        DiagnosticEvidence::ConfigurationPathsOverlap {
            orders_path,
            removed_path,
        } => Evidence::ConfigurationPathsOverlap {
            orders_path: orders_path.clone(),
            removed_path: removed_path.clone(),
        },
        DiagnosticEvidence::ConfiguredOrdersDirectoryUnavailable {
            configured_value,
            reason,
        } => Evidence::ConfiguredOrdersDirectory {
            configured_value: configured_value.clone(),
            reason: Some(reason.as_str()),
        },
        DiagnosticEvidence::ConfiguredOrdersDirectoryNameUnusable { configured_value } => {
            Evidence::ConfiguredOrdersDirectory {
                configured_value: configured_value.clone(),
                reason: None,
            }
        }
        DiagnosticEvidence::OrdersDirectoryEntryDisallowed { observed_kind }
        | DiagnosticEvidence::OrdersDirectoryEntryNameUnusable { observed_kind }
        | DiagnosticEvidence::ReservedContentUnexpected { observed_kind } => {
            Evidence::ObservedEntryKind {
                observed_kind: observed_kind.as_str(),
            }
        }
        DiagnosticEvidence::StagingArtifactPresent {
            destination_valid,
            candidate_digest,
        } => Evidence::StagingArtifactPresent {
            destination_valid: *destination_valid,
            candidate_digest: candidate_digest
                .as_ref()
                .map(|digest| digest.as_str().to_owned()),
        },
        DiagnosticEvidence::VendorIdDuplicate { vendor_id } => Evidence::VendorIdDuplicate {
            vendor_id: vendor_id.as_str().to_owned(),
        },
        DiagnosticEvidence::VendorNameDuplicate {
            vendor_ids,
            observed_names,
        } => Evidence::VendorNameDuplicate {
            vendor_ids: vendor_ids
                .iter()
                .map(|vendor_id| vendor_id.as_str().to_owned())
                .collect(),
            observed_names: observed_names.clone(),
        },
        DiagnosticEvidence::VendorReferenceUnresolved {
            order_id,
            vendor_id,
        } => Evidence::VendorReferenceUnresolved {
            order_id: order_id.as_str().to_owned(),
            vendor_id: vendor_id.as_str().to_owned(),
        },
        DiagnosticEvidence::OrderSemanticInvalid {
            order_id,
            field,
            reason,
        } => Evidence::OrderSemanticInvalid {
            order_id: order_id.as_str().to_owned(),
            field: field.clone(),
            reason: reason.as_str(),
        },
        DiagnosticEvidence::OrderPackageLocatorMismatch {
            order_id,
            package_root,
            reason,
        } => Evidence::OrderPackageLocatorMismatch {
            order_id: order_id.as_str().to_owned(),
            package_root: package_root.clone(),
            reason: reason.as_str(),
        },
        DiagnosticEvidence::OrderIdDuplicate {
            order_id,
            conflict_locations,
        } => Evidence::OrderIdDuplicate {
            order_id: order_id.as_str().to_owned(),
            conflict_locations: locations(conflict_locations)?,
        },
        DiagnosticEvidence::ReadmeUnavailable {
            document_kind,
            reason,
        } => Evidence::ReadmeUnavailable {
            document_kind: document_kind.as_str(),
            reason: reason.as_str(),
        },
    })
}

/// The scope a failure is about.
///
/// The core's four evidence scopes, plus the one the core never sees: a command line
/// the core is never shown. A usage error has no Package, Order, or document to be
/// about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FailureScope {
    Package,
    Order,
    Document,
    Operation,
    Usage,
}

impl FailureScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::Order => "order",
            Self::Document => "document",
            Self::Operation => "operation",
            Self::Usage => "usage",
        }
    }

    pub(crate) fn of_evidence(scope: EvidenceScope) -> Self {
        match scope {
            EvidenceScope::Package => Self::Package,
            EvidenceScope::Order => Self::Order,
            EvidenceScope::Document => Self::Document,
            EvidenceScope::Operation => Self::Operation,
        }
    }
}

/// Every stable failure code this executable can report.
///
/// Stable means a consumer may branch on it. The human message beside it may be
/// reworded without advancing a contract version; the code may not change without
/// advancing one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FailureCode {
    /// The exact path given is not a SpielOrders Root: no Package metadata directory
    /// stands there. No ancestor was consulted, because `--root` names the root and
    /// never a descendant of one.
    PackageRootUnmanaged,
    /// Package metadata stands there, but the load could not trust it: either it could
    /// not establish the Package's managed paths at all, or what stood under them did
    /// not validate. The response's diagnostics are the diagnosis.
    PackageRootDamaged,
    /// The Package declares a durable metadata version this build does not interpret.
    /// Distinct from damage because the action is different: `upgrade` if the Package is
    /// behind this build, a newer build if it is ahead.
    PackageMetadataUnsupported,
    /// The Package is mid-upgrade or awaiting one, so this build will not read it as
    /// current. Distinct from the code above because the upgrade is known to exist and
    /// resuming it is the action.
    PackageUpgradePending,
    /// The Package could not be read at all.
    PackageUnreadable,
    /// The Package was read completely and is invalid: some finding carries error
    /// severity. The response's diagnostics are the answer.
    ///
    /// **Distinct from `package_root_damaged`**, which is a Package the load could not
    /// trust enough to report its contents at all. This one was validated end to end,
    /// every document and every reserved directory included, and the findings under it
    /// are a diagnosis rather than the reason there is none.
    ///
    /// **Severity alone reaches this code.** A Package whose findings are all warnings is
    /// valid and exits `0`, whatever repairs those warnings offer.
    PackageInvalid,
    /// The selected Order's scope was validated and is invalid, by the same predicate at
    /// the narrower scope. A problem with the Package around it is reported at Package
    /// scope instead, because an Order-scoped answer depends on Package-wide evidence.
    OrderInvalid,
    /// The exact path given is not an Order Directory: no Order record stands under it.
    /// No ancestor was consulted, because `--root` names the Order Directory itself — and
    /// no descendant was either, so a SpielOrders Root handed to an `order` command lands
    /// here rather than being reinterpreted as one of its Orders.
    OrderRootUnmanaged,
    /// The Order record stands there and could not be read as one. Order-local damage,
    /// distinct from `package_root_damaged` because the Package around it is fine.
    OrderRecordDamaged,
    /// The Order record's own technical Package locator is missing or malformed, so this
    /// Order names no Package that can be reached from its Directory at all. Its own code
    /// because the repair is specific: recalculate the locator from where the directory
    /// physically sits.
    OrderLocatorUnusable,
    /// The locator is well formed and does not lead to a Package holding this Order, so
    /// it is stale or names another Package. The same repair, and a different observation
    /// — the value is readable and wrong rather than unreadable.
    OrderLocatorStale,
    /// The Package holds no Order carrying the requested Order ID.
    OrderNotFound,
    /// The Package holds Vendors and none of them carries the requested Vendor Name.
    /// Names resolve by the trimmed, case-insensitive, package-unique comparison, so
    /// this is a Name that matched none of them under that comparison.
    VendorNotFound,
    /// The Package holds no Vendors at all, so no Vendor Name can resolve in it. Its own
    /// code because the action differs from the one above: create a Vendor rather than
    /// check the Name. A Vendor is never deleted, so this is an unpopulated Package.
    PackageHasNoVendors,
    /// At least one canonical `field=value` assignment was refused, and nothing was
    /// written. Every refused assignment of the set travels under
    /// [`Failure::identities`] as [`Identity::rejected_assignment`], because an
    /// assignment set is adjudicated as a whole.
    ///
    /// **One code for every rule an assignment can break**, which is the opposite of the
    /// choice the configuration paths above made — and for the reason that made theirs
    /// right: each of those is one key breaking one rule, so the code can name the rule.
    /// An assignment *set* can be refused for several rules at once, and one `code` cannot
    /// say which field broke which. The rule each field broke is therefore the identity's
    /// own, in the core's own spelling.
    VendorAssignmentRejected,
    /// The creation supplied no Vendor Name, which with the Vendor ID is the only required
    /// Vendor fact. Distinct from a blank one, which is a refused assignment: this command
    /// line assigned nothing to it at all.
    VendorNameRequired,
    /// The Vendor Name is already carried by another Vendor of this Package, under the
    /// trimmed, case-insensitive comparison that makes Names unique. The existing Vendor's
    /// ID travels beside it as an identity.
    ///
    /// **There is no merge**, so this is refused rather than resolved: the correction is
    /// an existing Vendor, a deliberately distinct Name, or hiding the one no longer used.
    /// Spelled apart from the `vendor_name_duplicate` *diagnostic*, which is a Package
    /// whose stored records already name one Vendor twice.
    VendorNameInUse,
    /// The Package or its Vendor record changed between planning this mutation and
    /// performing it, so what would have been written is no longer what was validated.
    /// Nothing was written.
    VendorMutationPreconditionsChanged,
    /// The Vendor record could not be replaced. It holds what it held, and nothing this
    /// attempt staged remains.
    VendorMutationFailed,
    /// The same, and the staging artifact this attempt created could not be removed. Its
    /// own code because the action differs: an interrupted write's leftover stage is a
    /// diagnosed condition, so `spielorders validate` reports it and `repair` offers the
    /// action, where the code above needs only that the command be run again.
    VendorMutationCleanupFailed,
    /// The identity minted for a new Vendor is not a usable Vendor ID. Nothing was
    /// written; the text that was generated is in the message.
    VendorIdentityInvalid,
    /// The identity minted for a new Vendor is already carried by a Vendor of this
    /// Package. Nothing was written.
    VendorIdentityCollision,
    /// The complete Vendor aggregate this mutation assembled did not validate against the
    /// Package it would land in, so nothing was written.
    VendorAggregateInvalid,
    /// A Contact position names no Contact of the addressed collection.
    VendorContactPositionInvalid,
    /// The added Contact was assigned no Name, which is the only required Contact fact.
    /// Distinct from a blank one, which is a refused assignment, and distinct from
    /// `vendor_name_required` because the Vendor is not what has no Name: it resolved,
    /// and the correction is a Name for the Contact rather than one for it.
    VendorContactNameRequired,
    /// At least one canonical `field=value` assignment was refused, and nothing was
    /// written. Every refused assignment travels under [`Failure::identities`] as
    /// [`Identity::rejected_assignment`], for the reason
    /// [`Self::VendorAssignmentRejected`] gives: a set is adjudicated as a whole, so one
    /// `code` cannot say which field broke which rule.
    OrderAssignmentRejected,
    /// A fact this mutation requires was not assigned at all. Distinct from a blank one,
    /// which is a refused assignment. Every missing fact travels beside it as
    /// [`Identity::required_fact`], because a command line that named none of them should
    /// be told about all of them rather than one round trip at a time.
    OrderFactsRequired,
    /// `status` was assigned something that is not an established Order Status. Its own
    /// code because the correction is specific and the set of valid values is closed; the
    /// text that was assigned travels as [`Identity::order_status`].
    OrderStatusInvalid,
    /// `date_ordered` was assigned something that is not a real calendar day written
    /// `YYYY-MM-DD`. The text that was assigned travels as
    /// [`Identity::order_date_ordered`].
    ///
    /// **Its own code beside [`Self::OrderStatusInvalid`]**, because the correction differs:
    /// a Status is one of a closed set a message can list, and a date is a grammar and a
    /// calendar. Both are distinct from [`Self::OrderFactsInvalid`], which is the whole
    /// assembled record and names no field.
    OrderDateOrderedInvalid,
    /// `currency` was assigned something that is not an Order Currency: a three-letter
    /// uppercase code. An empty value clears it, so this is never a command line asking for
    /// no Currency. The text that was assigned travels as [`Identity::order_currency`].
    OrderCurrencyInvalid,
    /// The complete Order record this mutation assembled is not valid for this Package, so
    /// nothing was written.
    ///
    /// **The residual refusal.** Each fact the record constrains is adjudicated on its own
    /// before a candidate is assembled, under
    /// [`Self::OrderStatusInvalid`], [`Self::OrderDateOrderedInvalid`], or
    /// [`Self::OrderCurrencyInvalid`] — so this is what is left when every fact of the set
    /// was individually acceptable and the record they assemble into is not.
    OrderFactsInvalid,
    /// An Item position names no Item of the addressed collection. The counterpart of
    /// [`Self::VendorContactPositionInvalid`] for the Order's own ordered collection, and
    /// the position travels as [`Identity::item_position`].
    OrderItemPositionInvalid,
    /// A fact an Item requires was not assigned at all — its Description, Quantity, or Unit
    /// Price. Distinct from a blank one, which is a refused assignment, and distinct from
    /// [`Self::OrderFactsRequired`] because the Order is not what is missing a fact: it
    /// resolved, and the correction is an assignment for the Item.
    OrderItemFactsRequired,
    /// A genuinely new Reusable Free-Entry Value was proposed without confirmation.
    ///
    /// **No command line reaches this**, because a CLI assignment is itself the explicit
    /// confirmation the decision requires — this adapter hands the core a confirmed set. It
    /// is named because the refusal is the core's and a library caller can reach it, so the
    /// mapping stays total rather than folding an unrelated code over it.
    OrderReusableValueUnconfirmed,
    /// `vendor_id` names no Vendor of this Package. A Vendor is chosen, never created, by
    /// an Order mutation: `add-vendor` is what creates one.
    OrderVendorUnknown,
    /// This platform has no Trash this build can submit a directory to, so the
    /// disposition `--delete` asks for is not available here.
    ///
    /// **Its own code because the answer is specific and the Order is untouched.** There
    /// is no fallback that deletes: `spielorders order remove` without `--delete` is the
    /// disposition this platform does have, and nothing was erased to discover that.
    OrderTrashUnsupported,
    /// The platform Trash refused this directory. Distinct from the code above because
    /// the platform *has* a Trash and would not take it, so the correction is in the
    /// refusal rather than in the command. Nothing was erased.
    OrderTrashSubmissionFailed,
    /// The Order's *stored* Vendor Reference resolves to no Vendor of its Package, so an
    /// operation composed from that Vendor's current Name has none to compose from.
    ///
    /// **Distinct from [`Self::OrderVendorUnknown`]**, which is a command line naming a
    /// Vendor the Package has none of: nothing is wrong with the Package there, and the
    /// correction is a different assignment. This one is an Unresolved Vendor Reference —
    /// damage the Package already carries, diagnosed and explicitly repairable — so the
    /// correction is a repair rather than a different command line.
    OrderVendorUnresolved,
    /// The Order Title projects no usable Order Directory component, so this creation has
    /// no name to give the directory it would make.
    ///
    /// **No command line reaches this today**, because the record's own Title is a
    /// non-blank trimmed string and the projection replaces every character a directory
    /// name cannot carry rather than dropping it — so a Title that validated always
    /// projects at least one character. The arm is written rather than folded into a
    /// neighbour because the core states the refusal and the correction is its own: a
    /// different Title, where a refused *assignment* means a different command line. A
    /// blank Title is that other thing and is refused as an assignment.
    OrderTitleUnusable,
    /// Something already stands where this creation's Order Directory or its staging name
    /// would go. Refused rather than resolved with an invented suffix.
    OrderDirectoryOccupied,
    /// The candidate is not a direct child of this Package's configured Orders Directory.
    /// Adoption adopts a candidate where it stands, so a directory elsewhere is not one of
    /// this Package's Orders and is not moved to become one.
    OrderCandidateNotADirectChild,
    /// The candidate is a hidden entry, which the Package's own discovery rules exclude.
    OrderCandidateHidden,
    /// The candidate is a link. Links are never followed and never adopted, so an
    /// institution-grouped alias view cannot become an Order.
    OrderCandidateLink,
    /// The candidate is not an ordinary directory: a file, or absent, or something else
    /// the Package has no notion of.
    OrderCandidateIneligible,
    /// The candidate's filesystem name has no normalized Package-relative spelling, so
    /// nothing describing this Package could refer to an Order there. It is never indexed
    /// and is left exactly as it is.
    OrderCandidateNameUnusable,
    /// The candidate already holds a readable Order record, so it is already one of this
    /// Package's Orders. Adoption never overwrites one.
    OrderCandidateAlreadyManaged,
    /// The candidate holds reserved Package metadata that is not a readable Order record —
    /// a partial prior adoption, or something else under `.spielorders`. Its own code
    /// because the safe distinction cannot be made here: the candidate is left untouched
    /// for `spielorders validate` and explicit human disposition.
    OrderCandidateReservedStateConflict,
    /// A staging name this adoption would use is occupied, which is an interrupted earlier
    /// write rather than a candidate problem. `spielorders validate` reports it and
    /// `repair` offers the action.
    OrderCandidateStagingOccupied,
    /// The Package or the candidate changed between planning this mutation and performing
    /// it, so what would have been written is no longer what was validated. Nothing was
    /// written.
    OrderMutationPreconditionsChanged,
    /// The mutation failed partway and everything it had created was removed again, so the
    /// Package and the candidate are as they were.
    OrderMutationFailed,
    /// The same, and not everything this attempt created could be removed again. Its own
    /// code because the action differs: the named paths are what remains, and this
    /// application will not guess whether it may delete them.
    OrderMutationCleanupFailed,
    /// The identity minted for a new Order is not a usable Order ID. Nothing was written;
    /// the text that was generated is in the message.
    OrderIdentityInvalid,
    /// The identity minted for a new Order is already carried by an Order of this Package.
    /// Nothing was written and nothing was overwritten.
    OrderIdentityCollision,
    /// A path this initialization had to examine could not be examined, so the
    /// filesystem refused before anything was planned. The named path is the one it
    /// refused about.
    InitializationFilesystemFailed,
    /// The exact path given is not an ordinary directory, so there is nothing there to
    /// make a Package of. It may be a file, a link, or absent — the distinction belongs
    /// to the person looking at it, and none of the three is a directory.
    PackageRootNotADirectory,
    /// The exact path given is already a SpielOrders Root. Initializing it again is
    /// refused rather than merged into it, and it is left exactly as it was.
    PackageRootAlreadyManaged,
    /// A configured managed path is an absolute path. Each must be relative to the Root.
    ///
    /// **The seven codes below are one per configuration path rule**, because the rule
    /// broken is what a caller has to act on and each is a different correction. Their
    /// spellings are `configuration_path_` and the core's own
    /// `ConfigurationPathReason::as_str`, so a rule refused at initialization and the
    /// same rule diagnosed in a loaded Package cannot come to be named differently; the
    /// key that broke it travels beside them as [`Identity::configuration_path`].
    ConfigurationPathAbsolute,
    /// A configured managed path is `.`, which names the Root rather than a place in it.
    ConfigurationPathCurrentDirectory,
    /// A configured managed path leaves the Root through a `..` component.
    ConfigurationPathParentTraversal,
    /// A configured managed path has an empty component, which includes the empty value.
    ConfigurationPathEmptyComponent,
    /// A configured managed path uses this platform's own separator. Managed paths are
    /// `/`-separated so one Package reads the same on every platform.
    ConfigurationPathPlatformSeparator,
    /// A configured managed path occupies the reserved Package metadata subtree.
    ConfigurationPathReservedMetadata,
    /// `removed_path` is not a direct child of the Root.
    ConfigurationPathNotDirectChild,
    /// A configured managed path names the conventional Package README, which is a
    /// document this Package keeps rather than a directory it may hold Orders in.
    ConfigurationPathReservedReadme,
    /// The two configured managed paths are one place, or one holds the other.
    ConfigurationPathsOverlap,
    /// A configured managed path resolves through a link this application will not
    /// follow, so what it names is outside the Package it claims to be inside.
    InitializationDisallowedLink,
    /// Something already stands where a managed artifact had to be created.
    InitializationDestinationOccupied,
    /// The directory changed between the plan and the commit, so what would have been
    /// written is no longer what was validated. Nothing was written.
    PackageRootChanged,
    /// The initialization failed partway and everything it had created was removed
    /// again, so the directory is as it was.
    PackageInitializationFailed,
    /// The initialization failed partway and not everything it created could be removed
    /// again. Its own code because the action differs: the named paths are what remains,
    /// and this application will not guess whether it may delete them.
    PackageInitializationIncomplete,
    /// The derived Package Index was not rebuilt, because no truthful projection exists:
    /// an Order README is present and could not be read, and every index entry records
    /// its Order's README text. The stored index is preserved exactly as it was.
    ///
    /// **Its own code because the correction is outside this application**: the document
    /// has to become readable, and no repair replaces or converts a user-authored one.
    IndexProjectionIncomplete,
    /// The derived Package Index could not be replaced. It holds what it held, and
    /// nothing canonical was touched.
    IndexReplacementFailed,
    /// No currently diagnosed case answers to the handle this command line named. A case
    /// handle is derived from the evidence it was diagnosed from, so a handle from an
    /// older diagnosis stops naming a case as soon as that evidence moves.
    ///
    /// **Its own code because the correction is specific**: diagnose the scope again. A
    /// handle is never resolved against whatever stands at the same place now, which is
    /// the whole reason it is derived rather than minted.
    RepairCaseUnknown,
    /// The named case exists and does not offer the named action. A repair applies only an
    /// action its own diagnosis offered.
    RepairActionUnauthorized,
    /// The named action belongs to the other scope's capability, so this command cannot
    /// apply it. Distinct from the code above because the correction is a different
    /// command rather than a different action.
    RepairActionUnsupported,
    /// A value the named action requires was not assigned, or was assigned blank. Every
    /// missing resolution travels under [`Failure::identities`] as
    /// [`Identity::required_fact`], because a command line that supplied none of them
    /// should be told about all of them rather than one round trip at a time.
    RepairResolutionMissing,
    /// A `field=value` was assigned that the named action does not take. Its own code
    /// because the correction differs from the one above: remove the assignment rather
    /// than supply one.
    RepairResolutionUnexpected,
    /// The replacement or staging candidate is not a complete valid record of its kind, so
    /// nothing was written and nothing was partially salvaged from it.
    RepairCandidateInvalid,
    /// The candidate is a valid record on its own and would still leave this Package
    /// damaged, so nothing was written. Answering one diagnosed condition by creating
    /// another is not a repair.
    RepairCandidateDamagesPackage,
    /// A replacement stages beside its destination and abandoned staging evidence already
    /// occupies that path. The staging case is resolved first.
    RepairStagingEvidencePresent,
    /// The chosen Vendor is not one this Package has. A repair chooses an existing Vendor,
    /// deliberately creates a distinct one, or clears the reference.
    RepairVendorUnknown,
    /// A deliberately created Vendor's Name is already carried by a Vendor of this
    /// Package. There is no merge, so it is refused rather than resolved.
    RepairVendorNameInUse,
    /// A deliberately created Vendor was given no Name. A placeholder is not a Vendor.
    ///
    /// **No command line reaches this**, because this adapter refuses a resolution that is
    /// blank or whitespace-only before the core is asked — under
    /// [`Self::RepairResolutionMissing`], with every other resolution the action still owes
    /// beside it, which is the more useful answer for a caller who supplied none of them.
    /// The two guards ask the same question of the same text, so nothing gets between them.
    /// The arm is written for the reason [`Self::OrderReusableValueUnconfirmed`]'s is: the
    /// refusal is the core's and a library caller reaches it, so the mapping stays total
    /// rather than folding an unrelated code over it. Unlike its neighbour above, which a
    /// command line does reach — a Name already in use is judged after the trim, on a
    /// different property.
    RepairVendorNameRequired,
    /// The identity minted for this repair is already carried by a record of this Package.
    /// Nothing was written, and running the repair again mints another.
    RepairIdentityCollision,
    /// The Package could not be read as a current Package while this repair's candidate was
    /// being judged against it, so nothing shows the candidate is safe.
    ///
    /// **Distinct from a collision on purpose.** A collision is answered by generating
    /// another identity; this is answered by no identity at all, and reporting it as one
    /// would send a caller to retry forever.
    RepairPackageEvidenceUnavailable,
    /// The evidence this repair was authorized against stopped standing between planning
    /// and the write, so it is no longer the case that was diagnosed. Nothing was written.
    RepairEvidenceChanged,
    /// The destination stopped holding what the diagnosis observed there, so someone wrote
    /// it after this repair was authorized. Nothing was written and their work stands.
    ///
    /// **Distinguishable from a write failure**, which is [`Self::RepairFailed`]: nothing
    /// here failed. The correction is to diagnose again and decide against their bytes.
    RepairDestinationChanged,
    /// The repair could not be written. Every managed file holds what it held.
    RepairFailed,
    /// The same, and the staging artifact this attempt created could not be removed again.
    /// Its own code because the action differs: the staging artifact is what remains — the
    /// destination holds what it held — and this application will not guess whether it may
    /// delete it. It travels beside the destination under [`Failure::paths`].
    RepairCleanupFailed,
    /// A repair of several managed files at once failed partway, and one of the files it
    /// had already written could not be put back as it was.
    ///
    /// **Its own code because the Package is in neither state.** [`Self::RepairFailed`]
    /// says every managed file holds what it held, which is what makes it safe to try
    /// again; this says some file holds neither what it held nor a complete repair, and
    /// every such file is named under [`Failure::paths`].
    RepairRestorationFailed,
    /// The Package configuration that names the Orders Directory is missing, unreadable,
    /// or unusable, so a Metadata Upgrade refuses rather than assuming the conventional
    /// location. Assuming wrong would enumerate the wrong directory, find nothing
    /// pending, and advance the Package marker past records it never migrated.
    UpgradeConfigurationUnusable,
    /// A managed file a Metadata Upgrade had to replace could not be replaced, and the run
    /// had written nothing yet. Every managed file holds what it held.
    UpgradeFailed,
    /// A path the response would have to carry is not valid UTF-8.
    PathNotRepresentable,
    /// The command line was wrong.
    UsageInvalid,
}

impl FailureCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PackageRootUnmanaged => "package_root_unmanaged",
            Self::PackageRootDamaged => "package_root_damaged",
            Self::PackageMetadataUnsupported => "package_metadata_unsupported",
            Self::PackageUpgradePending => "package_upgrade_pending",
            Self::PackageUnreadable => "package_unreadable",
            Self::PackageInvalid => "package_invalid",
            Self::OrderInvalid => "order_invalid",
            Self::OrderRootUnmanaged => "order_root_unmanaged",
            Self::OrderRecordDamaged => "order_record_damaged",
            Self::OrderLocatorUnusable => "order_locator_unusable",
            Self::OrderLocatorStale => "order_locator_stale",
            Self::OrderNotFound => "order_not_found",
            Self::VendorNotFound => "vendor_not_found",
            Self::PackageHasNoVendors => "package_has_no_vendors",
            Self::VendorAssignmentRejected => "vendor_assignment_rejected",
            Self::VendorNameRequired => "vendor_name_required",
            Self::VendorNameInUse => "vendor_name_in_use",
            Self::VendorMutationPreconditionsChanged => "vendor_mutation_preconditions_changed",
            Self::VendorMutationFailed => "vendor_mutation_failed",
            Self::VendorMutationCleanupFailed => "vendor_mutation_cleanup_failed",
            Self::VendorIdentityInvalid => "vendor_identity_invalid",
            Self::VendorIdentityCollision => "vendor_identity_collision",
            Self::VendorAggregateInvalid => "vendor_aggregate_invalid",
            Self::VendorContactPositionInvalid => "vendor_contact_position_invalid",
            Self::VendorContactNameRequired => "vendor_contact_name_required",
            Self::OrderAssignmentRejected => "order_assignment_rejected",
            Self::OrderFactsRequired => "order_facts_required",
            Self::OrderStatusInvalid => "order_status_invalid",
            Self::OrderDateOrderedInvalid => "order_date_ordered_invalid",
            Self::OrderCurrencyInvalid => "order_currency_invalid",
            Self::OrderFactsInvalid => "order_facts_invalid",
            Self::OrderItemPositionInvalid => "order_item_position_invalid",
            Self::OrderItemFactsRequired => "order_item_facts_required",
            Self::OrderReusableValueUnconfirmed => "order_reusable_value_unconfirmed",
            Self::OrderVendorUnknown => "order_vendor_unknown",
            Self::OrderVendorUnresolved => "order_vendor_unresolved",
            Self::OrderTrashUnsupported => "order_trash_unsupported",
            Self::OrderTrashSubmissionFailed => "order_trash_submission_failed",
            Self::OrderTitleUnusable => "order_title_unusable",
            Self::OrderDirectoryOccupied => "order_directory_occupied",
            Self::OrderCandidateNotADirectChild => "order_candidate_not_a_direct_child",
            Self::OrderCandidateHidden => "order_candidate_hidden",
            Self::OrderCandidateLink => "order_candidate_link",
            Self::OrderCandidateIneligible => "order_candidate_ineligible",
            Self::OrderCandidateNameUnusable => "order_candidate_name_unusable",
            Self::OrderCandidateAlreadyManaged => "order_candidate_already_managed",
            Self::OrderCandidateReservedStateConflict => "order_candidate_reserved_state_conflict",
            Self::OrderCandidateStagingOccupied => "order_candidate_staging_occupied",
            Self::OrderMutationPreconditionsChanged => "order_mutation_preconditions_changed",
            Self::OrderMutationFailed => "order_mutation_failed",
            Self::OrderMutationCleanupFailed => "order_mutation_cleanup_failed",
            Self::OrderIdentityInvalid => "order_identity_invalid",
            Self::OrderIdentityCollision => "order_identity_collision",
            Self::InitializationFilesystemFailed => "initialization_filesystem_failed",
            Self::PackageRootNotADirectory => "package_root_not_a_directory",
            Self::PackageRootAlreadyManaged => "package_root_already_managed",
            Self::ConfigurationPathAbsolute => "configuration_path_absolute",
            Self::ConfigurationPathCurrentDirectory => "configuration_path_current_directory",
            Self::ConfigurationPathParentTraversal => "configuration_path_parent_traversal",
            Self::ConfigurationPathEmptyComponent => "configuration_path_empty_component",
            Self::ConfigurationPathPlatformSeparator => "configuration_path_platform_separator",
            Self::ConfigurationPathReservedMetadata => "configuration_path_reserved_metadata",
            Self::ConfigurationPathNotDirectChild => "configuration_path_not_direct_child",
            Self::ConfigurationPathReservedReadme => "configuration_path_reserved_readme",
            Self::ConfigurationPathsOverlap => "configuration_paths_overlap",
            Self::InitializationDisallowedLink => "initialization_disallowed_link",
            Self::InitializationDestinationOccupied => "initialization_destination_occupied",
            Self::PackageRootChanged => "package_root_changed",
            Self::PackageInitializationFailed => "package_initialization_failed",
            Self::PackageInitializationIncomplete => "package_initialization_incomplete",
            Self::IndexProjectionIncomplete => "index_projection_incomplete",
            Self::IndexReplacementFailed => "index_replacement_failed",
            Self::RepairCaseUnknown => "repair_case_unknown",
            Self::RepairActionUnauthorized => "repair_action_unauthorized",
            Self::RepairActionUnsupported => "repair_action_unsupported",
            Self::RepairResolutionMissing => "repair_resolution_missing",
            Self::RepairResolutionUnexpected => "repair_resolution_unexpected",
            Self::RepairCandidateInvalid => "repair_candidate_invalid",
            Self::RepairCandidateDamagesPackage => "repair_candidate_damages_package",
            Self::RepairStagingEvidencePresent => "repair_staging_evidence_present",
            Self::RepairVendorUnknown => "repair_vendor_unknown",
            Self::RepairVendorNameInUse => "repair_vendor_name_in_use",
            Self::RepairVendorNameRequired => "repair_vendor_name_required",
            Self::RepairIdentityCollision => "repair_identity_collision",
            Self::RepairPackageEvidenceUnavailable => "repair_package_evidence_unavailable",
            Self::RepairEvidenceChanged => "repair_evidence_changed",
            Self::RepairDestinationChanged => "repair_destination_changed",
            Self::RepairFailed => "repair_failed",
            Self::RepairCleanupFailed => "repair_cleanup_failed",
            Self::RepairRestorationFailed => "repair_restoration_failed",
            Self::UpgradeConfigurationUnusable => "upgrade_configuration_unusable",
            Self::UpgradeFailed => "upgrade_failed",
            Self::PathNotRepresentable => "path_not_representable",
            Self::UsageInvalid => "usage_invalid",
        }
    }

    /// The code for one broken configuration path rule.
    ///
    /// Taken from the core's own reason rather than from the plan error, so the seven
    /// codes above and the seven reasons a loaded Package reports stay one set. The
    /// eighth reason — a `removed_path` that exists and is not an ordinary directory —
    /// is only ever observed by a load of an existing Package, so an initialization
    /// cannot reach it; it lands on the ordinary occupied-destination code instead.
    pub(crate) fn of_configuration_path(reason: ConfigurationPathReason) -> Self {
        match reason {
            ConfigurationPathReason::Absolute => Self::ConfigurationPathAbsolute,
            ConfigurationPathReason::CurrentDirectory => Self::ConfigurationPathCurrentDirectory,
            ConfigurationPathReason::ParentTraversal => Self::ConfigurationPathParentTraversal,
            ConfigurationPathReason::EmptyComponent => Self::ConfigurationPathEmptyComponent,
            ConfigurationPathReason::PlatformSeparator => Self::ConfigurationPathPlatformSeparator,
            ConfigurationPathReason::ReservedMetadata => Self::ConfigurationPathReservedMetadata,
            ConfigurationPathReason::NotDirectChild => Self::ConfigurationPathNotDirectChild,
            ConfigurationPathReason::NotOrdinary => Self::InitializationDestinationOccupied,
        }
    }
}

/// One entity identity a failure is about — an Order, a Vendor, a Contact position.
///
/// Empty for every failure this executable can presently reach, and present because the
/// contract says a failure carries "relevant paths **or** identities": a command that
/// fails about a Vendor puts the Vendor ID here rather than inventing a member for it.
///
/// Ticket 002 left the member with no constructor, for the first command that fails
/// about an entity to add together with its `kind` spellings. `order show --uuid` is that
/// command: an Order ID that names no Order of the Package is a failure about an identity
/// and about no path, so `paths` would be empty and the identity is the only thing that
/// says what was asked for.
///
/// **One constructor per `kind`, rather than a public `kind` string**, so each spelling
/// exists in exactly one place — the property `FailureCode` and `FailureScope` have for
/// the same reason. A ticket whose command fails about a Vendor or a Contact position
/// adds its own constructor here beside [`Self::order`].
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Identity {
    kind: &'static str,
    value: String,
}

impl Identity {
    /// One Order, by the immutable opaque Order ID its record carries.
    pub(crate) fn order(order_id: &str) -> Self {
        Self {
            kind: "order",
            value: order_id.to_owned(),
        }
    }

    /// One Vendor, by the Vendor Name a caller named it with.
    ///
    /// **The Name rather than the Vendor ID, and the `kind` says so.** A Vendor's
    /// identity is its immutable Vendor ID, and a Name that resolved to nothing has none
    /// to report — so what this failure is about is the selector the caller supplied.
    /// Reporting it under `vendor` would claim a Name is an identity, which Define Vendor
    /// identity and historical semantics settles the other way.
    pub(crate) fn vendor_name(name: &str) -> Self {
        Self {
            kind: "vendor_name",
            value: name.to_owned(),
        }
    }

    /// One Vendor, by the immutable opaque Vendor ID its record carries.
    ///
    /// The counterpart of [`Self::vendor_name`], and used where the failure is about a
    /// Vendor that *does* resolve: a Name already in use is refused, and what a caller
    /// needs is the Vendor already carrying it.
    pub(crate) fn vendor(vendor_id: &str) -> Self {
        Self {
            kind: "vendor",
            value: vendor_id.to_owned(),
        }
    }

    /// One canonical assignment a mutation refused, by the field it named.
    ///
    /// **`kind` is the rule the field broke** rather than a constant, which is the one
    /// place this type departs from one constructor per `kind` — and the departure is the
    /// point. An assignment set is adjudicated as a whole, so a single refusal can name
    /// several fields for several different reasons, and the failure's one `code` cannot
    /// say which field broke which rule. Every spelling is still in exactly one place: it
    /// is the core's own [`RejectedAssignmentReason::as_str`], so a rule refused here and
    /// the same rule reported by the library cannot come to be named differently.
    pub(crate) fn rejected_assignment(field: &str, reason: RejectedAssignmentReason) -> Self {
        Self {
            kind: reason.as_str(),
            value: field.to_owned(),
        }
    }

    /// One position in a Vendor's ordered Contact collection, as the command line named
    /// it.
    ///
    /// **`kind` is which end the position was for** rather than a constant, which is the
    /// second place this type departs from one constructor per `kind` — and it departs for
    /// [`Self::rejected_assignment`]'s reason. A `move` addresses two positions and the
    /// failure's one `code` cannot say which of them was refused. The spelling is the
    /// core's own [`ContactPositionRole::as_str`], so a role refused here and the same role
    /// reported by the library cannot come to be named differently.
    ///
    /// **The value is the position as written**, because a token that is not a number at
    /// all is one of the ways to name no position — and quoting it back is what tells a
    /// caller which of their arguments this was about.
    pub(crate) fn contact_position(role: ContactPositionRole, position: &str) -> Self {
        Self {
            kind: role.as_str(),
            value: position.to_owned(),
        }
    }

    /// One fact a mutation requires and this command line did not assign, by its name.
    ///
    /// **The record's own field name**, which is also the token a caller writes to supply
    /// it — so the identity a refusal reports and the assignment that fixes it are the same
    /// word. Distinct from [`Self::rejected_assignment`], whose `kind` is the rule a
    /// *supplied* field broke: this one was not supplied at all.
    pub(crate) fn required_fact(field: &str) -> Self {
        Self {
            kind: "required_fact",
            value: field.to_owned(),
        }
    }

    /// One position in an Order's ordered Item collection, as the command line named it.
    ///
    /// [`Self::contact_position`]'s counterpart for the other ordered collection, with the
    /// same two `kind` spellings and for the same reasons: a `move` addresses two positions
    /// and the failure's one `code` cannot say which of them was refused. The spelling is
    /// the core's own [`ItemPositionRole::as_str`], which is the same spelling a Contact
    /// position's role carries, and the value is the position as written.
    pub(crate) fn item_position(role: ItemPositionRole, position: &str) -> Self {
        Self {
            kind: role.as_str(),
            value: position.to_owned(),
        }
    }

    /// One Order Status as the command line wrote it, where it named none.
    ///
    /// The value is the text as written, because quoting it back is what tells a caller
    /// which of their arguments this was about — the reason
    /// [`Self::contact_position`] carries a position as written.
    pub(crate) fn order_status(value: &str) -> Self {
        Self {
            kind: "order_status",
            value: value.to_owned(),
        }
    }

    /// One Date Ordered as the command line wrote it, where it is not a real calendar day.
    ///
    /// A constructor of its own beside [`Self::order_status`], rather than one taking the
    /// fact as an argument: one constructor per `kind` is what keeps each spelling in
    /// exactly one place, and these two facts are refused under two codes because the
    /// correction differs.
    pub(crate) fn order_date_ordered(value: &str) -> Self {
        Self {
            kind: "order_date_ordered",
            value: value.to_owned(),
        }
    }

    /// One Order Currency as the command line wrote it, where it is not a three-letter
    /// code.
    pub(crate) fn order_currency(value: &str) -> Self {
        Self {
            kind: "order_currency",
            value: value.to_owned(),
        }
    }

    /// One `field=value` a repair action does not take, by the field it named.
    ///
    /// Distinct from [`Self::required_fact`], whose value is a resolution the action needs
    /// and this command line did not supply: this one was supplied and belongs to no
    /// action here.
    pub(crate) fn repair_resolution(field: &str) -> Self {
        Self {
            kind: "repair_resolution",
            value: field.to_owned(),
        }
    }

    /// The Package-wide constraint a candidate would have broken, in the core's own
    /// spelling.
    ///
    /// **A fact a caller branches on**, not prose: picking another Vendor, taking a free
    /// Order identity, and correcting a locator are three different next steps, and a
    /// consumer that had to substring-match the sentence to tell them apart would be
    /// parsing human output.
    pub(crate) fn package_constraint(constraint: &str) -> Self {
        Self {
            kind: "package_constraint",
            value: constraint.to_owned(),
        }
    }

    /// One configured managed path, by the key that names it.
    ///
    /// **Not a filesystem path**, which is why it is here rather than under
    /// [`Failure::paths`]: `orders_path` is a key in `.spielorders/config.toml`, and a
    /// value that broke a rule may have no place on any filesystem at all. The failure's
    /// `code` says which rule broke and this says which of the two keys broke it —
    /// together, the whole typed answer without a member the envelope does not have.
    pub(crate) fn configuration_path(key: ConfigurationPathKind) -> Self {
        Self {
            kind: "configuration_path",
            value: key.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Failure {
    code: &'static str,
    message: String,
    scope: &'static str,
    paths: Vec<FailedPathProjection>,
    identities: Vec<Identity>,
}

impl Failure {
    pub(crate) fn new(code: FailureCode, scope: FailureScope, message: impl Into<String>) -> Self {
        Self {
            code: code.as_str(),
            message: message.into(),
            scope: scope.as_str(),
            paths: Vec::new(),
            identities: Vec::new(),
        }
    }

    /// Adds one path this failure is about.
    ///
    /// A [`FailedPath`] rather than a `Path` or a `Location`, so that the spelling
    /// travels with the path and no caller has to choose it. A `Location` converts
    /// into one, and a path this adapter never resolved is
    /// [`FailedPath::Unlocated`] — the honest name for the caller's own text.
    pub(crate) fn at(mut self, path: &FailedPath) -> Result<Self, NotRepresentable> {
        self.paths.push(FailedPathProjection::of(path)?);
        Ok(self)
    }

    /// Adds one entity identity this failure is about.
    ///
    /// The contract says a failure carries "relevant paths **or** identities", and a
    /// failure about an entity the caller named by identity has no path to give: an Order
    /// ID that resolves to nothing has no Order Directory.
    pub(crate) fn about(mut self, identity: Identity) -> Self {
        self.identities.push(identity);
        self
    }
}

/// The one envelope, whichever command produced it.
///
/// `C` is the command's targeting context — [`Context`] for a Package-level command and
/// [`OrderContext`] for an `order` command, which has a second root to name. It is a type
/// parameter rather than one union of every member any command might target, because
/// "absent optional output facts are `null`" is about a fact this response *has* and does
/// not have a value for; a member naming a root the command has no notion of is not that.
/// Everything else about the envelope — the seven members, their order, `ok` against the
/// exit status, `data` and `error` being mutually exclusive — is fixed here for every
/// command and is not a command's to redefine.
#[derive(Debug, Serialize)]
pub(crate) struct Response<T, C = Context> {
    contract: &'static str,
    contract_version: u32,
    ok: bool,
    context: C,
    data: Option<T>,
    diagnostics: Vec<DiagnosticProjection>,
    error: Option<Failure>,
}

impl<T, C> Response<T, C> {
    /// The command answered. `data` is its payload, `error` is `null`, and the process
    /// exits `0`.
    pub(crate) fn answered(
        contract: ResponseContract,
        context: C,
        data: T,
        diagnostics: Vec<DiagnosticProjection>,
    ) -> Self {
        Self {
            contract: contract.name(),
            contract_version: contract.version(),
            ok: true,
            context,
            data: Some(data),
            diagnostics,
            error: None,
        }
    }

    /// The command failed. `data` is `null`, `error` carries the typed failure, and
    /// the process exits `1` or `2`.
    ///
    /// A failure carries diagnostics like any other response — the contract lists them
    /// among what a failed `--json` response retains, and `validate` fails at exit `1`
    /// *with* the typed diagnostics that are its whole answer. Taking them as an
    /// argument rather than writing an empty vector here is what keeps a later command
    /// from having to redefine this constructor to report them. `package show` is the
    /// first command to need it: a damaged Package's diagnostics *are* its answer, and
    /// `data` is `null` on every failure, so this is the only channel that answer has.
    /// A failure that observed nothing still passes an empty vector.
    pub(crate) fn failed(
        contract: ResponseContract,
        context: C,
        error: Failure,
        diagnostics: Vec<DiagnosticProjection>,
    ) -> Self {
        Self {
            contract: contract.name(),
            contract_version: contract.version(),
            ok: false,
            context,
            data: None,
            diagnostics,
            error: Some(error),
        }
    }
}
