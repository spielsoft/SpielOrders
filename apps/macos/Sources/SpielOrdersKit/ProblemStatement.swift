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
        default: labeled(label, "\(label) has a value SpielOrders can’t use.")
        }
    }

    /// `sentence`, which names the field, or the library's message when the field has no label.
    private func labeled(_ label: String, _ sentence: String) -> String {
        label.isEmpty ? message : sentence
    }
}

extension LibraryEntry {
    /// The finding in plain words, such as “This Order’s file can’t be read.”, as the Order
    /// it is located in shows it when `inOrder`, and otherwise as the Package shows it. The
    /// words follow the library's finding code; one the app has no plainer words for keeps
    /// the library's message.
    func statement(inOrder: Bool) -> String {
        let file = inOrder ? "This Order’s file" : "One of the Package’s files"
        let folder = inOrder ? "This Order’s" : "The Package’s"
        return switch code {
        case SPIELORDERS_FINDING_MANAGED_ARTIFACT_MISSING:
            "\(file) is missing."
        case SPIELORDERS_FINDING_MANAGED_ARTIFACT_UNREADABLE:
            "\(file) can’t be opened."
        case SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_ENCODING, SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SYNTAX:
            "\(file) can’t be read."
        case SPIELORDERS_FINDING_MANAGED_ARTIFACT_INVALID_SHAPE, SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_KIND:
            "\(file) isn’t in the form SpielOrders expects."
        case SPIELORDERS_FINDING_MANAGED_ARTIFACT_WRONG_VERSION:
            "\(file) is from a version of SpielOrders this one doesn’t support."
        case SPIELORDERS_FINDING_ORDER_SEMANTIC_INVALID:
            "This Order’s file holds a value an Order can’t have."
        case SPIELORDERS_FINDING_ORDER_ID_DUPLICATE:
            "Another Order has the same Order ID."
        case SPIELORDERS_FINDING_ORDER_PACKAGE_LOCATOR_MISMATCH:
            "This Order’s link to its Package is out of date."
        case SPIELORDERS_FINDING_VENDOR_REFERENCE_UNRESOLVED:
            "This Order’s Vendor isn’t in the Package."
        case SPIELORDERS_FINDING_README_UNAVAILABLE:
            "\(folder) README can’t be read."
        case SPIELORDERS_FINDING_RESERVED_CONTENT_UNEXPECTED:
            "\(folder) hidden .spielorders folder holds something unexpected."
        case SPIELORDERS_FINDING_STAGING_ARTIFACT_PRESENT:
            "A save that didn’t finish left a file behind."
        case SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_DISALLOWED:
            "The Orders folder holds something that isn’t an Order."
        case SPIELORDERS_FINDING_ORDERS_DIRECTORY_ENTRY_NAME_UNUSABLE:
            "Something in the Orders folder has a name SpielOrders can’t use."
        case SPIELORDERS_FINDING_VENDOR_ID_DUPLICATE:
            "Two Vendors have the same Vendor ID."
        case SPIELORDERS_FINDING_VENDOR_NAME_DUPLICATE:
            "Two Vendors have the same Name."
        case SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_UNAVAILABLE:
            "The Package’s Orders folder can’t be found or opened."
        case SPIELORDERS_FINDING_CONFIGURATION_PATH_INVALID, SPIELORDERS_FINDING_CONFIGURATION_PATHS_OVERLAP,
             SPIELORDERS_FINDING_CONFIGURED_ORDERS_DIRECTORY_NAME_UNUSABLE:
            "The Package’s settings name a folder it can’t use."
        default:
            message
        }
    }
}
