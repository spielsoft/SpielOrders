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
            NamingField(initial: row.name, identifier: identifier + ".naming") { text in
                model.endNaming(row.id, as: text)
            } cancel: {
                model.cancelNaming()
            }
        } else {
            Text(row.name)
                .foregroundStyle(row.orderCount == nil ? HierarchicalShapeStyle.secondary : .primary)
                .accessibleValue("Name", value: row.name, identifier: identifier)
        }
    }
}

/// The field a table row names a value with, its text selected so typing replaces it.
private struct NamingField: View {
    let identifier: String
    let commit: (String) -> Void
    let cancel: () -> Void
    @State private var text: String
    @State private var ended = false
    @FocusState private var focused: Bool

    init(initial: String, identifier: String, commit: @escaping (String) -> Void, cancel: @escaping () -> Void) {
        _text = State(initialValue: initial)
        self.identifier = identifier
        self.commit = commit
        self.cancel = cancel
    }

    var body: some View {
        TextField("Name", text: $text)
            .labelsHidden()
            .focused($focused)
            .onSubmit { end { commit(text) } }
            .onExitCommand { end(cancel) }
            .onChange(of: focused) { _, focused in
                if !focused { end { commit(text) } }
            }
            .task { focused = true }
            .accessibilityLabel("Name")
            .accessibilityIdentifier(identifier)
    }

    private func end(_ action: () -> Void) {
        guard !ended else { return }
        ended = true
        action()
    }
}

/// A value table's row menu: Rename and Delete… for one value, Merge N … for several of one kind.
struct ValueMenu: View {
    @Bindable var model: DesktopModel
    let items: Set<ValueItem>
    /// The table's identifier, which the items extend.
    let prefix: String

    var body: some View {
        let values = items.compactMap(\.value)
        let scopes = Set(values.map(\.scope))
        if values.count == 1, let value = values.first {
            Button("Rename") { model.beginNaming(.value(value.scope, value.name)) }
                .accessibilityIdentifier("\(prefix).rename")
            Button("Delete \(value.scope.noun)…") { model.deleteValue(value.name, in: value.scope) }
                .accessibilityIdentifier("\(prefix).delete")
        } else if values.count > 1, scopes.count == 1, let scope = scopes.first {
            Button("Merge \(values.count) \(scope.plural)…") {
                model.mergeValues(values.map(\.name).sorted(), in: scope)
            }
            .accessibilityIdentifier("\(prefix).merge")
        }
    }
}
