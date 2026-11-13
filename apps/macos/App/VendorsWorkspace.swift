import CSpielOrders
import Foundation
import SwiftUI
import SpielOrdersKit

struct VendorsWorkspace: View {
    @Bindable var model: DesktopModel
    var body: some View {
        WorkspaceSplit(content: VendorGrid(model: model), detail: VendorDetail(model: model))
    }
}

private struct VendorGrid: View {
    @Bindable var model: DesktopModel
    @State private var selection: String?
    @State private var sortOrder = [KeyPathComparator(\VendorRow.name)]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                RecordSearch(model: model)
                ListCount(shown: model.vendors.count, total: model.vendorCount, noun: "Vendor")
            }
            .padding(12)
            Table(model.vendors, selection: $selection, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { row in
                    HStack {
                        Text(row.name)
                        if row.hidden { Text("Hidden").font(.caption).foregroundStyle(.secondary) }
                    }
                    .accessibleValue("Vendor Name", value: row.name + (row.hidden ? ", Hidden" : ""),
                                     identifier: "vendors.row.\(row.id).name")
                }.width(min: 140, ideal: 200)
                TableColumn("Phone", value: \.phone) { row in
                    Text(row.phone).accessibleValue("Phone", value: row.phone, identifier: "vendors.row.\(row.id).phone")
                }.width(min: 100, ideal: 120)
                TableColumn("Email", value: \.email) { row in
                    Text(row.email).accessibleValue("Email", value: row.email, identifier: "vendors.row.\(row.id).email")
                }.width(min: 130, ideal: 180)
                TableColumn("Website", value: \.website) { row in
                    Text(row.website).accessibleValue("Website", value: row.website, identifier: "vendors.row.\(row.id).website")
                }.width(min: 130, ideal: 160)
            }
            .accessibilityLabel("Vendors")
            .accessibilityIdentifier("vendors.table")
            .tableStyle(.inset).font(.callout).disabled(!model.listsAvailable)
            .overlay {
                if model.vendors.isEmpty, let reason = model.reloadReason {
                    ReloadRequired(model: model, reason: reason)
                }
            }
            .contextMenu(forSelectionType: String.self) { ids in
                if let row = model.vendors.first(where: { $0.id == ids.first }) {
                    Button("Open") { model.openVendor(row.id) }
                        .accessibilityIdentifier("vendors.row.\(row.id).open")
                    Button(row.hidden ? "Show Vendor" : "Hide Vendor") {
                        model.setVendorHidden(!row.hidden, vendor: row.id)
                    }
                    .disabled(model.busy || !model.canBrowseRecords)
                    .accessibilityIdentifier("vendors.row.\(row.id).visibility")
                }
            } primaryAction: { ids in model.openVendor(ids.first) }
            .onChange(of: selection) { _, id in
                model.selectVendor(id)
                selection = model.selectedVendorID
            }
            .onChange(of: model.selectedVendorID, initial: true) { _, id in selection = id }
            .onChange(of: sortOrder) { _, requested in
                let field: Int32
                switch requested.first?.keyPath {
                case \VendorRow.name: field = SPIELORDERS_VENDOR_FIELD_NAME
                case \VendorRow.phone: field = SPIELORDERS_VENDOR_FIELD_PHONE
                case \VendorRow.email: field = SPIELORDERS_VENDOR_FIELD_EMAIL
                case \VendorRow.website: field = SPIELORDERS_VENDOR_FIELD_WEBSITE
                default: field = -1
                }
                let descending = requested.first?.order == .reverse
                if !model.changeVendorList(includeHidden: model.includeHiddenVendors, field: field, descending: descending) {
                    sortOrder = appliedSort
                }
            }
            .onChange(of: model.vendorSortField, initial: true) { _, _ in sortOrder = appliedSort }
            .onChange(of: model.vendorSortDescending) { _, _ in sortOrder = appliedSort }
            .onChange(of: model.busy) { _, busy in if !busy { sortOrder = appliedSort } }
            CreationBar(title: "New Vendor", identifier: "vendor.new", action: model.newVendor)
                .disabled(model.busy && !model.committing)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var appliedSort: [KeyPathComparator<VendorRow>] {
        let order: SortOrder = model.vendorSortDescending ? .reverse : .forward
        switch model.vendorSortField {
        case SPIELORDERS_VENDOR_FIELD_NAME: return [KeyPathComparator(\VendorRow.name, order: order)]
        case SPIELORDERS_VENDOR_FIELD_PHONE: return [KeyPathComparator(\VendorRow.phone, order: order)]
        case SPIELORDERS_VENDOR_FIELD_EMAIL: return [KeyPathComparator(\VendorRow.email, order: order)]
        case SPIELORDERS_VENDOR_FIELD_WEBSITE: return [KeyPathComparator(\VendorRow.website, order: order)]
        default: return []
        }
    }
}

private struct VendorDetail: View {
    @Bindable var model: DesktopModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let editor = model.vendorEditor {
                    VendorEditorView(model: model, editor: editor)
                        .id(ObjectIdentifier(editor))
                    RecordActions(model: model, editor: editor)
                    if let details = model.vendorDetails {
                        Divider()
                        VendorOrders(model: model, orders: details.orders)
                    }
                } else {
                    VendorSummary(model: model)
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityLabel("Vendor detail")
        .accessibilityIdentifier("vendor.detail")
    }
}

/// The detail without a Vendor's editor: a Vendor that cannot be edited until the Package
/// is reloaded, or no selection.
private struct VendorSummary: View {
    @Bindable var model: DesktopModel

    var body: some View {
        if let details = model.vendorDetails, model.selectedVendorID != nil {
            if model.canBrowseRecords {
                ProgressView("Reading Vendor…")
            } else {
                RecordHeading(title: details.values[SPIELORDERS_VENDOR_FIELD_NAME] ?? "",
                              summary: VendorSummaryLine.of(details), prefix: "vendor.inspector") {
                    ReloadBadge(model: model, explanation: "Reload the Package to edit this Vendor.",
                                identifier: "vendor.inspector.reloadRequired")
                }
                VendorOrders(model: model, orders: details.orders)
            }
        } else if model.busy && model.selectedVendorID != nil {
            ProgressView("Reading Vendor…")
        } else {
            Text("Select a Vendor")
                .foregroundStyle(.secondary)
        }
    }
}

/// What the Vendor form does not show: the Orders that use the Vendor, compactly. Double-click
/// or Return opens one.
private struct VendorOrders: View {
    @Bindable var model: DesktopModel
    let orders: [OrderRow]
    @State private var selection: String?

    /// The table shows up to this many rows before it scrolls.
    private static let shownRows = 8
    private static let rowHeight: CGFloat = 24
    private static let headerHeight: CGFloat = 28

    var body: some View {
        Text("Orders")
            .font(.headline)
            .accessibilityAddTraits(.isHeader)
        if orders.isEmpty {
            Text("No Orders")
                .foregroundStyle(.tertiary)
                .accessibilityIdentifier("vendor.orders.placeholder")
        } else {
            table
        }
    }

    private var table: some View {
        Table(orders, selection: $selection) {
            TableColumn(OrderColumn.title.label) { order in
                cell(order, .title, value(order, .title))
            }
            .width(min: 120, ideal: 240)
            TableColumn(OrderColumn.date.label) { order in
                cell(order, .date, ShortDate.display(value(order, .date)))
            }
            .width(min: 60, ideal: 80)
            TableColumn(OrderColumn.status.label) { order in
                cell(order, .status, OrderStatus.label(value(order, .status)))
            }
            .width(min: 70, ideal: 100)
            TableColumn(OrderColumn.total.label) { order in
                cell(order, .total, Money.display(value(order, .total), currency: value(order, .currency)))
                    .monospacedDigit()
            }
            .width(min: 70, ideal: 100)
            .alignment(.trailing)
        }
        .tableStyle(.bordered(alternatesRowBackgrounds: true))
        .font(.callout)
        .frame(height: height)
        .accessibilityLabel("Vendor Orders")
        .accessibilityIdentifier("vendor.orders")
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first {
                Button("Open") { open(id) }
                    .accessibilityIdentifier("vendor.order.\(id).open")
            }
        } primaryAction: { ids in
            open(ids.first)
        }
    }

    private var height: CGFloat {
        Self.headerHeight + CGFloat(min(orders.count, Self.shownRows)) * Self.rowHeight
    }

    private func value(_ order: OrderRow, _ column: OrderColumn) -> String {
        order.values[column.field] ?? ""
    }

    private func cell(_ order: OrderRow, _ column: OrderColumn, _ text: String) -> some View {
        Text(text)
            .lineLimit(1)
            .accessibleValue(column.label, value: text, identifier: "vendor.order.\(order.id).\(column.rawValue)")
    }

    private func open(_ id: String?) {
        guard let order = orders.first(where: { $0.id == id }) else { return }
        model.openRelatedOrder(order)
    }
}
