import SwiftUI
import SpielOrdersKit

/// One Order fact's values with the number of Orders using each, as the library lists them,
/// beside the selected value's detail, which takes the spare width.
struct ValuesWorkspace: View {
    @Bindable var model: DesktopModel
    let list: DesktopModel.ValueList
    @State private var sortOrder = [KeyPathComparator(\ValueRow.name)]

    private var rows: [ValueRow] {
        let scope = list.scope
        let listed = (model.reusableValues[list.field] ?? []).map { value in
            ValueRow(id: .value(scope, value.name), name: value.name, orderCount: value.orderCount)
        }
        let sorted = listed.sorted(using: sortOrder)
        guard model.naming == .new(scope) else { return sorted }
        return sorted + [ValueRow(id: .new(scope), name: "New \(list.singular)", orderCount: 0)]
    }

    var body: some View {
        let rows = rows
        WorkspaceSplit(content: ValueGrid(model: model, list: list, rows: rows, sortOrder: $sortOrder),
                       detail: ValueDetail(model: model, kind: list.scope, rows: rows) { model.valueSelection = [$0] },
                       layout: .values)
    }
}

/// The values, with no search field: the library offers no search over them. + adds a value,
/// double-click renames one in place, and the row menu deletes or merges.
private struct ValueGrid: View {
    @Bindable var model: DesktopModel
    let list: DesktopModel.ValueList
    let rows: [ValueRow]
    @Binding var sortOrder: [KeyPathComparator<ValueRow>]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ListCount(total: model.reusableValues[list.field]?.count ?? 0, noun: list.singular)
                .padding(12)
            Table(rows, selection: $model.valueSelection, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { row in
                    ValueName(model: model, row: row, identifier: "\(list.key).row.\(row.name).name")
                }
                .width(min: 80)
                TableColumn("Orders", value: \.usage) { row in
                    Text(row.usage, format: .number)
                        .monospacedDigit()
                        .accessibleValue("Orders", value: String(row.usage),
                                         identifier: "\(list.key).row.\(row.name).orders")
                }
                .width(72)
                .alignment(.numeric)
            }
            .tableStyle(.inset(alternatesRowBackgrounds: !rows.isEmpty))
            .disabled(!model.listsAvailable)
            .contextMenu(forSelectionType: ValueItem.self) { items in
                ValueMenu(model: model, items: items, scope: list.scope, prefix: list.key)
            } primaryAction: { items in
                if items.count == 1, let item = items.first { model.beginNaming(item) }
            }
            .onDeleteCommand(perform: model.valueDeletion(model.valueSelection))
            .overlay {
                if rows.isEmpty && !model.refreshingLookup {
                    let hint = "Add one with + or ⌘N, or from an Order’s \(list.singular) field."
                    ContentUnavailableView("No \(list.title)", systemImage: "tray", description: Text(hint))
                }
            }
            .accessibilityLabel(list.title)
            .accessibilityIdentifier("\(list.key).table")
            CreationBar(title: "New \(list.singular)", identifier: "\(list.key).new") {
                model.beginNaming(.new(list.scope))
            }
            .disabled(!model.canStartNew)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
