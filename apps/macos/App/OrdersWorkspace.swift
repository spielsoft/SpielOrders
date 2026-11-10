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
    private static let facts: [OrderColumn] = [.date, .status, .vendor, .orderer, .institution,
                                               .project, .orderGroup, .currency, .directory]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let row = model.orders.first(where: { $0.id == model.selectedOrderID }) {
                    switch model.inspection {
                    case let .healthy(details):
                        Text(details.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "").font(.headline)
                            .accessibleValue("Order Title", value: details.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "",
                                             identifier: "order.inspector.title")
                        ForEach(Self.facts, id: \.self) { fact in
                            LabeledContent(fact.label, value: details.values[fact.field] ?? "")
                                .accessibleValue(fact.label, value: details.values[fact.field] ?? "",
                                                 identifier: "order.inspector.\(fact.rawValue)")
                        }
                        Divider()
                        Text("Items").font(.headline).accessibilityAddTraits(.isHeader)
                        if details.items.isEmpty { Text("No Items").foregroundStyle(.secondary) }
                        else {
                            Table(details.items) {
                                TableColumn("Catalog Number") { item in
                                    Text(item.catalogNumber).accessibleValue("Catalog Number", value: item.catalogNumber,
                                        identifier: "order.items.row.\(item.id).catalogNumber")
                                }.width(min: 55, ideal: 65)
                                TableColumn("Description") { item in
                                    Text(item.description).accessibleValue("Description", value: item.description,
                                        identifier: "order.items.row.\(item.id).description")
                                }.width(min: 75, ideal: 120)
                                TableColumn("Quantity") { item in
                                    Text(item.quantity).accessibleValue("Quantity", value: item.quantity,
                                        identifier: "order.items.row.\(item.id).quantity")
                                }.width(min: 35, ideal: 45)
                                TableColumn("Unit Price") { item in
                                    Text(item.unitPrice).accessibleValue("Unit Price", value: item.unitPrice,
                                        identifier: "order.items.row.\(item.id).unitPrice")
                                }.width(min: 50, ideal: 65)
                                TableColumn("Total") { item in
                                    Text(item.total).accessibleValue("Item Total", value: item.total,
                                        identifier: "order.items.row.\(item.id).total")
                                }.width(min: 50, ideal: 65)
                            }
                            .accessibilityLabel("Order Items")
                            .accessibilityIdentifier("order.items")
                            .frame(minHeight: 90, idealHeight: 140, maxHeight: 200)
                            .font(.caption)
                        }
                        LabeledContent("Total", value: details.values[SPIELORDERS_ORDER_FIELD_TOTAL] ?? "")
                            .accessibleValue("Total", value: details.values[SPIELORDERS_ORDER_FIELD_TOTAL] ?? "",
                                             identifier: "order.inspector.total")
                        Divider()
                        Text("README").font(.headline).accessibilityAddTraits(.isHeader)
                        if details.readmeState == SPIELORDERS_README_STATE_UNREADABLE {
                            Text("README is unreadable. Reopen the Package to refresh.")
                        } else {
                            Text(details.readmeText).font(.system(.body, design: .monospaced))
                                .textSelection(.enabled)
                                .accessibleValue("Order README", value: details.readmeText, identifier: "order.inspector.readme")
                        }
                        Button("Edit README") { model.editOrderReadme(row) }
                            .disabled(model.busy || details.readmeState == SPIELORDERS_README_STATE_UNREADABLE)
                            .accessibilityIdentifier("order.readme.edit")
                        if !details.notices.isEmpty { Text(details.notices.joined(separator: "\n")).foregroundStyle(.secondary) }
                    case let .damaged(messages):
                        Text(row.directoryName).font(.headline)
                            .accessibleValue("Directory Name", value: row.directoryName, identifier: "order.inspector.directory")
                        Text(messages.joined(separator: "\n"))
                            .accessibleValue("Diagnosis", value: messages.joined(separator: "\n"), identifier: "order.inspector.diagnosis")
                    case nil:
                        if model.busy { ProgressView("Reading Order…") }
                        else { Text("Order details are unavailable. Reopen the Package to refresh.") }
                    }
                    Button("Reveal in Finder") { model.selectOrder(row.id, revealInFinder: true) }.disabled(model.busy)
                        .accessibilityIdentifier("order.inspector.reveal")
                } else {
                    Text("Select an Order").foregroundStyle(.secondary)
                }
            }
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, 12)
        }
        .accessibilityLabel("Order inspector")
        .accessibilityIdentifier("order.inspector")
    }
}
