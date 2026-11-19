import SwiftUI
import SpielOrdersKit

/// The detail beside an Orderers, Institutions, or Projects list: the selected value, the
/// values selected to merge, or which kind of value to select.
struct ValueDetail: View {
    @Bindable var model: DesktopModel
    /// The kind of value the list holds, which names the detail while none is selected.
    let kind: ValueScope
    /// The list's rows, each with the rows beneath it.
    let rows: [ValueRow]
    /// Selects a row of the list, such as one of a Project's groups.
    var select: (ValueItem) -> Void

    var body: some View {
        let selection = model.valueSelection
        if selection.count > 1 {
            SelectedValues(model: model, kind: kind, items: selection)
        } else if let item = selection.first, let row = row(item) {
            SelectedValue(model: model, row: row, select: select)
                .id(item)
                .id(ObjectIdentifier(model.library))
        } else if !rows.isEmpty {
            ContentUnavailableView { Text("No \(kind.noun) Selected") }
                .accessibilityIdentifier("value.detail.placeholder")
        }
    }

    /// The listed row `item` names, unless it is a value being added.
    private func row(_ item: ValueItem) -> ValueRow? {
        if case .new = item { return nil }
        return (rows + rows.flatMap(\.children)).first { $0.id == item }
    }
}

extension ValueScope {
    /// The symbol of a value of this kind, as the destinations show it.
    fileprivate var symbol: String {
        if project != nil { return "tray.2" }
        return switch self {
        case .orderers: "person.2"
        case .institutions: "building.columns"
        default: "folder"
        }
    }
}

/// The detail for several selected values: how many, and Merge when they are of one kind.
private struct SelectedValues: View {
    @Bindable var model: DesktopModel
    let kind: ValueScope
    let items: Set<ValueItem>

    var body: some View {
        let values = items.compactMap(\.value)
        let scopes = Set(values.map(\.scope))
        let merged = values.count == items.count && scopes.count == 1 ? scopes.first : nil
        ContentUnavailableView {
            Label(merged.map { "\(items.count) \($0.plural) Selected" } ?? "\(items.count) Selected",
                  systemImage: (merged ?? kind).symbol)
        } description: {
            if merged != nil {
                Text("Merging asks which spelling to keep. Every Order using one of them uses it.")
            } else {
                Text("Select Projects, or Order Groups of one Project, to merge them.")
            }
        } actions: {
            if let merged {
                Button("Merge \(values.count) \(merged.plural)…") {
                    model.mergeValues(values.map(\.name).sorted(), in: merged)
                }
                .disabled(!model.canChangeValues)
                .accessibilityIdentifier("value.selection.merge")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(kind.noun) detail")
        .accessibilityIdentifier("value.selection")
    }
}

/// The detail for one value: its name, which renames it in place, the Order Groups of a
/// Project, and the Orders that use it.
private struct SelectedValue: View {
    @Bindable var model: DesktopModel
    let row: ValueRow
    let select: (ValueItem) -> Void
    @State private var orders: [OrderRow]?
    @State private var unlisted = false

    /// The value the row names; nil for No Project, which names none.
    private var value: (scope: ValueScope, name: String)? { row.id.value }

    var body: some View {
        DetailScroll {
            VStack(alignment: .leading, spacing: 10) {
                RecordHeading(title: row.name, summary: summary, prefix: "value.detail") { EmptyView() }
                if let value {
                    Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 12) {
                        GridRow {
                            Text("Name").gridColumnAlignment(.trailing)
                            ValueNameField(model: model, value: value.name, scope: value.scope)
                        }
                    }
                    .frame(maxWidth: 700, alignment: .leading)
                }
            }
            if value == nil || value?.scope == .projects {
                Divider()
                GroupsTable(model: model, scope: .groups(of: value?.name ?? ""),
                            groups: row.children.filter { $0.id.value != nil }, select: select)
            }
            if value != nil {
                Divider()
                if let orders {
                    RelatedOrders(model: model, orders: orders,
                                  columns: [.title, .date, .status, .vendor, .total],
                                  label: "\(value?.scope.noun ?? "") Orders", prefix: "value")
                } else if unlisted {
                    DetailSection("Orders") {
                        Text("The Orders couldn’t be listed. Reload the Package, then try again.")
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
        // Every change, Undo, and reload lists the Orders again, so the table follows them.
        .task(id: model.ordersRevision) { await listOrders() }
        .accessibilityLabel("\(value?.scope.noun ?? "Project") detail")
        .accessibilityIdentifier("value.detail")
    }

    /// How many Orders use the value; for an Order Group, its Project first.
    private var summary: String {
        guard let value else {
            let count = row.children.count { $0.id.value != nil }
            return count == 1 ? "1 Order Group" : "\(count) Order Groups"
        }
        let orders = OrderCount.text(row.usage)
        guard let project = value.scope.project else { return orders }
        let owner = project.isEmpty ? "Order Group with No Project" : "Order Group in \(project)"
        return "\(owner) · \(orders)"
    }

    private func listOrders() async {
        guard let value else { return }
        do {
            let listed = try await model.orders(using: value.name, in: value.scope)
            try Task.checkCancellation()
            orders = listed
            unlisted = false
        } catch {
            guard !Task.isCancelled else { return }
            orders = nil
            unlisted = true
        }
    }
}

/// The detail's Name field: Return ends editing and leaving it renames the value, once, as
/// renaming its row does; Escape restores the name.
private struct ValueNameField: View {
    @Bindable var model: DesktopModel
    let value: String
    let scope: ValueScope
    private let session: PackageSession
    @State private var local: NameDraft
    @FocusState private var focused: Bool

    init(model: DesktopModel, value: String, scope: ValueScope) {
        self.model = model
        self.value = value
        self.scope = scope
        session = model.library
        _local = State(initialValue: NameDraft(value, original: value))
    }

    private var item: ValueItem { .value(scope, value) }
    private var draft: NameDraft { model.nameDraft(for: item, in: session) ?? local }

    var body: some View {
        TextField("Name", text: Binding(get: { draft.text }, set: {
            model.trackName(draft, for: item, in: session)?.text = $0
        }))
            .labelsHidden()
            .focused($focused)
            .onSubmit { focused = false }
            .onExitCommand { model.cancelName(draft) }
            .onChange(of: focused) { _, focused in
                if !focused { model.finishName(draft) }
            }
            .onChange(of: model.busy) { _, busy in
                if !busy, !focused { draft.read(value) }
            }
            .onDisappear { model.finishName(draft) }
            .disabled(!model.canChangeValues)
            .accessibilityLabel("Name")
            .accessibilityIdentifier("value.detail.name")
    }
}

/// A Project's Order Groups with the Orders using each, every one shown. Selecting one selects
/// it in the list; + names a new one in the table.
private struct GroupsTable: View {
    @Bindable var model: DesktopModel
    /// The groups of this Project, or of the Orders with no Project.
    let scope: ValueScope
    let groups: [ValueRow]
    let select: (ValueItem) -> Void
    @State private var selection: ValueItem?

    /// The groups, then one being named.
    private var rows: [ValueRow] {
        guard model.naming == .new(scope) else { return groups }
        return groups + [ValueRow(id: .new(scope), name: "New Order Group", orderCount: 0)]
    }

    var body: some View {
        let rows = rows
        DetailSection("Order Groups") {
            Table(rows, selection: $selection) {
                TableColumn("Name") { group in
                    ValueName(model: model, row: group, identifier: "value.group.\(group.name).name")
                        .lineLimit(1)
                }
                .width(min: 120, ideal: 240)
                TableColumn("Orders") { group in
                    Text(group.usage, format: .number)
                        .monospacedDigit()
                        .accessibleValue("Orders", value: String(group.usage),
                                         identifier: "value.group.\(group.name).orders")
                }
                .width(min: 60, ideal: 80, max: 120)
                .alignment(.numeric)
            }
            .tableStyle(.bordered(alternatesRowBackgrounds: true))
            .frame(height: CompactTable.height(rows: max(rows.count, 2), limit: nil))
            .overlay {
                if rows.isEmpty {
                    Text("No Order Groups")
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("value.groups.placeholder")
                }
            }
            .onChange(of: selection) { _, item in
                if let item, item.value != nil { select(item) }
            }
            .accessibilityLabel("Order Groups")
            .accessibilityIdentifier("value.groups")
            RowActions(noun: "Order Group", prefix: "value.groups") { model.beginNaming(.new(scope)) }
                .disabled(!model.canStartNew || model.naming == .new(scope))
        }
    }
}

extension DesktopModel {
    /// The Orders that use the value `name` of `scope`, as the library lists them.
    func orders(using name: String, in scope: ValueScope) async throws -> [OrderRow] {
        let cancellation = LibraryCancellation()
        return try await withTaskCancellationHandler {
            try await library.orders(using: name, in: scope, cancellation: cancellation)
        } onCancel: {
            cancellation.request()
        }
    }
}
