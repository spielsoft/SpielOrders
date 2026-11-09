//! Safe filesystem paths and direct-child classification for a SpielOrder Package.

use crate::operations::FileSystemOperation;
use crate::{
    metadata::{Config, PackageRoot},
    ports::{FileSystem, FileSystemEntryKind},
    readme::{ConfigurationPathKind, ConfigurationPathReason, package_relative_path_text},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfiguredPath {
    Orders,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfiguredPathFailure {
    Absolute,
    CurrentDirectory,
    ParentTraversal,
    EmptyComponent,
    PlatformSeparator,
    ReservedMetadata,
    NotDirectChild,
}

/// The reported spelling of the configured key this is about.
///
/// One conversion rather than a mapping in each caller, because a configured key that a
/// loaded Package spells one way and a refused initialization another is one fact with
/// two names. `ConfigurationPathKind`'s code stays the one place the spelling exists.
impl From<ConfiguredPath> for ConfigurationPathKind {
    fn from(path: ConfiguredPath) -> Self {
        match path {
            ConfiguredPath::Orders => Self::OrdersPath,
            ConfiguredPath::Removed => Self::RemovedPath,
        }
    }
}

/// The reported spelling of the rule this configured value broke, for the same reason.
///
/// [`ConfigurationPathReason`] carries one variant this cannot produce — a `removed_path`
/// that is not an ordinary directory, which only a resolved Package observes.
impl From<ConfiguredPathFailure> for ConfigurationPathReason {
    fn from(failure: ConfiguredPathFailure) -> Self {
        match failure {
            ConfiguredPathFailure::Absolute => Self::Absolute,
            ConfiguredPathFailure::CurrentDirectory => Self::CurrentDirectory,
            ConfiguredPathFailure::ParentTraversal => Self::ParentTraversal,
            ConfiguredPathFailure::EmptyComponent => Self::EmptyComponent,
            ConfiguredPathFailure::PlatformSeparator => Self::PlatformSeparator,
            ConfiguredPathFailure::ReservedMetadata => Self::ReservedMetadata,
            ConfiguredPathFailure::NotDirectChild => Self::NotDirectChild,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectChildClassification {
    OrderCandidate,
    IgnoredHidden,
    Disallowed(FileSystemEntryKind),
    /// A direct child this application cannot name: its filesystem name has no
    /// normalized Package-relative spelling, so nothing that describes a Package
    /// can refer to it. The entry is left exactly as the user made it and is
    /// never opened, listed, or indexed.
    NameUnusable(FileSystemEntryKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DirectChildObservation {
    pub(crate) path: PathBuf,
    pub(crate) classification: DirectChildClassification,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PackageLayout {
    root: PathBuf,
    pub(crate) orders_directory: PathBuf,
    pub(crate) removed_orders_directory: PathBuf,
    pub(crate) direct_children: Vec<DirectChildObservation>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum PackagePathError<E> {
    FilesystemOperationFailed(E),
    InvalidConfiguration {
        path: ConfiguredPath,
        failure: ConfiguredPathFailure,
    },
    ManagedPathsOverlap,
    RootNotDirectory,
    OrdersDirectoryNotDirectory,
    OrdersDirectoryOutsideRoot,
    /// The Orders Directory resolves to a real name this application cannot express
    /// as a normalized Package-relative path, so nothing that describes the Package
    /// could refer to it.
    OrdersDirectoryNameUnusable,
    RemovedOrdersDirectoryNotOrdinary,
}

struct ResolvedManagedPaths {
    root: PathBuf,
    orders_directory: PathBuf,
    removed_orders_directory: PathBuf,
}

/// Establishes confined managed Package paths without enumerating Order membership.
pub(crate) fn resolve_package_layout<F: FileSystem>(
    root: &Path,
    config: &Config,
    filesystem: &F,
) -> Result<PackageLayout, PackagePathError<F::Error>> {
    let resolved = resolve_managed_paths(root, config, filesystem)?;
    Ok(PackageLayout {
        root: resolved.root,
        orders_directory: resolved.orders_directory,
        removed_orders_directory: resolved.removed_orders_directory,
        direct_children: Vec::new(),
    })
}

/// Lists and classifies exactly the established Orders Directory's direct children.
///
/// A name the user chose enters the application by being discovered here, so this
/// is where a discovered name the application cannot express is refused. Every
/// reader of Order membership — the ordinary load, the current-index fast path,
/// and exhaustive validation — reads this one list, so refusing at classification
/// rather than where each of them later builds a `Location` makes all three safe
/// by construction instead of by three separate guards.
///
/// It does not cover a capability that *names* a directory rather than
/// discovering one. Order adoption is the one that does, and it asks the same
/// question itself.
pub(crate) fn discover_order_direct_children<F: FileSystem>(
    mut layout: PackageLayout,
    filesystem: &F,
) -> Result<PackageLayout, F::Error> {
    let mut entries = filesystem.read_directory(&layout.orders_directory)?;
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    layout.direct_children = entries
        .into_iter()
        .map(|entry| {
            let path = layout.orders_directory.join(&entry.name);
            let classification = if entry.name.as_encoded_bytes().starts_with(b".") {
                // Hidden children are ignored whatever they are called. Nothing
                // ever builds a location for one, so an unusable hidden name has
                // no consequence and stays undiagnosed, as the diagnostic
                // contract requires of every hidden child.
                DirectChildClassification::IgnoredHidden
            } else if package_relative_path_text(&layout.root, &path).is_none() {
                DirectChildClassification::NameUnusable(entry.kind)
            } else if entry.kind == FileSystemEntryKind::Directory {
                DirectChildClassification::OrderCandidate
            } else {
                DirectChildClassification::Disallowed(entry.kind)
            };
            DirectChildObservation {
                path,
                classification,
            }
        })
        .collect();

    Ok(layout)
}

/// Whether an Order Directory's technical locator leads to this Package's root.
///
/// One question with one answer. The Package loader asks it of a stored record and
/// metadata repair asks it of a candidate before writing one; separate copies could
/// disagree about what a locator means.
pub(crate) fn locator_resolves_to_root(
    order_directory: &Path,
    package_root: &PackageRoot,
    root: &Path,
) -> bool {
    let parent_count = package_root.as_str().split('/').count();
    order_directory.ancestors().nth(parent_count) == Some(root)
}

fn resolve_managed_paths<F: FileSystem>(
    root: &Path,
    config: &Config,
    filesystem: &F,
) -> Result<ResolvedManagedPaths, PackagePathError<F::Error>> {
    let orders_components =
        validate_configured_path(&config.orders_path, false).map_err(|failure| {
            PackagePathError::InvalidConfiguration {
                path: ConfiguredPath::Orders,
                failure,
            }
        })?;
    let removed_components =
        validate_configured_path(&config.removed_path, true).map_err(|failure| {
            PackagePathError::InvalidConfiguration {
                path: ConfiguredPath::Removed,
                failure,
            }
        })?;
    if paths_overlap(&orders_components, &removed_components) {
        return Err(PackagePathError::ManagedPathsOverlap);
    }
    let root = filesystem
        .canonicalize(root)
        .map_err(PackagePathError::FilesystemOperationFailed)?;
    if filesystem
        .symlink_metadata(&root)
        .map_err(PackagePathError::FilesystemOperationFailed)?
        != Some(FileSystemEntryKind::Directory)
    {
        return Err(PackagePathError::RootNotDirectory);
    }

    let configured_removed = join_components(&root, &removed_components);
    let removed_orders_directory = match resolve_managed_directory(&configured_removed, filesystem)
        .map_err(|(_, error)| PackagePathError::FilesystemOperationFailed(error))?
    {
        ManagedDirectory::Resolved(resolved) => resolved,
        ManagedDirectory::Absent => configured_removed,
        ManagedDirectory::NotOrdinary => {
            return Err(PackagePathError::RemovedOrdersDirectoryNotOrdinary);
        }
    };

    let mut orders_directory = None;
    for component_count in 1..=orders_components.len() {
        let resolved = filesystem
            .canonicalize(&join_components(
                &root,
                &orders_components[..component_count],
            ))
            .map_err(PackagePathError::FilesystemOperationFailed)?;
        if !resolved.starts_with(&root) {
            return Err(PackagePathError::OrdersDirectoryOutsideRoot);
        }
        orders_directory = Some(resolved);
    }
    let orders_directory = orders_directory.expect("validated path has at least one component");
    if orders_directory.starts_with(root.join(".spielorders")) {
        return Err(PackagePathError::InvalidConfiguration {
            path: ConfiguredPath::Orders,
            failure: ConfiguredPathFailure::ReservedMetadata,
        });
    }
    if resolved_paths_overlap(&orders_directory, &removed_orders_directory) {
        return Err(PackagePathError::ManagedPathsOverlap);
    }
    if filesystem
        .symlink_metadata(&orders_directory)
        .map_err(PackagePathError::FilesystemOperationFailed)?
        != Some(FileSystemEntryKind::Directory)
    {
        return Err(PackagePathError::OrdersDirectoryNotDirectory);
    }
    // The configured text was checked against the Package-relative grammar, but the
    // name this resolution ends at is the filesystem's own, and a symbolic link makes
    // those two differ. Every projection of the Package would then have to carry a
    // spelling for a name that has none, so the configuration is refused here instead.
    //
    // Last, so that every condition an existing refusal already names keeps naming it:
    // a link to something that is not a directory is still reported as not a
    // directory, which is the more actionable of the two true answers.
    //
    // The Removed Orders Directory needs no such check. It is a direct child, so no
    // component between the root and it can be redirected, and a link at the child
    // itself is already refused as not ordinary. Its resolved name is therefore a
    // spelling of the configured one that the volume itself records — a case or
    // Unicode-normalization variant — and every such variant is still a name the
    // grammar accepts.
    if package_relative_path_text(&root, &orders_directory).is_none() {
        return Err(PackagePathError::OrdersDirectoryNameUnusable);
    }

    Ok(ResolvedManagedPaths {
        root,
        orders_directory,
        removed_orders_directory,
    })
}

/// What the filesystem currently makes of one configured managed path.
pub(crate) enum ManagedDirectory {
    /// A directory is there, spelled the way the filesystem itself spells it.
    Resolved(PathBuf),
    /// Nothing is there.
    Absent,
    /// Something is there that is not an ordinary directory.
    NotOrdinary,
}

/// Resolves one configured managed path to the filesystem's own spelling of it.
///
/// Two managed paths are two directories or one, and only the filesystem knows
/// which: a volume that resolves names without regard to case — APFS in its default
/// macOS configuration — makes `Orders` and `ORDERS` one directory, while a volume
/// that distinguishes them makes two, and no comparison of the configured text can
/// tell those apart. Canonicalizing both sides asks the question the filesystem
/// actually answers, and it answers it the same way for every equivalence a volume
/// enforces rather than only for case.
///
/// A path with nothing at it resolves to nothing rather than to a guess. An absent
/// name is not the directory another configured path already resolves to, so a pair
/// that cannot both be resolved cannot be one directory — which is why the absence
/// of a Removed Orders Directory that is only created on the first removal is an
/// ordinary answer here rather than a gap in the test.
/// A failure names the operation that failed beside the filesystem's error.
pub(crate) fn resolve_managed_directory<F: FileSystem>(
    path: &Path,
    filesystem: &F,
) -> Result<ManagedDirectory, (FileSystemOperation, F::Error)> {
    match filesystem
        .symlink_metadata(path)
        .map_err(|error| (FileSystemOperation::ObserveKind, error))?
    {
        None => Ok(ManagedDirectory::Absent),
        Some(FileSystemEntryKind::Directory) => Ok(ManagedDirectory::Resolved(
            filesystem
                .canonicalize(path)
                .map_err(|error| (FileSystemOperation::Canonicalize, error))?,
        )),
        Some(_) => Ok(ManagedDirectory::NotOrdinary),
    }
}

/// Whether two resolved managed directories are one directory, or one holds the
/// other. Both sides must already carry the filesystem's own spelling for this
/// comparison to mean anything.
pub(crate) fn resolved_paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

/// Checks one configured managed path against the closed relative-path grammar and
/// returns its components in order.
///
/// Package initialization and Package loading ask this same question, so they ask it
/// here rather than each carrying its own copy of the grammar; a copy that drifts lets
/// initialization write a `config.toml` that loading then refuses. Each caller labels
/// the returned failure with the configured path it was checking.
///
/// The reserved metadata name is matched without regard to ASCII case. The metadata
/// directory has to be recognized by name — initialization checks the configuration
/// before it has created anything to canonicalize — and on a case-insensitive
/// filesystem `.SpielOrders` names `.spielorders` itself. Case is the only equivalence
/// this covers: a filesystem may enforce others, Unicode normalization on APFS being
/// the obvious one, that no lexical test can settle.
///
/// A choice: the name is reserved at the first component only. `Orders/.spielorders` is
/// neither the Package metadata directory nor any Order's, so no reserved subtree overlaps
/// it. Reserving it at every depth would change what a configured Orders path may name,
/// which is a product decision rather than a safety one.
pub(crate) fn validate_configured_path(
    value: &str,
    require_direct_child: bool,
) -> Result<Vec<&str>, ConfiguredPathFailure> {
    let bytes = value.as_bytes();
    let has_windows_drive_prefix =
        bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if value.starts_with('/')
        || value.starts_with('\\')
        || has_windows_drive_prefix
        || Path::new(value).is_absolute()
    {
        return Err(ConfiguredPathFailure::Absolute);
    }
    let components = value.split('/').collect::<Vec<_>>();
    let failure = if value.contains('\\') {
        Some(ConfiguredPathFailure::PlatformSeparator)
    } else if components.iter().any(|component| component.is_empty()) {
        Some(ConfiguredPathFailure::EmptyComponent)
    } else if components.contains(&".") {
        Some(ConfiguredPathFailure::CurrentDirectory)
    } else if components.contains(&"..") {
        Some(ConfiguredPathFailure::ParentTraversal)
    } else if components
        .first()
        .is_some_and(|component| component.eq_ignore_ascii_case(".spielorders"))
    {
        Some(ConfiguredPathFailure::ReservedMetadata)
    } else if require_direct_child && components.len() != 1 {
        Some(ConfiguredPathFailure::NotDirectChild)
    } else {
        None
    };

    match failure {
        Some(failure) => Err(failure),
        None => Ok(components),
    }
}

fn join_components(root: &Path, components: &[&str]) -> PathBuf {
    components
        .iter()
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

/// Whether either configured component sequence contains the other, comparing the
/// configured text.
///
/// This is the lexical half of the question, and it is kept because it settles the
/// plain cases before any filesystem access. Two spellings the filesystem treats as
/// one directory are invisible to it; `resolve_managed_directory` answers that half.
pub(crate) fn paths_overlap(left: &[&str], right: &[&str]) -> bool {
    left.starts_with(right) || right.starts_with(left)
}
