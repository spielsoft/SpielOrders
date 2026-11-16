import AppKit
import CSpielOrders
import SpielOrdersKit

/// Adding, renaming, merging, and deleting the Package's values and Vendors from the sidebar
/// tables. Each change is one guarded library write that every affected Order follows; a change
/// to Orders states how many first and is not undoable, while adding a value is.
extension DesktopModel {
    /// Names `item` in place: a new value, or an existing one to rename.
    func beginNaming(_ item: ValueItem) {
        guard canBrowseRecords, !busy, item != .noProject else { return }
        valueSelection = [item]
        naming = item
    }

    /// Ends naming `item` with `text`: a new value is added, a renamed one respelled. A new
    /// value left blank is dropped.
    func endNaming(_ item: ValueItem, as text: String) {
        guard naming == item else { return }
        naming = nil
        switch item {
        case let .new(scope):
            valueSelection = []
            if !text.allSatisfy(\.isWhitespace) { addValue(text, to: scope) }
        case let .value(scope, name):
            if !text.utf8.elementsEqual(name.utf8) { renameValue(name, to: text, in: scope) }
        case .noProject:
            break
        }
    }

    func cancelNaming() {
        if case .new = naming { valueSelection = [] }
        naming = nil
    }

    /// Adds `name` to `scope`, unused; Undo deletes it again while no Order uses it.
    func addValue(_ name: String, to scope: ValueScope) {
        changeValues("Couldn’t add the \(scope.noun)") { [self] session, cancellation in
            valueSelection = [.value(scope, name)]
            return try await session.planAdding(name, to: scope, cancellation: cancellation)
        } then: { [self] committed in
            if committed.changed { offerUndo(AddedValue(scope: scope, name: name)) }
        }
    }

    /// Respells `value` as `name` in every Order that uses it, after stating how many change.
    /// A name that is already another value offers to merge the two instead.
    func renameValue(_ value: String, to name: String, in scope: ValueScope) {
        changeValues("Couldn’t rename the \(scope.noun)") { [self] session, cancellation in
            if await session.renamingMerges(value, into: name, in: scope) {
                let kept = try await spelling(of: name, besides: value, in: scope, session: session)
                let explanation = "“\(kept)” is already one of the Package’s \(scope.plural), so renaming "
                    + "“\(value)” merges the two. Choose the spelling to keep; "
                    + "every Order using either one uses it."
                return try await chooseMerge([kept, value], in: scope, title: "Merge “\(value)” and “\(kept)”?",
                                             explanation: explanation, session: session, cancellation: cancellation)
            }
            let change = try await session.planRenaming(value, to: name, in: scope, cancellation: cancellation)
            if change.orderCount > 0 {
                let count = OrderChangeCount.text(change.orderCount)
                let approved = await confirm("Rename “\(value)” to “\(name)”?", count,
                                             identifier: "value.rename", approve: "Rename")
                guard approved else { return nil }
            }
            valueSelection = [.value(scope, name)]
            return change
        }
    }

    /// Merge N …: asks which spelling to keep, then every Order using one of `names` uses it.
    func mergeValues(_ names: [String], in scope: ValueScope) {
        changeValues("Couldn’t merge the \(scope.plural)") { [self] session, cancellation in
            try await chooseMerge(names, in: scope, title: "Merge \(names.count) \(scope.plural)",
                                  explanation: "Choose the spelling to keep. Every Order using one of these uses it.",
                                  session: session, cancellation: cancellation)
        }
    }

    /// Delete…: an unused value goes after a short confirmation; a used one asks what its Orders
    /// use instead.
    func deleteValue(_ name: String, in scope: ValueScope) {
        changeValues("Couldn’t delete the \(scope.noun)") { [self] session, cancellation in
            let plan: @MainActor (Replacement) async throws -> ValueChange = { replacement in
                try await session.planDeleting(name, in: scope, replacement: replacement, cancellation: cancellation)
            }
            do {
                let unused = try await plan(.refuse)
                let approved = await confirm("Delete the \(scope.noun) “\(name)”?", unusedNote(name, in: scope),
                                             identifier: "value.delete", approve: "Delete", destructive: true)
                guard approved else { return nil }
                valueSelection = []
                return unused
            } catch let refusal as ValueRefusal where refusal.inUse {
                let others = try await values(in: scope, session: session).filter { !$0.utf8.elementsEqual(name.utf8) }
                    .map { (title: $0, id: $0) }
                let replaced = try await chooseReplacement(for: "the \(scope.noun) “\(name)”", others: others,
                                                           refusal: refusal, plan: plan)
                if replaced != nil { valueSelection = [] }
                return replaced
            }
        }
    }

    /// Delete… for a Vendor: an unused one goes after a confirmation, since it is not undoable;
    /// a used one asks which Vendor its Orders use instead.
    func deleteVendor(_ id: String) {
        if deferNavigation({ [self] in deleteVendor(id) }) { return }
        guard confirmDiscard(retry: { [self] in deleteVendor(id) }) else { return }
        changeValues("Couldn’t delete the Vendor") { [self] session, cancellation in
            let vendors = try await listVendors(session)
            let name = vendors.first { $0.id == id }?.name ?? ""
            let plan: @MainActor (Replacement) async throws -> ValueChange = { replacement in
                try await session.planDeletingVendor(id, replacement: replacement, cancellation: cancellation)
            }
            do {
                let unused = try await plan(.refuse)
                let approved = await confirm("Delete the Vendor “\(name)”?",
                                             "No Order uses it. Its Contacts are deleted with it, "
                                                + "which can’t be undone.",
                                             identifier: "vendor.delete", approve: "Delete", destructive: true)
                guard approved else { return nil }
                if selectedVendorID == id { selectedVendorID = nil }
                return unused
            } catch let refusal as ValueRefusal where refusal.inUse {
                let others = vendors.filter { $0.id != id && !$0.hidden }.map { (title: $0.name, id: $0.id) }
                let replaced = try await chooseReplacement(for: "the Vendor “\(name)”", others: others,
                                                           refusal: refusal, plan: plan)
                if replaced != nil, selectedVendorID == id { selectedVendorID = nil }
                return replaced
            }
        }
    }

    /// Merge N Vendors…: asks for the Vendors in precedence order, starting with the one the
    /// detail shows, which the owner selected first, and then the others as listed. The first
    /// is kept, each later one fills only the facts and Contacts it lacks, and every Order
    /// using one of them uses it.
    func mergeVendors(_ ids: Set<String>) {
        if deferNavigation({ [self] in mergeVendors(ids) }) { return }
        guard confirmDiscard(retry: { [self] in mergeVendors(ids) }) else { return }
        let shown = [selectedVendorID].compactMap { $0 }.filter(ids.contains)
        let ordered = shown + vendors.map(\.id).filter { ids.contains($0) && !shown.contains($0) }
        changeValues("Couldn’t merge the Vendors") { [self] session, cancellation in
            try await chooseVendorMerge(ordered, title: "Merge \(ordered.count) Vendors",
                                        explanation: "The first Vendor is kept. Each Vendor after it only fills "
                                            + "what the kept one still lacks: missing facts, and Contacts by Name. "
                                            + "Every Order using one of them uses the kept Vendor. "
                                            + "Drag the Vendors to change their order.",
                                        session: session, cancellation: cancellation)
        }
    }

    /// A Vendor renamed to another Vendor's Name offers to merge the two. Cancelling leaves the
    /// refused name in the editor, which can be reverted.
    func offerVendorMerge(_ editor: RecordEditor) {
        guard vendorEditor === editor, let id = editor.draft.recordID else { return }
        let name = editor.field(SPIELORDERS_VENDOR_FIELD_NAME)
        changeValues("Couldn’t merge the Vendors") { [self] session, cancellation in
            let other = try await session.vendorID(named: name)
            let explanation = "Another Vendor is already named “\(name)”. Merging the two keeps the first, "
                + "which fills what it lacks from the second and gains its Orders. "
                + "Drag the Vendors to change their order."
            return try await chooseVendorMerge([other, id], title: "Merge with “\(name)”?",
                                               explanation: explanation, session: session, cancellation: cancellation)
        }
    }

    /// The Package's own spelling of the value `name` names, other than `value`: a listed value
    /// spelled alike, once the library confirms it is that value, or else `name` as typed.
    private func spelling(of name: String, besides value: String, in scope: ValueScope,
                          session: PackageSession) async throws -> String {
        let listed = try await values(in: scope, session: session)
        return listed.first { !valueMatches($0, value) && valueMatches($0, name) } ?? name
    }

    private func values(in scope: ValueScope, session: PackageSession) async throws -> [String] {
        let listed = if let project = scope.project {
            try await session.groups(of: project)
        } else {
            try await session.values(scope.field)
        }
        return listed.map(\.name)
    }

    /// What the unused-value confirmation adds: a Project's groups go with it.
    private func unusedNote(_ name: String, in scope: ValueScope) -> String {
        let groups = scope == .projects && projects.contains {
            $0.project?.name.utf8.elementsEqual(name.utf8) == true && !$0.groups.isEmpty
        }
        return groups ? "No Order uses it. Its Order Groups move to No Project." : "No Order uses it."
    }

    private func chooseMerge(_ names: [String], in scope: ValueScope, title: String, explanation: String,
                             session: PackageSession, cancellation: LibraryCancellation) async throws -> ValueChange? {
        var plans: [ValueChange] = []
        for name in names {
            plans.append(try await session.planMerging(names, keeping: name, in: scope, cancellation: cancellation))
        }
        let choices = zip(names, plans).map { MergeChoice(name: $0, orderCount: $1.orderCount) }
        guard let kept = await dialogs.chooseKept(title, explanation: explanation, choices: choices,
                                                  identifier: "value.merge") else { return nil }
        valueSelection = [.value(scope, names[kept])]
        return plans[kept]
    }

    /// Asks for the Vendors `ids` in precedence order, planning each order the owner sets.
    private func chooseVendorMerge(_ ids: [String], title: String, explanation: String,
                                   session: PackageSession,
                                   cancellation: LibraryCancellation) async throws -> ValueChange? {
        let change = try await session.planMergingVendors(ids, cancellation: cancellation)
        let listed = try await listVendors(session)
        let vendors = ids.compactMap { id in listed.first { $0.id == id } }
        let planning = Planning()
        let plan: @MainActor ([VendorRow]) async throws -> VendorMerge = { vendors in
            let change = try await planning.run { cancellation in
                try await session.planMergingVendors(vendors.map(\.id), cancellation: cancellation)
            }
            return VendorMerge(vendors: vendors, change: change)
        }
        let initial = VendorMerge(vendors: vendors, change: change)
        let answer = await dialogs.reviewMerge(of: initial, title: title, explanation: explanation, plan: plan)
        await planning.finish()
        guard let merge = answer else { return nil }
        selectedVendorID = merge.vendors[0].id
        return merge.change
    }

    /// Asks what replaces `subject` in the Orders using it: None where the library accepts
    /// clearing it, or one of `others`. Throws `refusal` when there is nothing to offer.
    private func chooseReplacement(for subject: String, others: [(title: String, id: String)], refusal: ValueRefusal,
                                   plan: @escaping @MainActor (Replacement) async throws -> ValueChange) async throws -> ValueChange? {
        let cleared = try? await plan(.clear)
        let none = cleared.map { _ in [ReplacementChoice(title: "None", replacement: .clear)] } ?? []
        let choices = none + others.map { ReplacementChoice(title: $0.title, replacement: .with($0.id)) }
        guard let first = choices.first else { throw refusal }
        let counted: ValueChange
        if let cleared {
            counted = cleared
        } else {
            counted = try await plan(first.replacement)
        }
        let explanation = "Orders use \(subject). Choose what they use instead."
        return await dialogs.chooseReplacement("Delete \(subject)?", explanation: explanation,
                                               choices: choices, initial: first.replacement, change: counted,
                                               identifier: "value.replace", plan: plan)
    }

    private func listVendors(_ session: PackageSession) async throws -> [VendorRow] {
        try await session.vendors(query: "", caseSensitive: false, includeHidden: true,
                                  sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: false,
                                  cancellation: LibraryCancellation()).rows
    }

    /// Plans a change, which may ask the owner first and is nil when they decline, then commits
    /// it as the window's one mutation and lists everything it may have changed again. `then`
    /// follows a commit; without it, the change ends what Redo could repeat.
    private func changeValues(_ failure: String,
                              _ plan: @escaping ValuePlanning,
                              then: (@MainActor (CommittedValueChange) -> Void)? = nil,
                              failed: (@MainActor () -> Void)? = nil) {
        guard !busy, canBrowseRecords, let retainedFolder = folder else { return }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer {
                busy = false
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                guard let change = try await plan(session, cancellation) else { return }
                try Task.checkCancellation()
                let committed = try await session.commit(change)
                // History follows the write, even when its subsequent readback fails.
                if let then { then(committed) }
                else if committed.changed { endRedo() }
                discardEditors()
                if committed.requiresReopen {
                    let message = committed.changed
                        ? "The change was saved, but the Package must be reloaded before continuing."
                        : "The Package couldn’t be read again. Reload it before continuing."
                    requireReload(Notice(message,
                                         details: committed.warnings.joined(separator: "\n\n")))
                    clearOrders()
                    return
                }
                await refreshRecords(selectingOrder: selectedOrderID, vendor: selectedVendorID)
                await listValues()
                addWarnings(committed.warnings, to: destination)
            } catch {
                failed?()
                guard !Task.isCancelled else { return }
                alert(failure, (error as? ValueRefusal)?.summary ?? "", details: error.localizedDescription,
                      identifier: "value.change")
            }
        }
    }

    /// Lists the values the choosers offer, and the Projects while they are shown.
    private func listValues() async {
        await loadChoices(library)
        guard destination == .projects else { return }
        do {
            projects = try await library.projectGroups()
        } catch {
            warn(.projects, "Couldn’t list Projects.", error: error)
        }
    }
}

/// Plans a value change, possibly asking the owner first; nil when they decline.
typealias ValuePlanning = @MainActor (PackageSession, LibraryCancellation) async throws -> ValueChange?

/// A value + added, which Undo deletes again while no Order uses it and Redo adds back. An undo
/// manager does not retain its targets, so each registered action holds it.
final class AddedValue {
    let scope: ValueScope
    let name: String

    init(scope: ValueScope, name: String) {
        self.scope = scope
        self.name = name
    }
}

extension DesktopModel {
    fileprivate func offerUndo(_ added: AddedValue) {
        undoManager.registerUndo(withTarget: added) { [weak self] _ in self?.undoAdding(added) }
        undoManager.setActionName("Add \(added.scope.noun)")
    }

    /// Undo Add: deletes the value, refused once an Order uses it. Redo is registered at once.
    private func undoAdding(_ added: AddedValue) {
        undoManager.registerUndo(withTarget: added) { [weak self] _ in self?.redoAdding(added) }
        undoManager.setActionName("Add \(added.scope.noun)")
        replay(added, "Couldn’t undo adding “\(added.name)”") { session, cancellation in
            try await session.planDeleting(added.name, in: added.scope, replacement: .refuse,
                                           cancellation: cancellation)
        }
    }

    private func redoAdding(_ added: AddedValue) {
        offerUndo(added)
        replay(added, "Couldn’t add “\(added.name)” again") { session, cancellation in
            try await session.planAdding(added.name, to: added.scope, cancellation: cancellation)
        }
    }

    /// Applies an Undo or Redo of `added`; one that can't apply leaves the history, saying why.
    private func replay(_ added: AddedValue, _ failure: String,
                        _ plan: @escaping ValuePlanning) {
        guard !busy, canBrowseRecords, folder != nil else {
            undoManager.removeAllActions(withTarget: added)
            alert(failure, "Finish the current work, then try again.", identifier: "value.history")
            return
        }
        changeValues(failure, plan, then: { [self] committed in
            if !committed.changed { undoManager.removeAllActions(withTarget: added) }
        }, failed: { [self] in
            undoManager.removeAllActions(withTarget: added)
        })
    }
}
