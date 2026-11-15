import CSpielOrders

extension DraftProblem {
    /// Whether the problem is a value the Package lacks, which adding that value fixes.
    public var lacksValue: Bool {
        [SPIELORDERS_FAILURE_UNCONFIRMED_REUSABLE_VALUE, SPIELORDERS_FAILURE_UNKNOWN_VENDOR].contains(code)
    }

    /// Whether another Vendor already has the Name the draft gives its Vendor, which merging
    /// the two Vendors resolves.
    public var nameTaken: Bool {
        code == SPIELORDERS_FAILURE_VENDOR_NAME_IN_USE
    }

    /// The problem in plain words for the field it marks, such as “Quantity must be a number.”
    /// The words follow the library's problem code; one the app has no plainer words for keeps
    /// the library's message. `label` names the marked field as the editor shows it.
    public func statement(for label: String) -> String {
        switch code {
        case SPIELORDERS_FAILURE_INVALID_ASSIGNMENTS:
            refusedAssignment(label)
        case SPIELORDERS_FAILURE_MISSING_REQUIRED_FACTS, SPIELORDERS_FAILURE_MISSING_REQUIRED_ITEM_FACTS:
            labeled(label, "\(label) is required.")
        case SPIELORDERS_FAILURE_MISSING_REQUIRED_NAME:
            "Vendor Name is required."
        case SPIELORDERS_FAILURE_INVALID_FACT_VALUE:
            unheldValue(label)
        case SPIELORDERS_FAILURE_UNCONFIRMED_REUSABLE_VALUE:
            labeled(label, "This \(label) isn’t in the Package yet.")
        case SPIELORDERS_FAILURE_UNKNOWN_VENDOR:
            "No Vendor has this name."
        case SPIELORDERS_FAILURE_VENDOR_NAME_IN_USE:
            "Another Vendor already has this name."
        case SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED, SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED:
            "It changed on disk after it was read."
        case SPIELORDERS_FAILURE_DESTINATION_COLLISION:
            "The Orders folder already holds a folder with this Order’s name."
        default:
            message
        }
    }

    private func refusedAssignment(_ label: String) -> String {
        switch reason {
        case SPIELORDERS_REASON_BLANK_REQUIRED_VALUE: labeled(label, "\(label) is required.")
        case SPIELORDERS_REASON_NOT_A_FINITE_NUMBER: labeled(label, "\(label) must be a number.")
        default: message
        }
    }

    private func unheldValue(_ label: String) -> String {
        switch field {
        case SPIELORDERS_ORDER_FIELD_DATE_ORDERED: labeled(label, "\(label) must be a real date.")
        case SPIELORDERS_ORDER_FIELD_STATUS: labeled(label, "\(label) must be one of the Order statuses.")
        case SPIELORDERS_ORDER_FIELD_CURRENCY: labeled(label, "\(label) must be a three-letter code, such as USD.")
        default: labeled(label, "\(label) isn’t a value it can hold.")
        }
    }

    /// `sentence`, which names the field, or the library's message when the field has no label.
    private func labeled(_ label: String, _ sentence: String) -> String {
        label.isEmpty ? message : sentence
    }
}
