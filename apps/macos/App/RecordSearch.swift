import CSpielOrders
import SwiftUI
import SpielOrdersKit

/// Where the search field looks: every fact, one fact, or (for Orders) Folder Contents.
enum SearchScope: Hashable {
    case allFields
    case field(Int32)
    case folderContents

    var field: Int32? {
        guard case let .field(field) = self else { return nil }
        return field
    }

    /// The facts the destination's list can be searched in, in menu order.
    static func fields(in destination: DesktopModel.Destination) -> [Int32] {
        switch destination {
        case .orders: PackageSession.orderSearchFields
        case .vendors: PackageSession.vendorSearchFields
        case .values, .projects, .packageReadme: []
        }
    }

    func isOffered(in destination: DesktopModel.Destination) -> Bool {
        switch self {
        case .allFields: true
        case let .field(field): Self.fields(in: destination).contains(field)
        case .folderContents: destination == .orders
        }
    }

    /// Where the search looks, as No Results names it.
    var name: String {
        switch self {
        case .allFields: "All Fields"
        case let .field(field): Self.label(field)
        case .folderContents: "Folder Contents"
        }
    }

    func placeholder(in destination: DesktopModel.Destination) -> String {
        switch self {
        case .allFields: destination == .vendors ? "Search Vendors" : "Search Orders"
        case let .field(field): "Search \(Self.label(field))"
        case .folderContents: "Search Folder Contents"
        }
    }

    /// A fact's name in the menu. Item and Contact facts say whose they are, since a Vendor
    /// and its Contacts share names such as Phone.
    static func label(_ field: Int32) -> String {
        switch field {
        case SPIELORDERS_ITEM_FIELD_DESCRIPTION,
             SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER,
             SPIELORDERS_ITEM_FIELD_QUANTITY,
             SPIELORDERS_ITEM_FIELD_UNIT_PRICE,
             SPIELORDERS_ITEM_FIELD_NOTES:
            "Item " + RecordFieldLabel.of(field)
        case SPIELORDERS_CONTACT_FIELD_NAME,
             SPIELORDERS_CONTACT_FIELD_ROLE,
             SPIELORDERS_CONTACT_FIELD_PHONE,
             SPIELORDERS_CONTACT_FIELD_EMAIL:
            "Contact " + RecordFieldLabel.of(field)
        default:
            RecordFieldLabel.of(field)
        }
    }
}

/// What the search field asks the library for.
struct RecordQuery: Equatable {
    var text = ""
    var scope = SearchScope.allFields
    var matchCase = false

    /// Whether Spotlight is asked for Supporting Material.
    var searchesFiles: Bool { scope == .folderContents && !text.isEmpty }

    /// Whether Search In or Match Case narrows the search from its default.
    var narrowed: Bool { scope != .allFields || matchCase }

    /// Where the search looks, as No Results and the Search In button say it.
    var extent: String { scope.name + (matchCase ? ", matching case" : "") }
}

/// The search field above the list it filters, with its Search In menu beside it.
struct RecordSearch: View {
    /// Identifies the field, for Edit → Find and accessibility.
    static let field = "search.query"

    @Bindable var model: DesktopModel

    var body: some View {
        HStack(spacing: 6) {
            SearchField(placeholder: model.search.scope.placeholder(in: model.destination),
                        text: Binding(get: { model.search.text }, set: model.updateQuery),
                        identifier: Self.field)
                .frame(minWidth: 120)
            // A fixed slot, so Spotlight's progress or problem never moves the field.
            ZStack {
                ProgressView()
                    .controlSize(.small)
                    .opacity(model.contents.gathering ? 1 : 0)
                    .accessibilityLabel("Searching Folder Contents")
                    .accessibilityHidden(!model.contents.gathering)
                    .accessibilityIdentifier("search.progress")
                if let problem = model.contents.problem, !model.contents.gathering {
                    ProblemBadge(level: .warning, title: "Folder Contents Incomplete", lines: [problem.summary],
                                 actions: actions(problem), identifier: "search.problem")
                }
            }
            .frame(width: 16)
            if model.contents.gathering {
                Button("Stop", action: model.contents.stopGathering)
                    .disabled(model.contents.stopping)
                    .help("Stop Spotlight and show its available matches")
                    .accessibilityLabel("Stop Folder Contents Search")
                    .accessibilityIdentifier("search.stop")
            }
            SearchInMenu(model: model)
        }
        .disabled(model.busy && !model.refreshingLookup)
    }

    private func actions(_ problem: Notice) -> [NoticeAction] {
        guard !problem.details.isEmpty else { return [] }
        return [NoticeAction("Details", identifier: "search.problem.details") { [model] in
            model.showDetails([problem])
        }]
    }
}

extension DesktopModel {
    /// Whether Edit → Find has a search field to focus: the destination's list is searchable
    /// and its field accepts input.
    var canFind: Bool {
        guard root != nil, !diagnosticWorkspace, destination.searchable, canBrowseRecords else { return false }
        return vendorSheet == nil && (!busy || refreshingLookup)
    }
}

private struct SearchInMenu: View {
    @Bindable var model: DesktopModel

    var body: some View {
        Menu {
            Section("Search In") {
                choice(SearchScope.allFields.name, .allFields, identifier: "search.scope.allFields")
                ForEach(SearchScope.fields(in: model.destination), id: \.self) { field in
                    choice(SearchScope.label(field), .field(field),
                           identifier: "search.scope.\(RecordFieldLabel.key(field))")
                }
                if model.destination == .orders {
                    Divider()
                    choice("Folder Contents (Spotlight)", .folderContents, identifier: "search.scope.folderContents")
                }
            }
            Divider()
            Toggle(matchCaseLabel, isOn: Binding(get: { model.search.matchCase }, set: model.setMatchCase))
                .accessibilityIdentifier("search.matchCase")
        } label: {
            Image(systemName: model.search.narrowed
                ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease")
        }
        // Filled in the accent color while Search In or Match Case narrows the search.
        .tint(model.search.narrowed ? .accentColor : nil)
        .menuStyle(.button)
        .buttonStyle(.borderless)
        .buttonBorderShape(.circle)
        .menuIndicator(.hidden)
        .fixedSize()
        .help(model.search.narrowed ? "Search In: \(model.search.extent)" : "Search In")
        .accessibilityLabel("Search In")
        .accessibilityValue(model.search.extent)
        .accessibilityIdentifier("search.scope")
    }

    /// Spotlight matches files its own way, so Match Case narrows only the facts.
    private var matchCaseLabel: String {
        model.search.scope == .folderContents ? "Match Case in Facts" : "Match Case"
    }

    private func choice(_ title: String, _ scope: SearchScope, identifier: String) -> some View {
        Toggle(title, isOn: Binding(get: { model.search.scope == scope }, set: { if $0 { model.setScope(scope) } }))
            .accessibilityIdentifier(identifier)
    }
}
