import AppKit
import SpielOrdersKit

/// An Order that Remove Order moved to the Removed Orders directory. Edit → Undo restores it
/// through the library and Redo removes it again; it is the undo target of both. An undo
/// manager does not retain its targets, so each registered action holds it.
final class RemovedOrder {
    /// The Order's row identity, which a restore keeps.
    let id: String
    /// Where the last removal left it.
    var directory: URL

    init(id: String, directory: URL) {
        self.id = id
        self.directory = directory
    }
}

/// Remove Order's Undo and Redo in the window's history, with the removed Order as their target.
extension DesktopModel {
    /// Offers Undo Remove Order, and puts the keyboard in the Orders list.
    func offerRestore(_ order: RemovedOrder) {
        undoManager.registerUndo(withTarget: order) { [weak self] _ in self?.restore(order) }
        undoManager.setActionName(DirectoryAction.remove.title)
        guard let window = dialogs.window, let list = window.contentView?.descendant(OrdersTableView.self) else {
            return
        }
        window.makeFirstResponder(list)
    }

    /// Undo Remove Order. Redo is registered at once, as it must be while undoing; a refusal
    /// withdraws it and offers Undo again, leaving everything as it was.
    private func restore(_ order: RemovedOrder) {
        undoManager.registerUndo(withTarget: order) { [weak self] _ in self?.removeAgain(order) }
        undoManager.setActionName(DirectoryAction.remove.title)
        guard canBrowseRecords else {
            Task {
                keepRemoved(order, "The Order wasn’t restored",
                            "Reload the Package, then try again.")
            }
            return
        }
        guard !busy, !dirty, let retainedFolder = folder else {
            Task {
                keepRemoved(order, "The Order wasn’t restored",
                            "Finish the current work or unsaved changes, then choose Edit → Undo again.")
            }
            return
        }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer {
                busy = false
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let restored = try await withTaskCancellationHandler {
                    try await session.restoreOrder(id: order.id, from: order.directory, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                discardEditors()
                contents.stop()
                if restored.requiresReopen {
                    requireReload(Notice(restored.summary, details: restored.message))
                    clearOrders()
                } else {
                    await refreshRecords(selectingOrder: order.id, vendor: selectedVendorID)
                    addWarnings(restored.notices, to: .orders)
                }
            } catch {
                let summary = (error as? RestoreRefusal)?.summary ?? "The Order wasn’t restored"
                keepRemoved(order, summary, "It stays removed, and Edit → Undo can still restore it.",
                            details: error.localizedDescription)
            }
        }
    }

    /// Redo Remove Order: the existing removal, without a review. Undo is registered at once.
    private func removeAgain(_ order: RemovedOrder) {
        undoManager.registerUndo(withTarget: order) { [weak self] _ in self?.restore(order) }
        undoManager.setActionName(DirectoryAction.remove.title)
        guard canBrowseRecords, !busy, !dirty, let row = orderRow(order.id), let retainedFolder = folder else {
            Task { forget(order, "Finish the current work or unsaved changes, then choose Remove Order.") }
            return
        }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer {
                busy = false
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let plan = try await withTaskCancellationHandler {
                    try await session.planDirectoryChange(row, action: .remove, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                let removed = try await session.commit(plan)
                discardEditors()
                contents.stop()
                guard let directory = removed.finalDirectory else {
                    undoManager.removeAllActions(withTarget: order)
                    requireReload(Notice(removed.summary, details: removed.message))
                    clearOrders()
                    return
                }
                order.directory = directory
                if removed.requiresReopen {
                    requireReload(Notice(removed.summary, details: removed.message))
                    clearOrders()
                    return
                }
                await refreshRecords(selectingOrder: nil, vendor: selectedVendorID)
                addWarnings(removed.notices, to: .orders)
            } catch {
                forget(order, "Select it and choose Remove Order to try again.", details: error.localizedDescription)
            }
        }
    }

    /// The Order stays removed and Undo Remove Order is offered again.
    private func keepRemoved(_ order: RemovedOrder, _ title: String, _ explanation: String, details: String = "") {
        undoManager.removeAllActions(withTarget: order)
        offerRestore(order)
        alert(title, explanation, details: details, identifier: "order.restore")
    }

    /// The Order stays where it is, and its removal leaves the history.
    private func forget(_ order: RemovedOrder, _ explanation: String, details: String = "") {
        undoManager.removeAllActions(withTarget: order)
        alert("The Order wasn’t removed again", explanation, details: details, identifier: "order.removeAgain")
    }

    /// Nothing is listed until Reload, after a change committed without a readable Package.
    func clearOrders() {
        orders = []
        selectedOrderID = nil
        inspection = nil
    }
}

extension NSView {
    /// The first view of `type` in this view or beneath it.
    func descendant<View: NSView>(_ type: View.Type) -> View? {
        if let view = self as? View {
            return view
        }
        for view in subviews {
            if let found = view.descendant(type) {
                return found
            }
        }
        return nil
    }
}
