import CSpielOrders
import SwiftUI
import SpielOrdersKit

struct OrdersWorkspace: View {
    @Bindable var model: DesktopModel

    var body: some View {
        InspectorSplit(visible: $model.inspectorVisible, content: OrdersGrid(model: model), detail: OrderInspector(model: model))
    }
}

private struct OrdersGrid: View {
    @Bindable var model: DesktopModel
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("\(model.orders.count) of \(model.orderCount) orders shown.")
                .font(.callout).foregroundStyle(.secondary)
            OrdersTable(model: model)
        }
        .frame(minWidth: 460, maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct OrderInspector: View {
    @Bindable var model: DesktopModel
    private static let facts: [(Int32, String)] = [
        (SPIELORDERS_ORDER_FIELD_DATE_ORDERED, "Date"), (SPIELORDERS_ORDER_FIELD_STATUS, "Status"),
        (SPIELORDERS_ORDER_FIELD_VENDOR, "Vendor"), (SPIELORDERS_ORDER_FIELD_ORDERER, "Orderer"),
        (SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION, "Institution"),
        (SPIELORDERS_ORDER_FIELD_PROJECT, "Project"), (SPIELORDERS_ORDER_FIELD_ORDER_GROUP, "Order Group"),
        (SPIELORDERS_ORDER_FIELD_CURRENCY, "Currency"), (SPIELORDERS_ORDER_FIELD_DIRECTORY_NAME, "Directory Name"),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let row = model.orders.first(where: { $0.id == model.selectedOrderID }) {
                    switch model.inspection {
                    case let .healthy(details):
                        Text(details.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "").font(.headline)
                        ForEach(Self.facts, id: \.0) { fact in
                            LabeledContent(fact.1, value: details.values[fact.0] ?? "")
                        }
                        Divider()
                        Text("Items").font(.headline)
                        if details.items.isEmpty { Text("No Items").foregroundStyle(.secondary) }
                        else {
                            Table(details.items) {
                                TableColumn("Catalog Number", value: \.catalogNumber).width(min: 55, ideal: 65)
                                TableColumn("Description", value: \.description).width(min: 75, ideal: 120)
                                TableColumn("Quantity", value: \.quantity).width(min: 35, ideal: 45)
                                TableColumn("Unit Price", value: \.unitPrice).width(min: 50, ideal: 65)
                                TableColumn("Total", value: \.total).width(min: 50, ideal: 65)
                            }
                            .frame(minHeight: 90, idealHeight: 140, maxHeight: 200)
                            .font(.caption)
                        }
                        LabeledContent("Total", value: details.values[SPIELORDERS_ORDER_FIELD_TOTAL] ?? "")
                        Divider()
                        Text("README").font(.headline)
                        if details.readmeState == SPIELORDERS_README_STATE_UNREADABLE {
                            Text("README is unreadable. Reopen the Package to refresh.")
                        } else { Text(details.readmeText).font(.system(.body, design: .monospaced)) }
                        Button("Edit README") { model.editOrderReadme(row) }
                            .disabled(model.busy || details.readmeState == SPIELORDERS_README_STATE_UNREADABLE)
                        if !details.notices.isEmpty { Text(details.notices.joined(separator: "\n")).foregroundStyle(.secondary) }
                    case let .damaged(messages):
                        Text(row.directoryName).font(.headline)
                        Text(messages.joined(separator: "\n"))
                    case nil:
                        if model.busy { ProgressView("Reading Order…") }
                        else { Text("Order details are unavailable. Reopen the Package to refresh.") }
                    }
                    Button("Reveal in Finder") { model.selectOrder(row.id, revealInFinder: true) }.disabled(model.busy)
                } else {
                    Text("Select an Order").foregroundStyle(.secondary)
                }
            }
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, 12)
        }
    }
}
