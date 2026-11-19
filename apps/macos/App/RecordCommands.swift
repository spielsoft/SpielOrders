import SwiftUI
import SpielOrdersKit

/// The menu for the current destination's records: Order, Vendor, or the kind of value the
/// destination lists. It carries every command of a row's menu, for the list's selection.
struct RecordCommands: Commands {
    @Bindable var model: DesktopModel
    /// Whether Remove Order takes the Delete key: it does so only from the Orders list. The
    /// command itself stays available.
    let deleteKeyRemoves: Bool

    var body: some Commands {
        if model.diagnosticWorkspace || model.destination == .orders {
            CommandMenu("Order") { orderMenu }
        } else if model.destination == .vendors {
            CommandMenu("Vendor") {
                VendorMenu(model: model, ids: model.vendorSelection, complete: true, prefix: "vendor.menu")
            }
        } else if case let .values(list) = model.destination {
            CommandMenu(list.singular) {
                ValueMenu(model: model, items: model.valueSelection, scope: list.scope, complete: true,
                          prefix: "\(list.key).menu")
            }
        } else if model.destination == .projects {
            CommandMenu("Project") {
                ValueMenu(model: model, items: model.valueSelection, scope: .projects, complete: true,
                          prefix: "projects.menu")
            }
        }
    }

    @ViewBuilder private var orderMenu: some View {
        Button("Open") { model.openOrder(model.currentOrder?.id) }
            .disabled(model.busy || model.vendorSheet != nil || model.currentOrder == nil)
            .accessibilityIdentifier("order.menu.open")
        Button("Show in Finder") { model.revealOrder(model.currentOrder?.id) }
            .disabled(model.busy || model.vendorSheet != nil || model.currentOrder == nil)
            .accessibilityIdentifier("order.menu.reveal")
        Button("Align Directory Name…") { model.changeOrderDirectory(.align) }
            .disabled(!model.canChangeDirectory || model.vendorSheet != nil)
            .accessibilityIdentifier("order.menu.align")
        Button("Diagnose Order") { model.showDiagnosis(directory: model.currentOrder?.directory) }
            .disabled(model.busy || model.vendorSheet != nil || model.currentOrder == nil)
            .accessibilityIdentifier("order.menu.diagnose")
        Button("Remove Order") { model.changeOrderDirectory(.remove) }
            .keyboardShortcut(deleteKeyRemoves ? KeyboardShortcut(.delete, modifiers: []) : nil)
            .disabled(!model.canChangeDirectory || model.vendorSheet != nil)
            .accessibilityIdentifier("order.menu.remove")
        Button(model.mergeTitle) { model.mergeOrders() }
            .disabled(!model.canMergeOrders || model.vendorSheet != nil)
            .accessibilityIdentifier("order.menu.merge")
        Divider()
        Button("Move to Trash…") { model.changeOrderDirectory(.trash) }
            .disabled(!model.canChangeDirectory || model.vendorSheet != nil)
            .accessibilityIdentifier("order.menu.trash")
    }
}
