import SwiftUI
import SpielOrdersKit

/// One Order fact's values with the number of Orders using each, as the library lists them.
///
/// The table has no detail beside it, so it takes the content's width, and no search field:
/// the library offers no search over these values. + adds a value, double-click renames one in
/// place, and the row menu deletes or merges.
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
        VStack(alignment: .leading, spacing: 0) {
            ListCount(total: model.reusableValues[list.field]?.count ?? 0, noun: list.singular)
                .padding(12)
            Table(rows, selection: $model.valueSelection, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { row in
                    ValueName(model: model, row: row, identifier: "\(list.key).row.\(row.name).name")
                }
                .width(min: 160, ideal: 320, max: 480)
                TableColumn("Orders", value: \.usage) { row in
                    Text(row.usage, format: .number)
                        .monospacedDigit()
                        .accessibleValue("Orders", value: String(row.usage),
                                         identifier: "\(list.key).row.\(row.name).orders")
                }
                .width(min: 60, ideal: 80, max: 120)
                .alignment(.numeric)
            }
            .tableStyle(.inset)
            .font(.callout)
            .disabled(!model.listsAvailable)
            .contextMenu(forSelectionType: ValueItem.self) { items in
                ValueMenu(model: model, items: items, prefix: list.key)
            } primaryAction: { items in
                if items.count == 1, let item = items.first { model.beginNaming(item) }
            }
            .overlay {
                if rows.isEmpty && !model.refreshingLookup {
                    let hint = "Add one with +, or from an Order’s \(list.singular) field."
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
