import CSpielOrders
import Foundation
import SwiftUI
import SpielOrdersKit

struct VendorsWorkspace: View {
    @Bindable var model: DesktopModel
    var body: some View {
        InspectorSplit(visible: $model.inspectorVisible, content: VendorGrid(model: model), detail: VendorInspector(model: model))
    }
}

private struct VendorGrid: View {
    @Bindable var model: DesktopModel
    @State private var selection: String?
    @State private var sortOrder = [KeyPathComparator(\VendorRow.name)]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("\(model.vendors.count) vendors shown").font(.callout).foregroundStyle(.secondary)
                Spacer()
                Button("New Vendor", systemImage: "plus") { model.newVendor() }
                    .disabled(model.busy)
                    .accessibilityIdentifier("vendor.new")
                Toggle("Show Hidden Vendors", isOn: Binding(get: { model.includeHiddenVendors }, set: {
                    _ = model.changeVendorList(includeHidden: $0, field: model.vendorSortField, descending: model.vendorSortDescending)
                })).toggleStyle(.checkbox).disabled(model.busy)
                    .accessibilityIdentifier("vendors.showHidden")
            }
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
            .tableStyle(.inset).font(.callout).disabled(model.busy)
            .contextMenu(forSelectionType: String.self) { ids in
                if let id = ids.first {
                    Button("Open") { model.openVendor(id) }.accessibilityIdentifier("vendors.row.\(id).open")
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
        }
        .frame(minWidth: 460, maxWidth: .infinity, maxHeight: .infinity)
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

private struct VendorInspector: View {
    @Bindable var model: DesktopModel
    private static let fields: [(Int32, String)] = [
        (SPIELORDERS_VENDOR_FIELD_PHONE, "Phone"), (SPIELORDERS_VENDOR_FIELD_EMAIL, "Email"),
        (SPIELORDERS_VENDOR_FIELD_WEBSITE, "Website"), (SPIELORDERS_VENDOR_FIELD_ADDRESS, "Address"),
        (SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES, "Purchasing Notes"),
        (SPIELORDERS_VENDOR_FIELD_VENDOR_ID, "Vendor ID"),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let details = model.vendorDetails {
                    Text(details.values[SPIELORDERS_VENDOR_FIELD_NAME] ?? "").font(.headline)
                        .accessibleValue("Vendor Name", value: details.values[SPIELORDERS_VENDOR_FIELD_NAME] ?? "",
                                         identifier: "vendor.inspector.name")
                    ForEach(Self.fields, id: \.0) { field in
                        LabeledContent(field.1, value: details.values[field.0] ?? "")
                            .accessibleValue(field.1, value: details.values[field.0] ?? "",
                                             identifier: "vendor.inspector.field.\(field.0)")
                    }
                    LabeledContent("Visibility", value: details.values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true" ? "Hidden" : "Visible")
                        .accessibleValue("Visibility", value: details.values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true" ? "Hidden" : "Visible",
                                         identifier: "vendor.inspector.visibility")
                    Divider()
                    Text("Contacts").font(.headline).accessibilityAddTraits(.isHeader)
                    ForEach(details.contacts) { contact in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(contact.name)
                                .accessibleValue("Contact Name", value: contact.name, identifier: "vendor.contact.\(contact.id).name")
                            Text(contact.role).foregroundStyle(.secondary)
                                .accessibleValue("Role", value: contact.role, identifier: "vendor.contact.\(contact.id).role")
                            Text(contact.phone)
                                .accessibleValue("Phone", value: contact.phone, identifier: "vendor.contact.\(contact.id).phone")
                            Text(contact.email)
                                .accessibleValue("Email", value: contact.email, identifier: "vendor.contact.\(contact.id).email")
                        }
                    }
                    Divider()
                    Text("Orders").font(.headline).accessibilityAddTraits(.isHeader)
                    ForEach(details.orders) { order in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(order.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "")
                                .accessibleValue("Order Title", value: order.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "",
                                                 identifier: "vendor.order.\(order.id).title")
                            Text([order.values[SPIELORDERS_ORDER_FIELD_DATE_ORDERED] ?? "",
                                  order.values[SPIELORDERS_ORDER_FIELD_STATUS] ?? ""].joined(separator: " · "))
                                .foregroundStyle(.secondary)
                                .accessibleValue("Order Date and Status",
                                    value: [order.values[SPIELORDERS_ORDER_FIELD_DATE_ORDERED] ?? "",
                                            order.values[SPIELORDERS_ORDER_FIELD_STATUS] ?? ""].joined(separator: " · "),
                                    identifier: "vendor.order.\(order.id).summary")
                        }
                    }
                    if !details.notices.isEmpty { Text(details.notices.joined(separator: "\n")).foregroundStyle(.secondary) }
                    let hidden = details.values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true"
                    Button(hidden ? "Show Vendor" : "Hide Vendor") { model.setSelectedVendorHidden(!hidden) }
                        .disabled(model.busy)
                        .help(hidden ? "Offer this Vendor for new use again" : "Stop offering this Vendor for new use")
                        .accessibilityIdentifier("vendor.inspector.visibility.toggle")
                } else if model.busy && model.selectedVendorID != nil {
                    ProgressView("Reading Vendor…")
                } else { Text("Select a Vendor").foregroundStyle(.secondary) }
            }
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, 12)
        }
        .accessibilityLabel("Vendor inspector")
        .accessibilityIdentifier("vendor.inspector")
    }
}
