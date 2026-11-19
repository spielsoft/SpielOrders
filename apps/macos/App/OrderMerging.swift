import SpielOrdersKit

/// Selecting several Orders and merging them. The library plans each merge in the precedence
/// order the owner sets: the first Order is kept, and each later one only fills what the
/// result still lacks.
extension DesktopModel {
    /// Selects the Orders `ids`, as the list proposes them. Several are kept in the order the
    /// owner selected them, after the pending edit of the shown Order is settled.
    func selectOrders(_ ids: [String]) {
        guard ids.count > 1 else {
            selectOrder(ids.first)
            return
        }
        if deferNavigation({ [self] in selectOrders(ids) }) { return }
        guard !busy, folder != nil, confirmDiscard(retry: { [self] in selectOrders(ids) }) else { return }
        let earlier = multipleSelection.isEmpty ? [selectedOrderID].compactMap { $0 } : multipleSelection
        let retained = earlier.filter(ids.contains)
        discardEditors()
        multipleSelection = retained + ids.filter { !retained.contains($0) }
        selectedOrderID = nil
        orderContext = nil
        inspection = nil
    }

    /// Merge names how many Orders it merges once several are selected.
    var mergeTitle: String {
        multipleSelection.isEmpty ? "Merge Orders…" : "Merge \(multipleSelection.count) Orders…"
    }

    /// Whether Merge can combine the selected Orders: every one readable, and nothing else
    /// in progress or unsaved.
    var canMergeOrders: Bool {
        guard canBrowseRecords, !busy, !dirty, destination == .orders, !diagnosticWorkspace else { return false }
        let rows = multipleSelection.compactMap(orderRow)
        return rows.count > 1 && rows.count == multipleSelection.count && rows.allSatisfy { $0.damage == nil }
    }

    /// Merge N Orders…: shows the library's plan for the selection, which the owner orders and
    /// confirms, then commits exactly that plan and shows the kept Order. A merge is a bulk
    /// change that Undo does not reverse; the merged Orders stay recoverable in Removed Orders.
    func mergeOrders() {
        if deferNavigation({ [self] in mergeOrders() }) { return }
        guard canMergeOrders, let retainedFolder = folder else { return }
        let selected = multipleSelection.compactMap(orderRow)
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer {
                busy = false
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let planning = Planning()
                let plan: @MainActor ([OrderRow]) async throws -> OrderMerge = { orders in
                    try await planning.run { cancellation in
                        try await session.planMergingOrders(orders, cancellation: cancellation)
                    }
                }
                let initial = try await session.planMergingOrders(selected, cancellation: cancellation)
                let answer = await dialogs.reviewMerge(of: initial, plan: plan)
                await planning.finish()
                guard let merge = answer else { return }
                try Task.checkCancellation()
                let committed = try await session.commit(merge)
                endRedo()
                discardEditors()
                contents.stop()
                multipleSelection = []
                if committed.requiresReopen {
                    requireReload(Notice("The Orders were merged. Reload the Package to keep editing.",
                                         details: committed.warnings.joined(separator: "\n\n")))
                    clearOrders()
                    return
                }
                await refreshRecords(selectingOrder: merge.orders[0].id, vendor: selectedVendorID)
                addWarnings(committed.warnings, to: .orders)
                if committed.unremoved > 0 {
                    reportUnremoved(committed, keeping: merge.orders[0])
                }
            } catch {
                guard !Task.isCancelled else { return }
                alert("Couldn’t merge the Orders", (error as? OrderMergeRefusal)?.summary ?? "",
                      details: error.localizedDescription, identifier: "order.merge")
            }
        }
    }

    /// Says which merged Orders gave the kept one what it lacked but stayed listed.
    private func reportUnremoved(_ committed: CommittedOrderMerge, keeping kept: OrderRow) {
        let count = committed.unremoved
        let subject = count == 1 ? "1 merged Order" : "\(count) merged Orders"
        let explanation = "“\(kept.name)” has what they added, but \(subject) stayed in the Orders folder. "
            + "Remove \(count == 1 ? "it" : "them") with Remove Order."
        alert("Some Merged Orders Weren’t Removed", explanation,
              details: committed.warnings.joined(separator: "\n\n"), identifier: "order.merge.unremoved")
    }
}
