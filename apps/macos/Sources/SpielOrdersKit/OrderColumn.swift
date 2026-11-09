import CSpielOrders

public enum OrderColumn: String, CaseIterable, Sendable {
    case date, status, title, vendor, orderer, institution, project, orderGroup, directory, currency, identity, total

    public var field: Int32 {
        switch self {
        case .date: SPIELORDERS_ORDER_FIELD_DATE_ORDERED
        case .status: SPIELORDERS_ORDER_FIELD_STATUS
        case .title: SPIELORDERS_ORDER_FIELD_TITLE
        case .vendor: SPIELORDERS_ORDER_FIELD_VENDOR
        case .orderer: SPIELORDERS_ORDER_FIELD_ORDERER
        case .institution: SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION
        case .project: SPIELORDERS_ORDER_FIELD_PROJECT
        case .orderGroup: SPIELORDERS_ORDER_FIELD_ORDER_GROUP
        case .directory: SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME
        case .currency: SPIELORDERS_ORDER_FIELD_CURRENCY
        case .identity: SPIELORDERS_ORDER_FIELD_ORDER_ID
        case .total: SPIELORDERS_ORDER_FIELD_TOTAL
        }
    }

    public var label: String {
        switch self {
        case .date: "Date"
        case .status: "Status"
        case .title: "Order Title"
        case .vendor: "Vendor"
        case .orderer: "Orderer"
        case .institution: "Institution"
        case .project: "Project"
        case .orderGroup: "Order Group"
        case .directory: "Directory Name"
        case .currency: "Currency"
        case .identity: "Order ID"
        case .total: "Total"
        }
    }

    public var initiallyVisible: Bool { [.date, .status, .title, .vendor, .orderer].contains(self) }

    public var initialWidth: Double {
        switch self {
        case .title, .directory: 250
        case .identity: 260
        case .date: 100
        case .status, .total: 110
        default: 140
        }
    }
}
