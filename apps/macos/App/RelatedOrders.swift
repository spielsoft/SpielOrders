import SwiftUI
import SpielOrdersKit

/// The height of a compact table in a detail, which shows up to `shownRows` rows before it
/// scrolls.
enum CompactTable {
    static let shownRows = 6
    private static let rowHeight: CGFloat = 24
    private static let headerHeight: CGFloat = 28

    /// The height that shows `rows` rows, up to `limit` of them; nil shows every row.
    static func height(rows: Int, limit: Int? = shownRows) -> CGFloat {
        headerHeight + CGFloat(min(rows, limit ?? .max)) * rowHeight
    }
}

/// A detail's content from the top, at least as tall as the pane, so that its last table
/// takes the height the rest leaves. It scrolls only when the content, with that table at its
/// least, is taller than the pane, and sideways when the pane is narrower than `narrowest`.
struct DetailScroll<Content: View>: View {
    private let narrowest: CGFloat?
    private let content: Content
    @State private var height: CGFloat = 0

    init(narrowest: CGFloat? = nil, @ViewBuilder content: () -> Content) {
        self.narrowest = narrowest
        self.content = content()
    }

    var body: some View {
        ScrollView(narrowest == nil ? .vertical : [.vertical, .horizontal]) {
            VStack(alignment: .leading, spacing: 12) { content }
                .padding(12)
                .frame(maxWidth: .infinity, minHeight: height, alignment: .topLeading)
                .containerRelativeFrame(.horizontal) { width, _ in max(width, narrowest ?? 0) }
        }
        .defaultScrollAnchor(.topLeading)
        .scrollBounceBehavior(.basedOnSize)
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height = $0 }
    }
}

/// A section of a detail: its heading, then its content.
struct DetailSection<Content: View>: View {
    private let title: String
    private let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            content
        }
    }
}

/// The Orders that use a detail's record, filling the detail's spare height and never shorter
/// than a compact table. Double-click, Return, or Open shows one in Orders.
struct RelatedOrders: View {
    @Bindable var model: DesktopModel
    let orders: [OrderRow]
    let columns: [OrderColumn]
    /// The table's accessibility label, such as Vendor Orders.
    let label: String
    /// The identifier the table and its cells extend.
    let prefix: String
    @State private var selection: String?

    var body: some View {
        DetailSection("Orders") {
            if orders.isEmpty {
                Text("No Orders")
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("\(prefix).orders.placeholder")
            } else {
                table
            }
        }
    }

    private var table: some View {
        Table(orders, selection: $selection) {
            TableColumnForEach(columns, id: \.self) { column in
                TableColumn(column.label) { order in
                    let text = order.display(column)
                    Text(text)
                        .lineLimit(1)
                        .monospacedDigit()
                        .accessibleValue(column.label, value: text,
                                         identifier: "\(prefix).order.\(order.id).\(column.rawValue)")
                }
                .width(min: column.compactWidth.min, ideal: column.compactWidth.ideal)
                .alignment(column == .total ? .trailing : .leading)
            }
        }
        .tableStyle(.bordered(alternatesRowBackgrounds: true))
        .frame(minHeight: least, idealHeight: least, maxHeight: .infinity)
        .accessibilityLabel(label)
        .accessibilityIdentifier("\(prefix).orders")
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first {
                Button("Open") { open(id) }
                    .accessibilityIdentifier("\(prefix).order.\(id).open")
            }
        } primaryAction: { ids in
            open(ids.first)
        }
    }

    /// The table's height when the detail has none to spare.
    private var least: CGFloat { CompactTable.height(rows: orders.count) }

    private func open(_ id: String?) {
        guard let order = orders.first(where: { $0.id == id }) else { return }
        model.openRelatedOrder(order)
    }
}

extension OrderColumn {
    /// A column's width in a detail's compact table.
    fileprivate var compactWidth: (min: CGFloat, ideal: CGFloat) {
        switch self {
        case .title: (100, 150)
        case .date: (56, 70)
        case .status: (64, 90)
        case .total: (64, 80)
        default: (70, 100)
        }
    }
}
