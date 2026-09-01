//! Shared result vocabulary for side-effecting domain mutations.

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MutationReport<P, R, J, W> {
    Rejected {
        plan: P,
        rejection: J,
    },
    Unchanged {
        plan: P,
        result: R,
    },
    Committed {
        plan: P,
        result: R,
    },
    CommittedWithWarning {
        plan: P,
        result: R,
        warnings: Vec<W>,
    },
}

/// Why one canonical `field=value` assignment was refused. An atomic assignment
/// set reports every rejected field and changes nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RejectedAssignmentReason {
    UnknownField,
    ImmutableIdentity,
    ManagedTechnicalField,
    OrderedCollection,
    Duplicate,
    BlankRequiredValue,
    NotAReusableValue,
    NotAFiniteNumber,
}

impl RejectedAssignmentReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownField => "unknown_field",
            Self::ImmutableIdentity => "immutable_identity",
            Self::ManagedTechnicalField => "managed_technical_field",
            Self::OrderedCollection => "ordered_collection",
            Self::Duplicate => "duplicate",
            Self::BlankRequiredValue => "blank_required_value",
            Self::NotAReusableValue => "not_a_reusable_value",
            Self::NotAFiniteNumber => "not_a_finite_number",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedAssignment {
    field: String,
    reason: RejectedAssignmentReason,
}

impl RejectedAssignment {
    pub(crate) fn new(field: impl Into<String>, reason: RejectedAssignmentReason) -> Self {
        Self {
            field: field.into(),
            reason,
        }
    }

    pub fn field(&self) -> &str {
        &self.field
    }

    pub fn reason(&self) -> RejectedAssignmentReason {
        self.reason
    }
}
