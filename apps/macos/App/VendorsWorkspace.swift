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
    /// The selected rows: the Vendor the detail shows, or several to merge.
    @State private var selection: Set<String> = []
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
                    Text(row.name)
                        .accessibleValue("Vendor Name", value: row.name, identifier: "vendors.row.\(row.id).name")
                }.width(min: 140, ideal: 200)
                TableColumn("Hidden", value: \.hiddenRank) { row in
                    Image(systemName: "eye.slash")
                        .foregroundStyle(.secondary)
                        .opacity(row.hidden ? 1 : 0)
                        .frame(maxWidth: .infinity)
                        .accessibleValue("Hidden", value: row.hidden ? "Hidden" : "Shown",
                                         identifier: "vendors.row.\(row.id).hidden")
                }.width(72)
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
            .tableStyle(.inset(alternatesRowBackgrounds: !model.vendors.isEmpty))
            .disabled(!model.listsAvailable)
            .overlay {
                if model.vendors.isEmpty {
                    EmptyRecordList(model: model, total: model.vendorCount, noun: "Vendor",
                                    hint: "Add one with + or ⌘N, or from an Order’s Vendor field.")
                }
            }
            .contextMenu(forSelectionType: String.self) { ids in
                VendorMenu(model: model, ids: ids, prefix: "vendors")
            } primaryAction: { ids in
                if ids.count == 1 { model.openVendor(ids.first) }
            }
            .onDeleteCommand(perform: model.vendorDeletion(selected))
            .onChange(of: selection) { _, ids in
                model.selectVendors(ids)
                selection = selected
            }
            .onChange(of: model.selectedVendorID, initial: true) { selection = selected }
            .onChange(of: model.multipleVendorSelection) { selection = selected }
            .onChange(of: sortOrder) { _, requested in
                let field: Int32
                switch requested.first?.keyPath {
                case \VendorRow.name: field = SPIELORDERS_VENDOR_FIELD_NAME
                case \VendorRow.phone: field = SPIELORDERS_VENDOR_FIELD_PHONE
                case \VendorRow.email: field = SPIELORDERS_VENDOR_FIELD_EMAIL
                case \VendorRow.website: field = SPIELORDERS_VENDOR_FIELD_WEBSITE
                case \VendorRow.hiddenRank: field = SPIELORDERS_VENDOR_FIELD_HIDDEN
                default: field = -1
                }
                let descending = requested.first?.order == .reverse
                if !model.changeVendorSort(field: field, descending: descending) {
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

    /// The rows the model has selected.
    private var selected: Set<String> { model.vendorSelection }

    private var appliedSort: [KeyPathComparator<VendorRow>] {
        let order: SortOrder = model.vendorSortDescending ? .reverse : .forward
        switch model.vendorSortField {
        case SPIELORDERS_VENDOR_FIELD_NAME: return [KeyPathComparator(\VendorRow.name, order: order)]
        case SPIELORDERS_VENDOR_FIELD_PHONE: return [KeyPathComparator(\VendorRow.phone, order: order)]
        case SPIELORDERS_VENDOR_FIELD_EMAIL: return [KeyPathComparator(\VendorRow.email, order: order)]
        case SPIELORDERS_VENDOR_FIELD_WEBSITE: return [KeyPathComparator(\VendorRow.website, order: order)]
        case SPIELORDERS_VENDOR_FIELD_HIDDEN: return [KeyPathComparator(\VendorRow.hiddenRank, order: order)]
        default: return []
        }
    }
}

private extension VendorRow {
    /// The Hidden column's sort key; the library orders the rows.
    var hiddenRank: Int { hidden ? 1 : 0 }
}

/// The Vendor commands for `ids`: Open, Hide or Show, and Delete… for one Vendor, Merge N
/// Vendors… for several. A row's menu shows those that apply; the Vendor menu shows every one,
/// disabling those that don't.
struct VendorMenu: View {
    @Bindable var model: DesktopModel
    let ids: Set<String>
    /// Whether commands that don't apply are shown disabled, as the menu bar shows them.
    var complete = false
    /// The menu's identifier, which the commands extend.
    let prefix: String

    var body: some View {
        let row = ids.count == 1 ? model.vendors.first { $0.id == ids.first } : nil
        let deletion = model.vendorDeletion(ids)
        if row != nil || complete {
            Button("Open") { model.openVendor(row?.id) }
                .disabled(row == nil)
                .accessibilityIdentifier("\(prefix).open")
            Button(row?.hidden == true ? "Show Vendor" : "Hide Vendor") {
                if let row { model.setVendorHidden(!row.hidden, vendor: row.id) }
            }
            .disabled(row == nil || !model.canChangeValues)
            .accessibilityIdentifier("\(prefix).visibility")
        }
        if ids.count > 1 || complete {
            Button(ids.count > 1 ? "Merge \(ids.count) Vendors…" : "Merge Vendors…") { model.mergeVendors(ids) }
                .disabled(ids.count < 2 || !model.canChangeValues)
                .accessibilityIdentifier("\(prefix).merge")
        }
        if row != nil || complete {
            Divider()
            Button("Delete Vendor…") { deletion?() }
                .disabled(deletion == nil)
                .accessibilityIdentifier("\(prefix).delete")
        }
    }
}

private struct VendorDetail: View {
    @Bindable var model: DesktopModel

    var body: some View {
        if !model.multipleVendorSelection.isEmpty {
            SelectedVendors(model: model)
        } else if model.vendorEditor != nil || model.selectedVendorID != nil {
            SelectedVendor(model: model)
        } else if !model.vendors.isEmpty {
            ContentUnavailableView { Text("No Vendor Selected") }
                .accessibilityIdentifier("vendor.detail.placeholder")
        }
    }
}

/// The detail for several selected Vendors: how many, and Merge.
private struct SelectedVendors: View {
    @Bindable var model: DesktopModel

    var body: some View {
        let selection = model.multipleVendorSelection
        ContentUnavailableView {
            Label("\(selection.count) Vendors Selected", systemImage: "building.2")
        } description: {
            Text("Merging keeps the Vendor you selected first. You can change that before merging.")
        } actions: {
            Button("Merge \(selection.count) Vendors…") { model.mergeVendors(Set(selection)) }
                .disabled(model.busy || !model.canBrowseRecords)
                .accessibilityIdentifier("vendor.selection.merge")
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Vendor detail")
        .accessibilityIdentifier("vendor.selection")
    }
}

/// The detail for one selected Vendor: its editor, or what can be shown without one.
private struct SelectedVendor: View {
    @Bindable var model: DesktopModel

    var body: some View {
        DetailScroll(narrowest: 420) {
            if let editor = model.vendorEditor {
                VendorEditorView(model: model, editor: editor)
                    .id(ObjectIdentifier(editor))
                RecordActions(model: model, editor: editor)
                if let details = model.vendorDetails {
                    Divider()
                    RelatedOrders(model: model, orders: details.orders, columns: [.title, .date, .status, .total],
                                  label: "Vendor Orders", prefix: "vendor")
                }
            } else {
                VendorSummary(model: model)
            }
        }
        .accessibilityLabel("Vendor detail")
        .accessibilityIdentifier("vendor.detail")
    }
}

/// The detail without a Vendor's editor: a Vendor that cannot be edited until the Package
/// is reloaded, or one being read.
private struct VendorSummary: View {
    @Bindable var model: DesktopModel

    var body: some View {
        if let details = model.vendorDetails, model.selectedVendorID != nil {
            if model.canBrowseRecords {
                ProgressView("Reading Vendor…")
            } else {
                RecordHeading(title: details.values[SPIELORDERS_VENDOR_FIELD_NAME] ?? "",
                              summary: VendorSummaryLine.of(details), prefix: "vendor.inspector") {
                    ReloadBadge(model: model, explanation: "Reload the Package to keep editing.",
                                identifier: "vendor.inspector.reloadRequired")
                }
                Divider()
                RelatedOrders(model: model, orders: details.orders, columns: [.title, .date, .status, .total],
                              label: "Vendor Orders", prefix: "vendor")
            }
        } else if model.busy {
            ProgressView("Reading Vendor…")
        }
    }
}
