//! Planning and persistence for initializing one unmanaged SpielOrders Package.

use crate::{
    Answer, Failure,
    change::PlannedAct,
    filesystem::staging_path,
    journal::{CreationAct, Journal, planned_location},
    metadata::{IndexDocument, StoredValues, VendorsDocument, Version1, VersionDocument},
    mutation::{CommitFailure, MutationReport},
    operations::{FailedOperation, FailedPath, refuse_linked_root},
    paths::{
        ConfiguredPath, ManagedDirectory, paths_overlap, resolve_managed_directory,
        resolved_paths_overlap, validate_configured_path,
    },
    ports::{FileSystem, FileSystemEntryKind},
    readme::Location,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InitializationOptions {
    orders_path: String,
    removed_path: String,
}

impl InitializationOptions {
    pub fn new(orders_path: impl Into<String>, removed_path: impl Into<String>) -> Self {
        Self {
            orders_path: orders_path.into(),
            removed_path: removed_path.into(),
        }
    }

    /// The configured Orders Directory, exactly as supplied.
    ///
    /// Exposed so that a caller which overrides one of the two paths can take the other
    /// from [`Self::default`] rather than repeating the conventional name. The
    /// conventional names are this type's own default and are spelled nowhere else.
    pub fn orders_path(&self) -> &str {
        &self.orders_path
    }

    /// The configured Removed Orders Directory, exactly as supplied.
    pub fn removed_path(&self) -> &str {
        &self.removed_path
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::new("Orders", "Removed")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageInitializationPlan {
    orders_directory: PathBuf,
    removed_directory: PathBuf,
    pub(crate) journal: Journal,
}

pub(crate) type PackageInitializationReport = MutationReport<()>;

pub(crate) fn plan_package_initialization<F: FileSystem>(
    selected_root: &Path,
    options: &InitializationOptions,
    filesystem: &F,
) -> Answer<PackageInitializationPlan> {
    Answer::of(|| {
        refuse_linked_root(selected_root, filesystem)?;
        let selected_kind = observe(selected_root, None, filesystem)?;
        if selected_kind != Some(FileSystemEntryKind::Directory) {
            return Err(Failure::RootNotOrdinaryDirectory);
        }
        let root = filesystem.canonicalize(selected_root).map_err(|error| {
            Failure::from(FailedOperation::filesystem(
                FailedPath::of(selected_root),
                &error,
                filesystem,
            ))
        })?;
        // The root the refusals below name. They refuse before any Package exists here, so
        // there is nothing for the root to be relative to and it carries no spelling.
        let root_location = Location::absolute(root.clone())
            .expect("a canonicalized selected root is an absolute path");
        if observe(&root.join(".spielorders"), Some(&root_location), filesystem)?.is_some() {
            // The one refusal above that is about a Package which *does* stand here, so the
            // root has a Package to be relative to and spells itself the way a loaded one
            // does. One question, one answer.
            return Err(Failure::AlreadyAPackage {
                root: Location::within_package(root.clone(), ".")
                    .expect("a canonicalized selected root is an absolute path"),
            });
        }

        let orders_components =
            validate_configured_path(&options.orders_path, false).map_err(|failure| {
                Failure::InvalidConfiguredPath {
                    root: root_location.clone(),
                    path: ConfiguredPath::Orders,
                    failure,
                }
            })?;
        let removed_components =
            validate_configured_path(&options.removed_path, true).map_err(|failure| {
                Failure::InvalidConfiguredPath {
                    root: root_location.clone(),
                    path: ConfiguredPath::Removed,
                    failure,
                }
            })?;
        if paths_overlap(&orders_components, &removed_components) {
            return Err(Failure::ManagedPathsOverlap {
                root: Box::new(root_location),
            });
        }
        for (configured, component) in [
            (ConfiguredPath::Orders, orders_components[0]),
            (ConfiguredPath::Removed, removed_components[0]),
        ] {
            if component.eq_ignore_ascii_case("README.md")
                || component.eq_ignore_ascii_case(".README.md.spielorders-stage")
            {
                return Err(Failure::PlannedPathCollision {
                    root: root_location,
                    configured,
                    path: planned_location(&root, root.join(component)),
                });
            }
        }

        let act = PlannedAct::Creation;
        let mut journal = Journal::new(&root);
        journal.require(root.clone(), Some(FileSystemEntryKind::Directory));
        let mut created_files = Vec::new();
        journal.create_directory(root.join(".spielorders"), act(CreationAct::CreateDirectory));
        let mut orders_directory = root.clone();
        for component in orders_components {
            orders_directory.push(component);
            match observe(&orders_directory, Some(&root_location), filesystem)? {
                None => journal
                    .create_directory(orders_directory.clone(), act(CreationAct::CreateDirectory)),
                Some(FileSystemEntryKind::Directory) => {
                    let resolved = filesystem
                        .canonicalize(&orders_directory)
                        .map_err(|error| {
                            Failure::from(
                                FailedOperation::filesystem(
                                    FailedPath::of(&orders_directory),
                                    &error,
                                    filesystem,
                                )
                                .under(&root_location),
                            )
                        })?;
                    if !resolved.starts_with(&root) {
                        return Err(Failure::DisallowedLink {
                            root: root_location,
                            path: planned_location(&root, orders_directory),
                        });
                    }
                    journal.preserve(
                        orders_directory.clone(),
                        FileSystemEntryKind::Directory,
                        act(CreationAct::PreserveDirectory),
                    );
                }
                Some(FileSystemEntryKind::SymbolicLink) => {
                    return Err(Failure::DisallowedLink {
                        root: root_location,
                        path: planned_location(&root, orders_directory),
                    });
                }
                Some(actual) => {
                    return Err(Failure::DestinationCollision {
                        root: root_location,
                        location: planned_location(&root, orders_directory),
                        found: Some(actual),
                    });
                }
            }
        }

        let removed_directory = root.join(removed_components[0]);
        let removed_kind = observe(&removed_directory, Some(&root_location), filesystem)?;
        // Two configured paths are two directories or one, and the filesystem is the only
        // thing that knows which. Both must already be there for it to answer: a path
        // with nothing at it is not the directory the other one resolves to, and where
        // neither exists yet the question has no answer until initialization creates the
        // Orders Directory, which is where the commit asks it again.
        match managed_paths_collide(&orders_directory, &removed_directory, filesystem) {
            Ok(false) => {}
            Ok(true) => {
                return Err(Failure::ManagedPathsOverlap {
                    root: Box::new(root_location),
                });
            }
            Err((path, error)) => {
                return Err(Failure::from(
                    FailedOperation::filesystem(FailedPath::of(&path), &error, filesystem)
                        .under(&root_location),
                ));
            }
        }
        match removed_kind {
            None => {}
            Some(FileSystemEntryKind::Directory) => {
                journal.preserve(
                    removed_directory.clone(),
                    FileSystemEntryKind::Directory,
                    act(CreationAct::PreserveDirectory),
                );
            }
            Some(FileSystemEntryKind::SymbolicLink) => {
                return Err(Failure::DisallowedLink {
                    root: root_location,
                    path: planned_location(&root, removed_directory),
                });
            }
            Some(actual) => {
                return Err(Failure::DestinationCollision {
                    root: root_location,
                    location: planned_location(&root, removed_directory),
                    found: Some(actual),
                });
            }
        }

        let metadata = root.join(".spielorders");
        let config = config_bytes(options);
        let version = VersionDocument {
            metadata_version: Version1,
        }
        .to_json_bytes()
        .expect("version 1 marker serialization is infallible");
        let vendors = VendorsDocument {
            schema_version: Version1,
            vendors: Vec::new(),
            values: StoredValues::default(),
        }
        .to_json_bytes()
        .expect("empty version 1 Vendors serialization is infallible");
        let index = IndexDocument {
            schema_version: Version1,
            orders: Vec::new(),
            vendors: Vec::new(),
            values: StoredValues::default(),
        }
        .to_json_bytes()
        .expect("empty version 1 Package Index serialization is infallible");
        for (name, bytes) in [
            ("config.toml", config),
            ("version.json", version),
            ("vendors.json", vendors),
            ("index.json", index),
        ] {
            created_files.push(metadata.join(name));
            journal.create_file(metadata.join(name), bytes, act(CreationAct::CreateFile));
        }

        let readme = root.join("README.md");
        match observe(&readme, Some(&root_location), filesystem)? {
            None => {
                created_files.push(readme.clone());
                journal.create_file(readme, Vec::new(), act(CreationAct::CreateFile));
            }
            Some(FileSystemEntryKind::File) => journal.preserve(
                readme,
                FileSystemEntryKind::File,
                act(CreationAct::PreserveFile),
            ),
            Some(FileSystemEntryKind::SymbolicLink) => {
                return Err(Failure::DisallowedLink {
                    root: root_location,
                    path: planned_location(&root, readme),
                });
            }
            Some(actual) => {
                return Err(Failure::DestinationCollision {
                    root: root_location,
                    location: planned_location(&root, readme),
                    found: Some(actual),
                });
            }
        }

        for file in &created_files {
            let stage = staging_path(file).expect("planned creation files have basenames");
            if let Some(actual) = observe(&stage, Some(&root_location), filesystem)? {
                return Err(Failure::DestinationCollision {
                    root: root_location,
                    location: planned_location(&root, stage),
                    found: Some(actual),
                });
            }
        }
        journal.require(removed_directory.clone(), removed_kind);
        journal.require_planned();

        Ok(PackageInitializationPlan {
            orders_directory,
            removed_directory,
            journal,
        })
    })
}

/// Whether the two configured managed paths are one directory on the filesystem, or
/// one holds the other, as it resolves them right now.
///
/// Package loading asks the same question of the same resolution, so a configuration
/// this accepts is one that loading accepts too. Keeping the two answers in one place
/// is what stops initialization minting a Package its own loader then refuses.
///
/// A failure carries the path it was observing, so neither caller has to guess which
/// of the two managed paths the filesystem refused to answer for.
fn managed_paths_collide<F: FileSystem>(
    orders_directory: &Path,
    removed_directory: &Path,
    filesystem: &F,
) -> Result<bool, (PathBuf, F::Error)> {
    let orders = resolve_managed_directory(orders_directory, filesystem)
        .map_err(|(_, error)| (orders_directory.to_path_buf(), error))?;
    let removed = resolve_managed_directory(removed_directory, filesystem)
        .map_err(|(_, error)| (removed_directory.to_path_buf(), error))?;
    let (ManagedDirectory::Resolved(orders), ManagedDirectory::Resolved(removed)) =
        (orders, removed)
    else {
        return Ok(false);
    };
    Ok(resolved_paths_overlap(&orders, &removed))
}

pub(crate) fn commit_package_initialization<F: FileSystem>(
    plan: PackageInitializationPlan,
    filesystem: &F,
) -> Answer<PackageInitializationReport> {
    Answer::of(|| {
        let root = plan.journal.root();
        let committed = plan
            .journal
            .run_checking(filesystem, |directory, filesystem| {
                if directory != plan.orders_directory {
                    return None;
                }
                match managed_paths_collide(
                    &plan.orders_directory,
                    &plan.removed_directory,
                    filesystem,
                ) {
                    Ok(false) => None,
                    Ok(true) => Some(CommitFailure::new(planned_location(
                        root,
                        plan.removed_directory.clone(),
                    ))),
                    Err((path, _)) => Some(CommitFailure::new(planned_location(root, path))),
                }
            })?;
        Ok(committed.report(()))
    })
}

/// What stands at `path`, or the typed refusal the filesystem gave instead.
///
/// `root` is the resolved selected root where the caller already has one, so a refusal
/// arriving after the resolution names it.
fn observe<F: FileSystem>(
    path: &Path,
    root: Option<&Location>,
    filesystem: &F,
) -> Result<Option<FileSystemEntryKind>, Failure> {
    filesystem.symlink_metadata(path).map_err(|error| {
        let failed = FailedOperation::filesystem(FailedPath::of(path), &error, filesystem);
        Failure::from(match root {
            Some(root) => failed.under(root),
            None => failed,
        })
    })
}

fn config_bytes(options: &InitializationOptions) -> Vec<u8> {
    let mut table = toml::Table::new();
    if options.orders_path != "Orders" {
        table.insert(
            "orders_path".into(),
            toml::Value::String(options.orders_path.clone()),
        );
    }
    if options.removed_path != "Removed" {
        table.insert(
            "removed_path".into(),
            toml::Value::String(options.removed_path.clone()),
        );
    }
    toml::to_string(&table)
        .expect("string-only Package configuration serialization is infallible")
        .into_bytes()
}
