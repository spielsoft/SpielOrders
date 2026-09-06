//! Decision sources: Decide whether SpielOrders and SpielGantt share a
//! filesystem-bucket foundation; Define the SpielOrders package topology; Define
//! the Rust CLI capability contract; Define integrity, validation, and repair
//! boundaries; Define durable metadata schemas and version evolution; Require two
//! parent segments in the Order package locator.

use crate::support::{RealFileSystem, TemporaryTree};
use spielorders_core::metadata::{Config, OrderDocument};
use spielorders_core::paths::{
    ConfiguredPath, ConfiguredPathFailure, DirectChildClassification, OrderLocatorError,
    PackagePathError, ReservedPathKind, inspect_package_layout, validate_order_package_locator,
};
use spielorders_core::ports::{DirectoryEntry, FileSystem, FileSystemEntryKind};
use std::{
    collections::BTreeMap,
    convert::Infallible,
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct MemoryFileSystem {
    entries: BTreeMap<PathBuf, FileSystemEntryKind>,
    canonical_paths: BTreeMap<PathBuf, PathBuf>,
}

impl MemoryFileSystem {
    fn directory(mut self, path: &str) -> Self {
        self.entries
            .insert(PathBuf::from(path), FileSystemEntryKind::Directory);
        self
    }

    fn entry(mut self, path: &str, kind: FileSystemEntryKind) -> Self {
        self.entries.insert(PathBuf::from(path), kind);
        self
    }

    fn canonical_path(mut self, path: &str, resolved: &str) -> Self {
        self.canonical_paths
            .insert(PathBuf::from(path), PathBuf::from(resolved));
        self
    }
}

impl FileSystem for MemoryFileSystem {
    type Error = Infallible;

    fn read(&self, _path: &Path) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }

    fn write(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn write_new(&self, _path: &Path, _contents: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn sync_file(&self, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    fn replace_file(&self, _source: &Path, _destination: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    fn remove_file(&self, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Self::Error> {
        Ok(self
            .canonical_paths
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_path_buf()))
    }

    fn symlink_metadata(&self, path: &Path) -> Result<Option<FileSystemEntryKind>, Self::Error> {
        Ok(self.entries.get(path).copied())
    }

    fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, Self::Error> {
        let entries = self
            .entries
            .iter()
            .filter(|(entry_path, _kind)| entry_path.parent() == Some(path))
            .map(|(entry_path, kind)| {
                DirectoryEntry::new(
                    entry_path
                        .file_name()
                        .expect("fixture entry has a basename"),
                    *kind,
                )
            })
            .collect();
        Ok(entries)
    }
}

#[cfg(unix)]
fn symlink_directory(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("directory symlink is created");
}

#[cfg(windows)]
fn symlink_directory(target: &Path, link: &Path) {
    std::os::windows::fs::symlink_dir(target, link).expect("directory symlink is created");
}

#[test]
fn defaults_resolve_and_an_ordinary_direct_child_is_a_candidate() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Orders")
        .directory("/package/Orders/First Order");
    let config = Config::from_toml_bytes(b"").expect("default config parses");

    let layout = inspect_package_layout(Path::new("/package"), &config, &filesystem)
        .expect("default layout is safe");

    assert_eq!(layout.root(), Path::new("/package"));
    assert_eq!(layout.orders_directory(), Path::new("/package/Orders"));
    assert_eq!(
        layout.removed_orders_directory(),
        Path::new("/package/Removed")
    );
    assert_eq!(layout.direct_children().len(), 1);
    assert_eq!(
        layout.direct_children()[0].classification(),
        DirectChildClassification::OrderCandidate
    );
    assert_eq!(
        layout.direct_children()[0].path(),
        Path::new("/package/Orders/First Order")
    );
}

#[test]
fn unsafe_configured_path_shapes_are_rejected_before_filesystem_traversal() {
    let filesystem = MemoryFileSystem::default().directory("/package");
    let cases = [
        (
            Config {
                orders_path: ".".into(),
                removed_path: "Removed".into(),
            },
            ConfiguredPath::Orders,
            ConfiguredPathFailure::CurrentDirectory,
        ),
        (
            Config {
                orders_path: "/outside".into(),
                removed_path: "Removed".into(),
            },
            ConfiguredPath::Orders,
            ConfiguredPathFailure::Absolute,
        ),
        (
            Config {
                orders_path: "C:/outside".into(),
                removed_path: "Removed".into(),
            },
            ConfiguredPath::Orders,
            ConfiguredPathFailure::Absolute,
        ),
        (
            Config {
                orders_path: "Orders".into(),
                removed_path: "C:outside".into(),
            },
            ConfiguredPath::Removed,
            ConfiguredPathFailure::Absolute,
        ),
        (
            Config {
                orders_path: "../outside".into(),
                removed_path: "Removed".into(),
            },
            ConfiguredPath::Orders,
            ConfiguredPathFailure::ParentTraversal,
        ),
        (
            Config {
                orders_path: "Orders//Active".into(),
                removed_path: "Removed".into(),
            },
            ConfiguredPath::Orders,
            ConfiguredPathFailure::EmptyComponent,
        ),
        (
            Config {
                orders_path: "Orders\\Active".into(),
                removed_path: "Removed".into(),
            },
            ConfiguredPath::Orders,
            ConfiguredPathFailure::PlatformSeparator,
        ),
        (
            Config {
                orders_path: ".spielorders/Orders".into(),
                removed_path: "Removed".into(),
            },
            ConfiguredPath::Orders,
            ConfiguredPathFailure::ReservedMetadata,
        ),
        (
            Config {
                orders_path: "Orders".into(),
                removed_path: "Removed/Old".into(),
            },
            ConfiguredPath::Removed,
            ConfiguredPathFailure::NotDirectChild,
        ),
    ];

    for (config, expected_path, expected_failure) in cases {
        let error = inspect_package_layout(Path::new("/package"), &config, &filesystem)
            .expect_err("unsafe configuration must fail");
        assert!(
            matches!(
                error,
                PackagePathError::InvalidConfiguration { path, failure }
                    if path == expected_path && failure == expected_failure
            ),
            "unexpected error: {error:?}"
        );
    }
}

#[test]
fn orders_and_removed_paths_cannot_overlap() {
    let filesystem = MemoryFileSystem::default().directory("/package");

    for config in [
        Config {
            orders_path: "Removed/Active".into(),
            removed_path: "Removed".into(),
        },
        Config {
            orders_path: "Orders".into(),
            removed_path: "Orders".into(),
        },
    ] {
        assert!(matches!(
            inspect_package_layout(Path::new("/package"), &config, &filesystem),
            Err(PackagePathError::ManagedPathsOverlap)
        ));
    }
}

#[test]
fn nested_orders_resolve_but_an_existing_removed_path_must_be_an_ordinary_directory() {
    let config = Config {
        orders_path: "Purchases/Active".into(),
        removed_path: "Retired".into(),
    };
    let valid = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Purchases")
        .directory("/package/Purchases/Active")
        .directory("/package/Retired");

    let layout = inspect_package_layout(Path::new("/package"), &config, &valid)
        .expect("safe nested Orders and ordinary Removed paths resolve");
    assert_eq!(
        layout.orders_directory(),
        Path::new("/package/Purchases/Active")
    );

    for kind in [
        FileSystemEntryKind::File,
        FileSystemEntryKind::SymbolicLink,
        FileSystemEntryKind::Other,
    ] {
        let invalid = MemoryFileSystem::default()
            .directory("/package")
            .directory("/package/Purchases")
            .directory("/package/Purchases/Active")
            .entry("/package/Retired", kind);
        assert!(matches!(
            inspect_package_layout(Path::new("/package"), &config, &invalid),
            Err(PackagePathError::RemovedOrdersDirectoryNotOrdinary)
        ));
    }
}

#[test]
fn every_configured_orders_prefix_must_remain_inside_the_root() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/InternalOrders")
        .canonical_path("/package/Mount", "/outside")
        .canonical_path("/package/Mount/Orders", "/package/InternalOrders");
    let config = Config {
        orders_path: "Mount/Orders".into(),
        removed_path: "Removed".into(),
    };

    assert!(matches!(
        inspect_package_layout(Path::new("/package"), &config, &filesystem),
        Err(PackagePathError::OrdersDirectoryOutsideRoot)
    ));

    let temporary = TemporaryTree::new("package-paths");
    let root = temporary.path().join("package");
    let outside_orders = temporary.path().join("outside/Orders");
    std::fs::create_dir(&root).expect("real Package root is created");
    std::fs::create_dir_all(&outside_orders).expect("outside Orders target is created");
    symlink_directory(
        outside_orders.parent().expect("outside path has a parent"),
        &root.join("Mount"),
    );
    assert!(matches!(
        inspect_package_layout(&root, &config, &RealFileSystem),
        Err(PackagePathError::OrdersDirectoryOutsideRoot)
    ));
}

#[test]
fn direct_child_classification_never_follows_links() {
    let memory = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Orders")
        .directory("/package/Orders/Order")
        .entry("/package/Orders/.hidden", FileSystemEntryKind::SymbolicLink)
        .entry("/package/Orders/loose.txt", FileSystemEntryKind::File)
        .entry(
            "/package/Orders/LinkedOrder",
            FileSystemEntryKind::SymbolicLink,
        )
        .directory("/package/Orders/Order/Supporting")
        .entry(
            "/package/Orders/Order/Supporting/DeepLink",
            FileSystemEntryKind::SymbolicLink,
        );
    let config = Config::from_toml_bytes(b"").expect("default config parses");
    let memory_layout = inspect_package_layout(Path::new("/package"), &config, &memory)
        .expect("the double-backed layout is safe");
    assert_classifications(
        &memory_layout,
        &[
            (".hidden", DirectChildClassification::IgnoredHidden),
            (
                "LinkedOrder",
                DirectChildClassification::Disallowed(FileSystemEntryKind::SymbolicLink),
            ),
            ("Order", DirectChildClassification::OrderCandidate),
            (
                "loose.txt",
                DirectChildClassification::Disallowed(FileSystemEntryKind::File),
            ),
        ],
    );

    let temporary = TemporaryTree::new("package-paths");
    let root = temporary.path().join("package");
    let orders = root.join("Orders");
    let outside = temporary.path().join("outside");
    std::fs::create_dir_all(&orders).expect("real Orders directory is created");
    std::fs::create_dir(&outside).expect("outside target is created");
    symlink_directory(&outside, &orders.join("LinkedOrder"));

    let real_layout = inspect_package_layout(&root, &config, &RealFileSystem)
        .expect("a direct-child link is observed without being followed");
    let linked = real_layout
        .direct_children()
        .iter()
        .find(|entry| entry.path().file_name() == Some(OsStr::new("LinkedOrder")))
        .expect("the visible link is retained as an observation");
    assert_eq!(
        linked.classification(),
        DirectChildClassification::Disallowed(FileSystemEntryKind::SymbolicLink)
    );
}

fn assert_classifications(
    layout: &spielorders_core::paths::PackageLayout,
    expected: &[(&str, DirectChildClassification)],
) {
    let actual = layout
        .direct_children()
        .iter()
        .map(|entry| {
            (
                entry
                    .path()
                    .file_name()
                    .expect("direct child has a basename")
                    .to_string_lossy()
                    .into_owned(),
                entry.classification(),
            )
        })
        .collect::<Vec<_>>();
    let expected = expected
        .iter()
        .map(|(name, classification)| ((*name).to_owned(), *classification))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn only_package_and_immediate_order_metadata_subtrees_are_reserved() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Orders")
        .directory("/package/Orders/Order")
        .directory("/package/Orders/.hidden");
    let config = Config::from_toml_bytes(b"").expect("default config parses");
    let layout = inspect_package_layout(Path::new("/package"), &config, &filesystem)
        .expect("layout is safe");

    assert_eq!(
        layout.reserved_path_kind(Path::new("/package/.spielorders")),
        Some(ReservedPathKind::PackageMetadata)
    );
    assert_eq!(
        layout.reserved_path_kind(Path::new("/package/.spielorders/version.json")),
        Some(ReservedPathKind::PackageMetadata)
    );
    assert_eq!(
        layout.reserved_path_kind(Path::new("/package/Orders/Order/.spielorders/order.json")),
        Some(ReservedPathKind::OrderMetadata)
    );
    assert_eq!(
        layout.reserved_path_kind(Path::new(
            "/package/Orders/Order/Supporting/.spielorders/user.txt"
        )),
        None
    );
    assert_eq!(
        layout.reserved_path_kind(Path::new("/package/Orders/.hidden/.spielorders/order.json")),
        None
    );
}

#[test]
fn non_normalized_package_paths_are_reserved_only_when_they_remain_in_package_metadata() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Orders");
    let config = Config::from_toml_bytes(b"").expect("default config parses");
    let layout = inspect_package_layout(Path::new("/package"), &config, &filesystem)
        .expect("layout is safe");

    assert_eq!(
        layout.reserved_path_kind(Path::new("/package/.spielorders/../README.md")),
        None
    );
    assert_eq!(
        layout.reserved_path_kind(Path::new(
            "/package/.spielorders/../../package/.spielorders/version.json"
        )),
        None
    );
}

#[test]
fn non_normalized_order_paths_use_lexical_location_without_inspecting_supporting_material() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Orders")
        .directory("/package/Orders/Order")
        .directory("/package/Orders/Order/Supporting")
        .entry(
            "/package/Orders/Order/Supporting/Linked",
            FileSystemEntryKind::SymbolicLink,
        )
        .canonical_path(
            "/package/Orders/Order/Supporting/Linked",
            "/package/Orders/Order",
        );
    let config = Config::from_toml_bytes(b"").expect("default config parses");
    let layout = inspect_package_layout(Path::new("/package"), &config, &filesystem)
        .expect("layout is safe");

    assert_eq!(
        layout.reserved_path_kind(Path::new(
            "/package/Orders/Order/Supporting/../.spielorders/order.json"
        )),
        Some(ReservedPathKind::OrderMetadata)
    );
    assert_eq!(
        layout.reserved_path_kind(Path::new(
            "/package/Orders/Order/Supporting/Linked/.spielorders/order.json"
        )),
        None
    );
}

#[test]
fn package_locator_must_match_the_explicit_root_and_active_nested_placement() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Purchases")
        .directory("/package/Purchases/Active")
        .directory("/package/Purchases/Active/Order")
        .directory("/package/Purchases/Old")
        .directory("/package/Purchases/Old/Order");
    let config = Config {
        orders_path: "Purchases/Active".into(),
        removed_path: "Removed".into(),
    };
    let two_parents = fixture_package_root("../..");
    let three_parents = fixture_package_root("../../..");
    let four_parents = fixture_package_root("../../../..");

    validate_order_package_locator(
        Path::new("/package"),
        &config,
        Path::new("/package/Purchases/Active/Order"),
        &three_parents,
        &filesystem,
    )
    .expect("the nested locator reaches the explicit Package root");

    for (order_directory, locator, expected) in [
        (
            Path::new("/package/Purchases/Active/Order"),
            &two_parents,
            OrderLocatorError::DoesNotResolveToRoot,
        ),
        (
            Path::new("/package/Purchases/Active/Order"),
            &four_parents,
            OrderLocatorError::DoesNotResolveToRoot,
        ),
        (
            Path::new("/package/Purchases/Old/Order"),
            &three_parents,
            OrderLocatorError::NotActiveDirectChild,
        ),
    ] {
        let error = validate_order_package_locator(
            Path::new("/package"),
            &config,
            order_directory,
            locator,
            &filesystem,
        )
        .expect_err("a stale or mismatched locator must fail locally");
        assert_eq!(error, expected);
    }
}

#[test]
fn configured_paths_cannot_overlap_after_link_resolution() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/Removed")
        .canonical_path("/package/Orders", "/package/Removed");
    let config = Config::from_toml_bytes(b"").expect("default config parses");

    assert!(matches!(
        inspect_package_layout(Path::new("/package"), &config, &filesystem),
        Err(PackagePathError::ManagedPathsOverlap)
    ));
}

#[test]
fn configured_orders_cannot_resolve_onto_reserved_package_metadata() {
    let filesystem = MemoryFileSystem::default()
        .directory("/package")
        .directory("/package/.spielorders")
        .canonical_path("/package/Orders", "/package/.spielorders");
    let config = Config::from_toml_bytes(b"").expect("default config parses");

    assert!(matches!(
        inspect_package_layout(Path::new("/package"), &config, &filesystem),
        Err(PackagePathError::InvalidConfiguration {
            path: ConfiguredPath::Orders,
            failure: ConfiguredPathFailure::ReservedMetadata,
        })
    ));
}

fn fixture_package_root(locator: &str) -> spielorders_core::metadata::PackageRoot {
    let source = format!(
        r#"{{
  "schema_version": 1,
  "package_root": "{locator}",
  "order_id": "8b6e3f0a-8ec5-4d47-95e1-412652a56d16",
  "title": "Locator fixture",
  "date_ordered": "2026-07-19",
  "status": "in_preparation",
  "items": []
}}"#
    );
    OrderDocument::from_json_bytes(source.as_bytes())
        .expect("locator fixture parses")
        .package_root
        .spelled()
        .cloned()
        .expect("the locator fixture spells a locator")
}
