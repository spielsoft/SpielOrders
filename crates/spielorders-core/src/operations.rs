//! Stable evidence for filesystem and Package Index operation failures.

use crate::readme::{EvidenceScope, Location};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OperationFailureCode {
    FilesystemOperationFailed,
    PackageIndexReplacementFailed,
}

impl OperationFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FilesystemOperationFailed => "filesystem_operation_failed",
            Self::PackageIndexReplacementFailed => "package_index_replacement_failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FileSystemOperation {
    Canonicalize,
    ObserveKind,
    ListDirectory,
    Read,
}

impl FileSystemOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Canonicalize => "canonicalize",
            Self::ObserveKind => "observe_kind",
            Self::ListDirectory => "list_directory",
            Self::Read => "read",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OperationEvidence {
    FileSystem {
        operation: FileSystemOperation,
    },
    PackageIndexReplacement {
        phase: PackageIndexReplacementPhase,
        cleanup_failure: Option<PackageIndexReplacementPhase>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PackageIndexReplacementPhase {
    WriteStage,
    ReadStage,
    CandidateValidation,
    SyncStage,
    Replace,
    RemoveStage,
}

impl PackageIndexReplacementPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WriteStage => "write_stage",
            Self::ReadStage => "read_stage",
            Self::CandidateValidation => "candidate_validation",
            Self::SyncStage => "sync_stage",
            Self::Replace => "replace",
            Self::RemoveStage => "remove_stage",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OperationFailure {
    code: OperationFailureCode,
    scope: EvidenceScope,
    primary_location: Location,
    related_locations: Vec<Location>,
    evidence: OperationEvidence,
}

impl OperationFailure {
    pub(crate) fn filesystem(
        operation: FileSystemOperation,
        primary_location: Location,
        related_locations: Vec<Location>,
    ) -> Self {
        Self {
            code: OperationFailureCode::FilesystemOperationFailed,
            scope: EvidenceScope::Operation,
            primary_location,
            related_locations,
            evidence: OperationEvidence::FileSystem { operation },
        }
    }

    pub(crate) fn package_index_replacement(
        primary_location: Location,
        related_locations: Vec<Location>,
        phase: PackageIndexReplacementPhase,
        cleanup_failure: Option<PackageIndexReplacementPhase>,
    ) -> Self {
        Self {
            code: OperationFailureCode::PackageIndexReplacementFailed,
            scope: EvidenceScope::Operation,
            primary_location,
            related_locations,
            evidence: OperationEvidence::PackageIndexReplacement {
                phase,
                cleanup_failure,
            },
        }
    }

    pub(crate) fn observe_kind(location: &Location) -> Self {
        Self::filesystem(
            FileSystemOperation::ObserveKind,
            location.clone(),
            Vec::new(),
        )
    }

    pub fn code(&self) -> OperationFailureCode {
        self.code
    }

    pub fn scope(&self) -> EvidenceScope {
        self.scope
    }

    pub fn primary_location(&self) -> &Location {
        &self.primary_location
    }

    pub fn related_locations(&self) -> &[Location] {
        &self.related_locations
    }

    pub fn evidence(&self) -> &OperationEvidence {
        &self.evidence
    }
}
