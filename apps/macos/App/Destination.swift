import CSpielOrders

extension DesktopModel {
    /// What the content column shows for the open Package, chosen in the sidebar.
    enum Destination: Hashable {
        case orders
        case vendors
        case values(ValueList)
        case projects
        case packageReadme

        /// The sidebar's fixed destinations in order, which View numbers from Command-1.
        static let fixed: [Destination] = [
            .orders, .vendors, .values(.orderers), .values(.institutions), .projects, .packageReadme,
        ]

        var title: String {
            switch self {
            case .orders: "Orders"
            case .vendors: "Vendors"
            case let .values(list): list.title
            case .projects: "Projects"
            case .packageReadme: "Package README"
            }
        }

        /// A stable identifier fragment for accessibility.
        var key: String {
            switch self {
            case .orders: "orders"
            case .vendors: "vendors"
            case let .values(list): list.key
            case .projects: "projects"
            case .packageReadme: "packageReadme"
            }
        }

        /// Whether the search field filters this destination's list through the library.
        var searchable: Bool {
            self == .orders || self == .vendors
        }

        /// File → New's title: the kind of record this destination lists.
        var newTitle: String {
            switch self {
            case .orders, .packageReadme: "New Order"
            case .vendors: "New Vendor"
            case let .values(list): "New \(list.singular)"
            case .projects: "New Project"
            }
        }

        /// Whether File → New can create a record here. Values and Projects wait on the
        /// library's support for adding them.
        var creates: Bool {
            self == .orders || self == .vendors
        }
    }

    /// An Order fact whose values a destination lists, each with the number of Orders using it.
    enum ValueList: Hashable {
        case orderers
        case institutions

        /// The `SPIELORDERS_ORDER_FIELD_*` constant the values belong to.
        var field: Int32 {
            switch self {
            case .orderers: SPIELORDERS_ORDER_FIELD_ORDERER
            case .institutions: SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION
            }
        }

        var title: String {
            switch self {
            case .orderers: "Orderers"
            case .institutions: "Institutions"
            }
        }

        /// One value's kind.
        var singular: String {
            switch self {
            case .orderers: "Orderer"
            case .institutions: "Institution"
            }
        }

        var key: String {
            switch self {
            case .orderers: "orderers"
            case .institutions: "institutions"
            }
        }
    }
}
