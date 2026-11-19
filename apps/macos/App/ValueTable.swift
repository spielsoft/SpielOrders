import CSpielOrders
import SpielOrdersKit
import SwiftUI

/// What a row of a value table shows: a value, No Project above the groups of Orders with no
/// Project, or a new value being named.
enum ValueItem: Hashable {
    case value(ValueScope, String)
    case noProject
    case new(ValueScope)

    static func == (left: Self, right: Self) -> Bool {
        switch (left, right) {
        case let (.value(leftScope, leftName), .value(rightScope, rightName)):
            leftScope == rightScope && leftName.utf8.elementsEqual(rightName.utf8)
        case (.noProject, .noProject): true
        case let (.new(left), .new(right)): left == right
        default: false
        }
    }

    func hash(into hasher: inout Hasher) {
        switch self {
        case let .value(scope, name):
            hasher.combine(0)
            hasher.combine(scope)
            hasher.combine(Array(name.utf8))
        case .noProject: hasher.combine(1)
        case let .new(scope):
            hasher.combine(2)
            hasher.combine(scope)
        }
    }

    /// The value this row names, if it is one.
    var value: (scope: ValueScope, name: String)? {
        guard case let .value(scope, name) = self else { return nil }
        return (scope, name)
    }
}

/// A row of the Orderers, Institutions, or Projects table, with the rows beneath it.
struct ValueRow: Identifiable {
    let id: ValueItem
    let name: String
    /// How many Orders use it; nil where the library does not count, such as No Project.
    let orderCount: Int?
    var children: [ValueRow] = []

    /// The Orders column's sort key.
    var usage: Int { orderCount ?? 0 }
}

extension ValueScope {
    /// One value's kind, as menus and sheets name it.
    var noun: String {
        switch field {
        case SPIELORDERS_ORDER_FIELD_ORDERER: "Orderer"
        case SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION: "Institution"
        case SPIELORDERS_ORDER_FIELD_PROJECT: "Project"
        default: "Order Group"
        }
    }

    var plural: String { noun + "s" }
}

/// A value's name, or the field that names it in place: Return or leaving the field commits,
/// Escape cancels.
struct ValueName: View {
    @Bindable var model: DesktopModel
    let row: ValueRow
    let identifier: String

    var body: some View {
        if model.naming == row.id {
            NamingField(model: model, item: row.id, initial: row.name, identifier: identifier + ".naming")
        } else {
            Text(row.name)
                .foregroundStyle(row.orderCount == nil ? HierarchicalShapeStyle.secondary : .primary)
                .accessibleValue("Name", value: row.name, identifier: identifier)
        }
    }
}

/// The field a table row names a value with, its text selected so typing replaces it.
private struct NamingField: View {
    @Bindable var model: DesktopModel
    let item: ValueItem
    let identifier: String
    private let session: PackageSession
    @State private var local: NameDraft
    @FocusState private var focused: Bool

    init(model: DesktopModel, item: ValueItem, initial: String, identifier: String) {
        self.model = model
        self.item = item
        session = model.library
        _local = State(initialValue: NameDraft(initial, original: item.value?.name))
        self.identifier = identifier
    }

    private var draft: NameDraft { model.nameDraft(for: item, in: session) ?? local }

    var body: some View {
        TextField("Name", text: Binding(get: { draft.text }, set: {
            model.trackName(draft, for: item, in: session)?.text = $0
        }))
            .labelsHidden()
            .focused($focused)
            .onSubmit { focused = false; model.finishName(draft) }
            .onExitCommand { model.cancelName(draft) }
            .onChange(of: focused) { _, focused in
                if !focused { model.finishName(draft) }
            }
            .task {
                model.trackName(draft, for: item, in: session)
                focused = true
            }
            .onDisappear { model.finishName(draft) }
            .disabled(!model.canChangeValues)
            .accessibilityLabel("Name")
            .accessibilityIdentifier(identifier)
    }

}

/// The value commands for `items`: Rename and Delete… for one value, Merge N … for several of
/// one kind. A row's menu shows those that apply; the destination's menu shows every one,
/// disabling those that don't.
struct ValueMenu: View {
    @Bindable var model: DesktopModel
    let items: Set<ValueItem>
    /// The kind of value the table lists, which names the commands while none applies.
    let scope: ValueScope
    /// Whether commands that don't apply are shown disabled, as the menu bar shows them.
    var complete = false
    /// The menu's identifier, which the commands extend.
    let prefix: String

    var body: some View {
        let values = items.compactMap(\.value)
        let value = values.count == 1 ? values.first : nil
        let scopes = Set(values.map(\.scope))
        let merged = values.count > 1 && scopes.count == 1 ? scopes.first : nil
        let deletion = model.valueDeletion(items)
        if value != nil || complete {
            Button("Rename") {
                if let value { model.beginNaming(.value(value.scope, value.name)) }
            }
            .disabled(value == nil || !model.canChangeValues)
            .accessibilityIdentifier("\(prefix).rename")
        }
        if merged != nil || complete {
            Button("Merge \(merged == nil ? "" : "\(values.count) ")\((merged ?? scope).plural)…") {
                if let merged { model.mergeValues(values.map(\.name).sorted(), in: merged) }
            }
            .disabled(merged == nil || !model.canChangeValues)
            .accessibilityIdentifier("\(prefix).merge")
        }
        if value != nil || complete {
            Divider()
            Button("Delete \((value?.scope ?? scope).noun)…") { deletion?() }
                .disabled(deletion == nil)
                .accessibilityIdentifier("\(prefix).delete")
        }
    }
}
