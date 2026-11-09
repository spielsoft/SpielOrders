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
                Toggle("Show Hidden Vendors", isOn: Binding(get: { model.includeHiddenVendors }, set: {
                    _ = model.changeVendorList(includeHidden: $0, field: model.vendorSortField, descending: model.vendorSortDescending)
                })).toggleStyle(.checkbox).disabled(model.busy)
            }
            Table(model.vendors, selection: $selection, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { row in
                    HStack {
                        Text(row.name)
                        if row.hidden { Text("Hidden").font(.caption).foregroundStyle(.secondary) }
                    }
                }.width(min: 140, ideal: 200)
                TableColumn("Phone", value: \.phone).width(min: 100, ideal: 120)
                TableColumn("Email", value: \.email).width(min: 130, ideal: 180)
                TableColumn("Website", value: \.website).width(min: 130, ideal: 160)
            }
            .tableStyle(.inset).font(.callout).disabled(model.busy)
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
                    ForEach(Self.fields, id: \.0) { field in LabeledContent(field.1, value: details.values[field.0] ?? "") }
                    LabeledContent("Visibility", value: details.values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true" ? "Hidden" : "Visible")
                    Divider()
                    Text("Contacts").font(.headline)
                    ForEach(details.contacts) { contact in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(contact.name)
                            Text(contact.role).foregroundStyle(.secondary)
                            Text(contact.phone)
                            Text(contact.email)
                        }
                    }
                    Divider()
                    Text("Orders").font(.headline)
                    ForEach(details.orders) { order in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(order.values[SPIELORDERS_ORDER_FIELD_TITLE] ?? "")
                            Text([order.values[SPIELORDERS_ORDER_FIELD_DATE_ORDERED] ?? "",
                                  order.values[SPIELORDERS_ORDER_FIELD_STATUS] ?? ""].joined(separator: " · "))
                                .foregroundStyle(.secondary)
                        }
                    }
                    if !details.notices.isEmpty { Text(details.notices.joined(separator: "\n")).foregroundStyle(.secondary) }
                } else if model.busy && model.selectedVendorID != nil {
                    ProgressView("Reading Vendor…")
                } else { Text("Select a Vendor").foregroundStyle(.secondary) }
            }
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, 12)
        }
    }
}
