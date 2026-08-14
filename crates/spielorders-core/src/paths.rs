//! Safe filesystem paths and direct-child classification for a SpielOrder Package.

use crate::{
    metadata::{Config, PackageRoot},
    ports::{FileSystem, FileSystemEntryKind},
};
use std::path::{Component, Path, PathBuf};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectChildClassification {
    OrderCandidate,
    IgnoredHidden,
    Disallowed(FileSystemEntryKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservedPathKind {
    PackageMetadata,
    OrderMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectChildObservation {
    path: PathBuf,
    classification: DirectChildClassification,
}

impl DirectChildObservation {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn classification(&self) -> DirectChildClassification {
        self.classification
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageLayout {
    root: PathBuf,
    orders_directory: PathBuf,
    removed_orders_directory: PathBuf,
    direct_children: Vec<DirectChildObservation>,
}

impl PackageLayout {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn orders_directory(&self) -> &Path {
        &self.orders_directory
    }

    pub fn removed_orders_directory(&self) -> &Path {
        &self.removed_orders_directory
    }

    pub fn direct_children(&self) -> &[DirectChildObservation] {
        &self.direct_children
    }

    pub fn package_metadata_directory(&self) -> PathBuf {
        self.root.join(".spielorders")
    }

    /// Classifies a path by its lexically normalized location within this layout.
    ///
    /// The input must be rooted at the layout's canonical SpielOrders Root.
    /// Current-directory components are ignored, and parent components may move
    /// only within that root. A path outside the root, or one that attempts to
    /// traverse above it, is not reserved. Classification performs no filesystem
    /// access, recursive inspection, or link following.
    pub fn reserved_path_kind(&self, path: &Path) -> Option<ReservedPathKind> {
        let normalized_path = normalize_path_within(&self.root, path)?;
        if normalized_path.starts_with(self.package_metadata_directory()) {
            return Some(ReservedPathKind::PackageMetadata);
        }

        self.direct_children
            .iter()
            .filter(|entry| entry.classification == DirectChildClassification::OrderCandidate)
            .any(|entry| normalized_path.starts_with(entry.path.join(".spielorders")))
            .then_some(ReservedPathKind::OrderMetadata)
    }
}

fn normalize_path_within(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok()?;
    let mut normalized = root.to_path_buf();
    let mut relative_depth = 0usize;

    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(component) => {
                normalized.push(component);
                relative_depth += 1;
            }
            Component::ParentDir if relative_depth > 0 => {
                normalized.pop();
                relative_depth -= 1;
            }
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    Some(normalized)
}

#[derive(Debug, PartialEq)]
pub enum PackagePathError<E> {
    FileSystem(E),
    InvalidConfiguration {
        path: ConfiguredPath,
        failure: ConfiguredPathFailure,
    },
    ManagedPathsOverlap,
    RootNotDirectory,
    OrdersDirectoryNotDirectory,
    OrdersDirectoryOutsideRoot,
    RemovedOrdersDirectoryNotOrdinary,
}

#[derive(Debug, PartialEq)]
pub enum OrderLocatorError<E> {
    PackagePath(PackagePathError<E>),
    NotActiveDirectChild,
    DoesNotResolveToRoot,
}

struct ResolvedManagedPaths {
    root: PathBuf,
    orders_directory: PathBuf,
    removed_orders_directory: PathBuf,
}

pub fn inspect_package_layout<F: FileSystem>(
    root: &Path,
    config: &Config,
    filesystem: &F,
) -> Result<PackageLayout, PackagePathError<F::Error>> {
    let resolved = resolve_managed_paths(root, config, filesystem)?;
    let mut entries = filesystem
        .read_directory(&resolved.orders_directory)
        .map_err(PackagePathError::FileSystem)?;
    entries.sort_by(|left, right| left.name().cmp(right.name()));
    let direct_children = entries
        .into_iter()
        .map(|entry| {
            let classification = if entry.name().as_encoded_bytes().starts_with(b".") {
                DirectChildClassification::IgnoredHidden
            } else if entry.kind() == FileSystemEntryKind::Directory {
                DirectChildClassification::OrderCandidate
            } else {
                DirectChildClassification::Disallowed(entry.kind())
            };
            DirectChildObservation {
                path: resolved.orders_directory.join(entry.name()),
                classification,
            }
        })
        .collect();

    Ok(PackageLayout {
        root: resolved.root,
        orders_directory: resolved.orders_directory,
        removed_orders_directory: resolved.removed_orders_directory,
        direct_children,
    })
}

pub fn validate_order_package_locator<F: FileSystem>(
    root: &Path,
    config: &Config,
    order_directory: &Path,
    package_root: &PackageRoot,
    filesystem: &F,
) -> Result<(), OrderLocatorError<F::Error>> {
    let resolved =
        resolve_managed_paths(root, config, filesystem).map_err(OrderLocatorError::PackagePath)?;
    if filesystem
        .symlink_metadata(order_directory)
        .map_err(|error| OrderLocatorError::PackagePath(PackagePathError::FileSystem(error)))?
        != Some(FileSystemEntryKind::Directory)
        || order_directory
            .file_name()
            .is_none_or(|name| name.as_encoded_bytes().starts_with(b"."))
    {
        return Err(OrderLocatorError::NotActiveDirectChild);
    }
    let order_directory = filesystem
        .canonicalize(order_directory)
        .map_err(|error| OrderLocatorError::PackagePath(PackagePathError::FileSystem(error)))?;
    if order_directory.parent() != Some(resolved.orders_directory.as_path()) {
        return Err(OrderLocatorError::NotActiveDirectChild);
    }

    let parent_count = package_root.as_str().split('/').count();
    if order_directory.ancestors().nth(parent_count) != Some(resolved.root.as_path()) {
        return Err(OrderLocatorError::DoesNotResolveToRoot);
    }
    Ok(())
}

fn resolve_managed_paths<F: FileSystem>(
    root: &Path,
    config: &Config,
    filesystem: &F,
) -> Result<ResolvedManagedPaths, PackagePathError<F::Error>> {
    let orders_components =
        validate_configured_path(&config.orders_path, ConfiguredPath::Orders, false)?;
    let removed_components =
        validate_configured_path(&config.removed_path, ConfiguredPath::Removed, true)?;
    if paths_overlap(&orders_components, &removed_components) {
        return Err(PackagePathError::ManagedPathsOverlap);
    }
    let root = filesystem
        .canonicalize(root)
        .map_err(PackagePathError::FileSystem)?;
    if filesystem
        .symlink_metadata(&root)
        .map_err(PackagePathError::FileSystem)?
        != Some(FileSystemEntryKind::Directory)
    {
        return Err(PackagePathError::RootNotDirectory);
    }

    let removed_orders_directory = join_components(&root, &removed_components);
    if !matches!(
        filesystem
            .symlink_metadata(&removed_orders_directory)
            .map_err(PackagePathError::FileSystem)?,
        None | Some(FileSystemEntryKind::Directory)
    ) {
        return Err(PackagePathError::RemovedOrdersDirectoryNotOrdinary);
    }

    let mut orders_directory = None;
    for component_count in 1..=orders_components.len() {
        let resolved = filesystem
            .canonicalize(&join_components(
                &root,
                &orders_components[..component_count],
            ))
            .map_err(PackagePathError::FileSystem)?;
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
    if orders_directory.starts_with(&removed_orders_directory)
        || removed_orders_directory.starts_with(&orders_directory)
    {
        return Err(PackagePathError::ManagedPathsOverlap);
    }
    if filesystem
        .symlink_metadata(&orders_directory)
        .map_err(PackagePathError::FileSystem)?
        != Some(FileSystemEntryKind::Directory)
    {
        return Err(PackagePathError::OrdersDirectoryNotDirectory);
    }

    Ok(ResolvedManagedPaths {
        root,
        orders_directory,
        removed_orders_directory,
    })
}

fn validate_configured_path<E>(
    value: &str,
    path: ConfiguredPath,
    require_direct_child: bool,
) -> Result<Vec<&str>, PackagePathError<E>> {
    let bytes = value.as_bytes();
    let has_windows_drive_prefix =
        bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if value.starts_with('/')
        || value.starts_with('\\')
        || has_windows_drive_prefix
        || Path::new(value).is_absolute()
    {
        return Err(PackagePathError::InvalidConfiguration {
            path,
            failure: ConfiguredPathFailure::Absolute,
        });
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
    } else if components.first() == Some(&".spielorders") {
        Some(ConfiguredPathFailure::ReservedMetadata)
    } else if require_direct_child && components.len() != 1 {
        Some(ConfiguredPathFailure::NotDirectChild)
    } else {
        None
    };

    match failure {
        Some(failure) => Err(PackagePathError::InvalidConfiguration { path, failure }),
        None => Ok(components),
    }
}

fn join_components(root: &Path, components: &[&str]) -> PathBuf {
    components
        .iter()
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn paths_overlap(left: &[&str], right: &[&str]) -> bool {
    left.starts_with(right) || right.starts_with(left)
}
