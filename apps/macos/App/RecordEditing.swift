import AppKit
import CSpielOrders
import SpielOrdersKit

/// Opening, saving, and leaving the full Order and Vendor editors.
extension DesktopModel {
    var recordEditor: RecordEditor? { destination == .vendors ? vendorEditor : orderEditor }

    var canSaveRecord: Bool {
        guard !busy, vendorSheet == nil, let editor = recordEditor else { return false }
        // A new record's starting facts are already a valid record to save.
        return editor.isNew || editor.dirty
    }

    func openOrder(_ id: String?) {
        guard !busy, let row = orders.first(where: { $0.id == id }) else { return }
        if row.damage != nil { showDiagnosis(directory: row.directory); return }
        guard confirmDiscard(retry: { [self] in openOrder(id) }) else { return }
        loadEditor { session, cancellation in try await session.orderDraft(row, cancellation: cancellation) }
    }

    func newOrder() {
        guard !busy, canBrowseRecords, root != nil, confirmDiscard(retry: { [self] in newOrder() }) else { return }
        loadEditor { session, _ in try await session.newOrderDraft() }
    }

    func openVendor(_ id: String?) {
        guard !busy, let id, confirmDiscard(retry: { [self] in openVendor(id) }) else { return }
        loadEditor { session, cancellation in try await session.vendorDraft(id: id, cancellation: cancellation) }
    }

    func newVendor() {
        guard !busy, canBrowseRecords, root != nil, confirmDiscard(retry: { [self] in newVendor() }) else { return }
        loadEditor { session, _ in try await session.newVendorDraft() }
    }

    private func loadEditor(_ load: @escaping (PackageSession, LibraryCancellation) async throws -> RecordDraft) {
        guard let retainedFolder = folder else { return }
        let session = library
        busy = true
        message = ""
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let draft = try await withTaskCancellationHandler {
                    try await load(session, cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                if draft.kind == .order { await loadChoices(session) }
                discardEditors()
                text = original
                editingOrder = nil
                let editor = RecordEditor(draft)
                if draft.kind == .order { orderEditor = editor; destination = .orders }
                else { vendorEditor = editor; destination = .vendors }
            } catch { if !Task.isCancelled { message = "Couldn’t open the editor.\n" + error.localizedDescription } }
        }
    }

    /// The Vendors and reusable values the Order editor offers.
    private func loadChoices(_ session: PackageSession) async {
        do {
            vendorChoices = try await session.vendors(query: "", caseSensitive: false, includeHidden: false,
                                                      sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: false,
                                                      cancellation: LibraryCancellation()).rows.map(\.name)
            var values: [Int32: [String]] = [:]
            for field in [SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
                          SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDER_GROUP] {
                values[field] = try await session.values(field)
            }
            reusableValues = values
        } catch {
            message = "Suggestions are unavailable.\n" + error.localizedDescription
        }
    }

    func closeRecordEditor() {
        guard !busy, let editor = recordEditor, confirmDiscard(retry: { [self] in closeRecordEditor() }) else { return }
        let id = editor.draft.recordID
        discardEditors()
        if editor.kind == .order {
            selectedOrderID = nil
            if let id { selectOrder(id, revealInFinder: false) }
        } else {
            selectedVendorID = nil
            selectVendor(id)
        }
    }

    func discardEditors() {
        for editor in [orderEditor, vendorEditor, vendorSheet] { editor?.close() }
        orderEditor = nil
        vendorEditor = nil
        vendorSheet = nil
        recoveryEditor = nil
        diagnosis = nil
        diagnosticWorkspace = false
        expertDocument = nil
        rawText = ""
        rawOriginal = ""
    }

    func saveRecord() {
        guard canSaveRecord, let editor = recordEditor else { return }
        save(editor) { [self] saved in
            if let draft = saved.draft { editor.saved(draft) }
            else { discardEditors() }
            await refreshRecords(selectingOrder: editor.kind == .order ? saved.recordID : selectedOrderID,
                                 vendor: editor.kind == .vendor ? saved.recordID : selectedVendorID)
        }
    }

    func beginVendorSheet() {
        guard !busy, orderEditor != nil, vendorSheet == nil else { return }
        let session = library
        busy = true
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false }
            do {
                let editor = RecordEditor(try await session.newVendorDraft())
                vendorSheet = editor
                if !dialogs.presentVendor(editor, model: self) { vendorSheet = nil }
            }
            catch { message = "Couldn’t start a new Vendor.\n" + error.localizedDescription }
        }
    }

    func cancelVendorSheet() {
        guard !busy, let sheet = vendorSheet else { return }
        busy = true
        Task {
            let discard = sheet.dirty ? await confirm("Discard this new Vendor?", "The Vendor has not been saved.",
                                                       identifier: "vendor.sheet.discard", destructive: true) : true
            busy = false
            guard discard, vendorSheet === sheet else { return }
            sheet.close()
            vendorSheet = nil
        }
    }

    func saveVendorSheet() {
        guard !busy, let sheet = vendorSheet, let order = orderEditor else { return }
        save(sheet) { [self] saved in
            let name = saved.draft?.field(SPIELORDERS_VENDOR_FIELD_NAME) ?? sheet.field(SPIELORDERS_VENDOR_FIELD_NAME)
            sheet.close()
            vendorSheet = nil
            await loadChoices(library)
            order.set(SPIELORDERS_ORDER_FIELD_VENDOR, name)
            await refreshRecords(selectingOrder: selectedOrderID, vendor: selectedVendorID)
        }
    }

    /// Saves `editor`'s draft. A refusal for new reusable values asks to confirm them
    /// and saves again; any other refusal keeps the draft and shows where it failed.
    private func save(_ editor: RecordEditor, then committed: @escaping @MainActor (SavedRecord) async -> Void) {
        guard let retainedFolder = folder else { return }
        let session = library
        busy = true
        message = ""
        editor.clearProblems()
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            var confirmed = false
            while true {
                do {
                    let saved = try await session.save(editor.draft, cancellation: cancellation)
                    await committed(saved)
                    if !saved.warnings.isEmpty || saved.requiresReopen {
                        message = ([message, "Saved with warnings."].filter { !$0.isEmpty } + saved.warnings
                            + (saved.requiresReopen ? ["Reopen the Package before continuing."] : [])).joined(separator: "\n")
                    }
                    return
                } catch let refusal as SaveRefusal {
                    let fields = refusal.unconfirmedFields
                    if !confirmed, !fields.isEmpty, refusal.problems.allSatisfy({ fields.contains($0.field) }),
                       await confirmNewValues(fields.map { ($0, editor.field($0)) }) {
                        for field in fields { try? editor.draft.confirm(field) }
                        confirmed = true
                        continue
                    }
                    editor.refuse(refusal)
                    if refusal.stale { message = "This record changed on disk after it was opened. Your draft is kept; reopen the record to see the current version." }
                    return
                } catch {
                    editor.refuse(SaveRefusal.unavailable(error.localizedDescription))
                    return
                }
            }
        }
    }

    private func confirmNewValues(_ values: [(Int32, String)]) async -> Bool {
        await confirm("Add new values?",
                values.map { "\(RecordFieldLabel.of($0.0)): \($0.1)" }.joined(separator: "\n")
                    + "\nThe Package does not use these values yet.",
                identifier: "order.editor.newValues", approve: "Add Values")
    }

    /// Lists Orders and Vendors again after a commit, keeping the given selections.
    func refreshRecords(selectingOrder orderID: String?, vendor vendorID: String?) async {
        let session = library
        do {
            let all = try await session.orders(query: "", caseSensitive: false, sortField: orderSortField,
                                               descending: orderSortDescending, cancellation: LibraryCancellation())
            orderCount = all.rows.count
            orders = query.isEmpty ? all.rows
                : try await session.orders(query: query, caseSensitive: caseSensitive, sortField: orderSortField,
                                           descending: orderSortDescending, cancellation: LibraryCancellation()).rows
            vendors = try await session.vendors(query: destination == .vendors ? query : "", caseSensitive: caseSensitive,
                                                includeHidden: includeHiddenVendors, sortField: vendorSortField,
                                                descending: vendorSortDescending, cancellation: LibraryCancellation()).rows
            selectedOrderID = orders.first { $0.id == orderID }?.id
            inspection = nil
            if let row = orders.first(where: { $0.id == selectedOrderID }) {
                inspection = try? await session.inspectOrder(row, cancellation: LibraryCancellation())
            }
            selectedVendorID = vendors.first { $0.id == vendorID }?.id
            vendorDetails = nil
            if let row = vendors.first(where: { $0.id == selectedVendorID }) {
                vendorDetails = try? await session.inspectVendor(row, cancellation: LibraryCancellation())
            }
        } catch {
            canBrowseRecords = false
            writable = false
            orders = []
            vendors = []
            selectedOrderID = nil
            selectedVendorID = nil
            inspection = nil
            vendorDetails = nil
            searchAnswer = nil
            message = ([message, "The change was committed, but the lists could not be refreshed; reopen the Package.",
                        error.localizedDescription].filter { !$0.isEmpty }).joined(separator: "\n")
        }
    }

    func setSelectedVendorHidden(_ hidden: Bool) {
        guard !busy, let id = selectedVendorID, let retainedFolder = folder, confirmDiscard(retry: { [self] in setSelectedVendorHidden(hidden) }) else { return }
        let session = library
        busy = true
        message = ""
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let warnings = try await session.setVendorHidden(id: id, hidden: hidden, cancellation: cancellation)
                await refreshRecords(selectingOrder: selectedOrderID, vendor: id)
                if !warnings.isEmpty { message = ([message, "Saved with warnings."].filter { !$0.isEmpty } + warnings).joined(separator: "\n") }
            } catch { message = "Couldn’t change the Vendor’s visibility.\n" + error.localizedDescription }
        }
    }
}

extension SaveRefusal {
    static func unavailable(_ message: String) -> SaveRefusal {
        SaveRefusal(result: SPIELORDERS_RESULT_REFUSED, message: message)
    }
}
