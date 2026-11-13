import SwiftUI
import SpielOrdersKit

/// One Order fact's values with the number of Orders using each, as the library lists them.
///
/// The table is display-only and has no detail beside it, so it takes the content's width.
/// It has no search field: the library offers no search over these values.
struct ValuesWorkspace: View {
    @Bindable var model: DesktopModel
    let list: DesktopModel.ValueList
    @State private var sortOrder = [KeyPathComparator(\ReusableValue.name)]

    private var values: [ReusableValue] {
        (model.reusableValues[list.field] ?? []).sorted(using: sortOrder)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ListCount(total: model.reusableValues[list.field]?.count ?? 0, noun: list.singular)
                .padding(12)
            Table(values, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { value in
                    Text(value.name)
                        .accessibleValue("Name", value: value.name,
                                         identifier: "\(list.key).row.\(value.name).name")
                }
                .width(min: 160, ideal: 320, max: 480)
                TableColumn("Orders", value: \.orderCount) { value in
                    Text(value.orderCount, format: .number)
                        .monospacedDigit()
                        .accessibleValue("Orders", value: String(value.orderCount),
                                         identifier: "\(list.key).row.\(value.name).orders")
                }
                .width(min: 60, ideal: 80, max: 120)
                .alignment(.numeric)
            }
            .tableStyle(.inset)
            .font(.callout)
            .overlay {
                if values.isEmpty && !model.refreshingLookup {
                    ContentUnavailableView("No \(list.title)", systemImage: "tray",
                                           description: Text("Values appear here once an Order uses them."))
                }
            }
            .accessibilityLabel(list.title)
            .accessibilityIdentifier("\(list.key).table")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
