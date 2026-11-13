import SwiftUI
import SpielOrdersKit

struct OrderCommands: Commands {
    @Bindable var model: DesktopModel

    var body: some Commands {
        CommandMenu("Order") {
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
                .disabled(!model.canChangeDirectory || model.vendorSheet != nil)
                .accessibilityIdentifier("order.menu.remove")
            Divider()
            Button("Move to Trash…") { model.changeOrderDirectory(.trash) }
                .disabled(!model.canChangeDirectory || model.vendorSheet != nil)
                .accessibilityIdentifier("order.menu.trash")
        }
    }
}
