import AppKit
import CSpielOrders
import SpielOrdersKit

/// A selected record being read for the detail pane while the record before it stays shown.
struct SelectionRead {
    let id: String
    /// Whether Open asked for the first field once the record shows.
    var focus: Bool
    /// An ended field in the still-shown record waits until this read releases the gate.
    var afterRead: (() -> Void)?
}

/// What an editor opens on.
private enum EditorSource {
    case order(OrderRow)
    case vendor(VendorRow)
    case newOrder
    case newVendor

    /// The listed record this source reads; a new record has none yet.
    var id: String? {
        switch self {
        case let .order(row): row.id
        case let .vendor(row): row.id
        case .newOrder, .newVendor: nil
        }
    }

    /// What an alert says when this source cannot be read.
    var failure: String {
        switch self {
        case .order: "The Order couldn’t be read"
        case .vendor: "The Vendor couldn’t be read"
        case .newOrder: "Couldn’t start a new Order"
        case .newVendor: "Couldn’t start a new Vendor"
        }
    }
}

/// A record's draft with the details its pane shows beside the form.
private struct OpenedRecord {
    let draft: RecordDraft
    var order: OrderRow?
    var orderDetails: OrderInspection?
    var vendor: VendorRow?
    var vendorDetails: VendorDetails?
    var readme: ReadmeText?
}

/// Selecting, creating, and leaving Orders and Vendors. The selected record is always shown
/// in its editor, whose fields commit as they end; only a new record waits for Create.
extension DesktopModel {
    var recordEditor: RecordEditor? { destination == .vendors ? vendorEditor : orderEditor }

    /// Whether Create can write the new record staged in the window or the Vendor sheet.
    var canCreateRecord: Bool {
        guard canBrowseRecords, !busy || committing else { return false }
        return vendorSheet != nil || recordEditor?.isNew == true
    }

    /// Whether `editor` accepts input. An existing record's input buffers while its own save
    /// settles, and stays live while the next selection is read.
    func canType(_ editor: RecordEditor) -> Bool {
        guard canBrowseRecords else { return false }
        guard busy else { return true }
        if editor.isNew { return false }
        if selectionRead != nil { return true }
        return committing && !navigationPending && !dialogs.isPresenting
    }

    /// Lists stay live while a commit settles or the next selection is read.
    var listsAvailable: Bool { !busy || committing || selectionRead != nil }

    /// Shows the selected Order in its editor, or what the library can tell about an Order
    /// that cannot be edited: a damaged one, or any while the Package needs Reload.
    func selectOrder(_ id: String?, focus: Bool = false) {
        if let editor = orderEditor, id == selectedOrderID, id == editor.draft.recordID { return }
        if deferNavigation({ [self] in selectOrder(id, focus: focus) }) { return }
        guard !busy, folder != nil,
              confirmDiscard(retry: { [self] in selectOrder(id, focus: focus) }) else { return }
        multipleSelection = []
        guard let row = orderRow(id) else {
            discardEditors()
            selectedOrderID = nil
            orderContext = nil
            inspection = nil
            return
        }
        // The list keeps the new selection while the detail still shows the previous Order.
        selectedOrderID = row.id
        if row.damage == nil, canBrowseRecords {
            loadEditor(.order(row), focus: focus)
        } else {
            inspect(row)
        }
    }

    /// Open: focuses the Order's first field, showing the Order first when another is shown.
    func openOrder(_ id: String?) {
        if deferNavigation({ [self] in openOrder(id) }) { return }
        guard let id else { return }
        if let editor = orderEditor, editor.draft.recordID == id {
            editor.focus()
        } else if selectionRead?.id == id {
            selectionRead?.focus = true
        } else if let row = orderRow(id), row.damage != nil {
            showDiagnosis(directory: row.directory)
        } else {
            selectOrder(id, focus: true)
        }
    }

    /// File → New: a new record of the kind the current destination lists.
    func newRecord() {
        switch destination {
        case .orders: newOrder()
        case .vendors: newVendor()
        case let .values(list): beginNaming(.new(list.scope))
        case .projects: beginNaming(.new(.projects))
        case .packageReadme: break
        }
    }

    /// Whether File → New can start a record of the current destination's kind.
    var canStartNew: Bool {
        guard destination.creates, !diagnosticWorkspace, canBrowseRecords, vendorSheet == nil else { return false }
        return !busy || committing
    }

    func newOrder() {
        if deferNavigation({ [self] in newOrder() }) { return }
        guard !busy, canBrowseRecords, root != nil, confirmDiscard(retry: { [self] in newOrder() }) else { return }
        loadEditor(.newOrder)
    }

    /// Shows the selected Vendor in its editor, or its details while the Package needs Reload.
    func selectVendor(_ id: String?, focus: Bool = false) {
        if let editor = vendorEditor, id == selectedVendorID, id == editor.draft.recordID { return }
        if deferNavigation({ [self] in selectVendor(id, focus: focus) }) { return }
        guard !busy, folder != nil,
              confirmDiscard(retry: { [self] in selectVendor(id, focus: focus) }) else { return }
        multipleVendorSelection = []
        guard let row = vendorRow(id) else {
            discardEditors()
            selectedVendorID = nil
            vendorContext = nil
            vendorDetails = nil
            return
        }
        selectedVendorID = row.id
        if canBrowseRecords {
            loadEditor(.vendor(row), focus: focus)
        } else {
            inspect(row)
        }
    }

    /// Open: focuses the Vendor's first field, showing the Vendor first when another is shown.
    func openVendor(_ id: String?) {
        if deferNavigation({ [self] in openVendor(id) }) { return }
        guard let id else { return }
        if let editor = vendorEditor, editor.draft.recordID == id {
            editor.focus()
        } else if selectionRead?.id == id {
            selectionRead?.focus = true
        } else {
            selectVendor(id, focus: true)
        }
    }

    func newVendor() {
        if deferNavigation({ [self] in newVendor() }) { return }
        guard !busy, canBrowseRecords, root != nil, confirmDiscard(retry: { [self] in newVendor() }) else { return }
        loadEditor(.newVendor)
    }

    /// Shows an Order listed in the Vendor's detail.
    func openRelatedOrder(_ row: OrderRow) {
        guard !busy, confirmDiscard(retry: { [self] in openRelatedOrder(row) }) else { return }
        loadEditor(.order(row))
    }

    func orderRow(_ id: String?) -> OrderRow? {
        orders.first { $0.id == id } ?? (orderContext?.id == id ? orderContext : nil)
    }

    private func vendorRow(_ id: String?) -> VendorRow? {
        vendors.first { $0.id == id } ?? (vendorContext?.id == id ? vendorContext : nil)
    }

    /// Reads `source` into an editor that replaces the shown one. The shown editor stays
    /// until then; input it receives meanwhile keeps it, and the selection returns to it.
    private func loadEditor(_ source: EditorSource, focus: Bool = false) {
        guard let retainedFolder = folder else { return }
        let session = library
        let shown = recordEditor
        let revision = shown?.revision
        busy = true
        selectionRead = source.id.map { SelectionRead(id: $0, focus: focus) }
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer {
                let field = selectionRead?.afterRead
                busy = false
                selectionRead = nil
                field?()
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let opened = try await withTaskCancellationHandler {
                    try await read(source, session: session, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                guard shown?.revision == revision else {
                    restoreSelection(of: source)
                    return
                }
                show(opened, focus: selectionRead?.focus ?? focus)
            } catch {
                restoreSelection(of: source)
                if !Task.isCancelled {
                    alert(source.failure, "Reload the Package, then try again.",
                          details: error.localizedDescription, identifier: "selection.read")
                }
            }
        }
    }

    /// Reads the selected record's editor where a refresh or navigation left the detail
    /// without one. A selection that cannot be read is cleared, with the reason.
    func openSelection(_ session: PackageSession, cancellation: LibraryCancellation) async {
        guard canBrowseRecords, !diagnosticWorkspace else { return }
        let source: EditorSource
        switch destination {
        case .orders:
            guard orderEditor == nil, let row = orderContext, row.id == selectedOrderID, row.damage == nil else {
                return
            }
            source = .order(row)
        case .vendors:
            guard vendorEditor == nil, let row = vendorContext, row.id == selectedVendorID else { return }
            source = .vendor(row)
        case .values, .projects, .packageReadme:
            return
        }
        do {
            let opened = try await withTaskCancellationHandler {
                try await read(source, session: session, cancellation: cancellation)
            } onCancel: { cancellation.request() }
            try Task.checkCancellation()
            show(opened, focus: false)
        } catch {
            if destination == .orders {
                selectedOrderID = nil
                orderContext = nil
                inspection = nil
            } else {
                selectedVendorID = nil
                vendorContext = nil
                vendorDetails = nil
            }
            if !Task.isCancelled {
                warn(destination, "The selection couldn’t be read. Reload the Package, then try again.", error: error)
            }
        }
    }

    /// Shows the existing Order or Vendor `id` in its editor, wherever the window is, as Open
    /// does from a list, then calls `shown` once the window is free again. A record that is
    /// gone or can't be read is not shown, and `shown` still runs to say so.
    func showRecord(_ kind: RecordDraft.Kind, id: String, then shown: @escaping @MainActor () -> Void) {
        guard !busy, let retainedFolder = folder else { return }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer {
                busy = false
                if !Task.isCancelled { shown() }
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let opened: OpenedRecord? = try await withTaskCancellationHandler {
                    let source = try await listedSource(kind, id: id, session: session, cancellation: cancellation)
                    guard let source else { return nil }
                    return try await read(source, session: session, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                if let opened { show(opened, focus: false) }
            } catch {
                // `shown` finds the record unshown and says why.
            }
        }
    }

    /// The editable record `id` as the Package lists it now, or nil when it is gone or damaged.
    private func listedSource(_ kind: RecordDraft.Kind, id: String, session: PackageSession,
                              cancellation: LibraryCancellation) async throws -> EditorSource? {
        switch kind {
        case .order:
            let listed = try await session.orders(query: "", caseSensitive: false, sortField: orderSortField,
                                                  descending: orderSortDescending, cancellation: cancellation)
            guard let row = listed.rows.first(where: { $0.id == id }), row.damage == nil else { return nil }
            return .order(row)
        case .vendor:
            let listed = try await session.vendors(query: "", caseSensitive: false, includeHidden: true,
                                                   sortField: vendorSortField, descending: vendorSortDescending,
                                                   cancellation: cancellation)
            return listed.rows.first { $0.id == id }.map(EditorSource.vendor)
        }
    }

    private func read(_ source: EditorSource, session: PackageSession,
                      cancellation: LibraryCancellation) async throws -> OpenedRecord {
        switch source {
        case let .order(row):
            let draft = try await session.orderDraft(row, cancellation: cancellation)
            try Task.checkCancellation()
            await loadChoices(session)
            let details = try await session.inspectOrder(row, cancellation: cancellation)
            let readme = try await session.readme(of: row, cancellation: cancellation)
            return OpenedRecord(draft: draft, order: row, orderDetails: details, readme: readme)
        case let .vendor(row):
            let draft = try await session.vendorDraft(id: row.id, cancellation: cancellation)
            let details = try await session.inspectVendor(row, cancellation: cancellation)
            return OpenedRecord(draft: draft, vendor: row, vendorDetails: details)
        case .newOrder:
            let draft = try await session.newOrderDraft()
            await loadChoices(session)
            return OpenedRecord(draft: draft)
        case .newVendor:
            return OpenedRecord(draft: try await session.newVendorDraft())
        }
    }

    private func show(_ opened: OpenedRecord, focus: Bool) {
        discardEditors()
        let editor = makeEditor(opened.draft)
        if opened.draft.kind == .order {
            orderEditor = editor
            destination = .orders
            multipleSelection = []
            selectedOrderID = opened.draft.recordID
            orderContext = opened.order
            inspection = opened.orderDetails
            if let readme = opened.readme { showReadme(readme, of: opened.order) }
        } else {
            vendorEditor = editor
            destination = .vendors
            multipleVendorSelection = []
            selectedVendorID = opened.draft.recordID
            vendorContext = opened.vendor
            vendorDetails = opened.vendorDetails
        }
        if focus { editor.focus() }
    }

    /// The list selection returns to the record the detail still shows.
    private func restoreSelection(of source: EditorSource) {
        switch source {
        case .order: selectedOrderID = orderContext?.id
        case .vendor: selectedVendorID = vendorContext?.id
        case .newOrder, .newVendor: break
        }
    }

    /// Reads what the library can tell about an Order that cannot be shown in its editor.
    private func inspect(_ row: OrderRow) {
        guard let retainedFolder = folder else { return }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let details = try await withTaskCancellationHandler {
                    try await session.inspectOrder(row, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                discardEditors()
                orderContext = row
                inspection = details
            } catch {
                selectedOrderID = orderContext?.id
                if !Task.isCancelled {
                    alert("The Order couldn’t be read", "Reload the Package, then try again.",
                          details: error.localizedDescription, identifier: "selection.read")
                }
            }
        }
    }

    /// Reads a Vendor's details while the Package needs Reload before it can be edited.
    private func inspect(_ row: VendorRow) {
        guard let retainedFolder = folder else { return }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let details = try await withTaskCancellationHandler {
                    try await session.inspectVendor(row, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                discardEditors()
                vendorContext = row
                vendorDetails = details
            } catch {
                selectedVendorID = vendorContext?.id
                if !Task.isCancelled {
                    alert("The Vendor couldn’t be read", "Reload the Package, then try again.",
                          details: error.localizedDescription, identifier: "selection.read")
                }
            }
        }
    }

    private func makeEditor(_ draft: RecordDraft) -> RecordEditor {
        let editor = RecordEditor(draft)
        editor.window = dialogs.window
        editor.onChange = { [weak self] change in self?.register(change) }
        editor.onInput = { [weak self] in self?.endRedo() }
        editor.onFinish = { [weak self, weak editor] in
            guard let self, let editor else { return }
            if !editor.dirty {
                // Input returned to the saved record leaves nothing Not Saved.
                if editor.refused, !editor.isNew { editor.clearProblems() }
            } else if !editor.isNew {
                commitRecord(editor)
            }
        }
        return editor
    }

    /// The Vendors and reusable values the Order editor offers.
    func loadChoices(_ session: PackageSession) async {
        do {
            vendorChoices = try await session.vendors(query: "", caseSensitive: false, includeHidden: false,
                                                      sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: false,
                                                      cancellation: LibraryCancellation()).rows.map(\.name)
            knownVendors = try await session.vendors(query: "", caseSensitive: false, includeHidden: true,
                                                     sortField: SPIELORDERS_VENDOR_FIELD_NAME, descending: false,
                                                     cancellation: LibraryCancellation()).rows.map(\.name)
            var values: [Int32: [ReusableValue]] = [:]
            for field in [SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
                          SPIELORDERS_ORDER_FIELD_PROJECT] {
                values[field] = try await session.values(field)
            }
            reusableValues = values
        } catch {
            // The choosers offer what was last read; the library still judges each save.
        }
    }

    /// The Order Groups the editor suggests for an Order in `project`, or in none for "".
    func groupChoices(in project: String) async -> [String] {
        // The chooser offers no group until a read succeeds; the library still judges each save.
        (try? await library.groups(of: project).map(\.name)) ?? []
    }

    /// Cancel: discards a new record that was never created.
    func cancelRecord() {
        if deferNavigation({ [self] in cancelRecord() }) { return }
        guard !busy, recordEditor?.isNew == true,
              confirmDiscard(retry: { [self] in cancelRecord() }) else { return }
        discardEditors()
    }

    /// Revert: replaces input a save refused with the record as it is saved.
    func revertRecord() {
        guard !busy, let editor = recordEditor, !editor.isNew else { return }
        busy = true
        Task {
            let discard = await confirm("Discard unsaved changes?", "The changes that couldn’t be saved will be lost.",
                                        identifier: "editor.revert", approve: "Revert", destructive: true)
            busy = false
            // Revert reads the Order again, README included; README text is saved first.
            guard discard, recordEditor === editor, await settleReadme() else { return }
            if editor.kind == .order, let row = currentOrder {
                loadEditor(.order(row))
            } else if editor.kind == .vendor, let row = vendorRow(editor.draft.recordID) {
                loadEditor(.vendor(row))
            }
        }
    }

    func discardEditors() {
        for editor in [orderEditor, vendorEditor, vendorSheet] { editor?.close() }
        orderEditor = nil
        vendorEditor = nil
        vendorSheet = nil
        readme = nil
        recoveryEditor = nil
        diagnosis = nil
        diagnosticWorkspace = false
        expertDocument = nil
        rawText = ""
        rawOriginal = ""
    }

    /// Create: writes the new record staged in the window.
    func createRecord() {
        if deferNavigation({ [self] in createRecord() }) { return }
        guard !busy, let editor = recordEditor, editor.isNew else { return }
        commitRecord(editor)
    }

    /// Commits an existing record's ended field, or creates a new record, then lists the
    /// result. A record saved without a readable replacement asks for Reload instead.
    func commitRecord(_ editor: RecordEditor) {
        guard vendorSheet == nil else { return }
        guard !busy else {
            let finish = { [weak self, weak editor] in
                guard let self, let editor else { return }
                commitRecord(editor)
            }
            if committing, let readme, readme.draft.saving {
                readme.afterSave = finish
            } else if selectionRead != nil {
                selectionRead?.afterRead = finish
            } else {
                // Other work, such as a lookup that disabled the field, saves it once it ends.
                waiting.whenFree { [weak self, weak editor] in
                    guard let self, let editor, [orderEditor, vendorEditor].contains(where: { $0 === editor }) else { return }
                    commitRecord(editor)
                }
            }
            return
        }
        guard canBrowseRecords, folder != nil else {
            if editor.dirty || editor.isNew {
                let record = editor.kind == .order ? "Order" : "Vendor"
                editor.requireReopen("This \(record) has changes that can’t be saved until the Package is reloaded. "
                    + "Reload the Package to keep editing; it asks before discarding them.")
            }
            return
        }
        guard editor.dirty || editor.isNew else { return }
        let creating = editor.isNew
        committing = true
        save(editor) { [self] saved in
            editor.saved(saved.draft)
            if saved.requiresReopen {
                requireReload(Notice("The change was saved. Reload the Package to keep editing.",
                                     details: saved.warnings.joined(separator: "\n\n")))
                if editor.dirty {
                    editor.requireReopen("The submitted change was saved. Later input is kept here. "
                        + "Reload the Package to keep editing; it asks before discarding that input.")
                }
            }
            guard creating || !saved.requiresReopen else { return }
            await refreshRecords(selectingOrder: editor.kind == .order ? saved.recordID : selectedOrderID,
                                 vendor: editor.kind == .vendor ? saved.recordID : selectedVendorID)
            // A save may add a value or rename a Vendor that the choosers then offer.
            await loadChoices(library)
            if creating, editor.kind == .order, canBrowseRecords, let row = orderContext {
                do {
                    let text = try await library.readme(of: row, cancellation: LibraryCancellation())
                    showReadme(text, of: row)
                } catch {
                    editor.warn(Notice("The Order was created, but its README couldn’t be read. Reload the Package.",
                                       details: error.localizedDescription))
                }
            }
        }
    }

    func retryRecord() {
        guard let editor = recordEditor, editor.refused else { return }
        if editor.isNew {
            createRecord()
        } else {
            commitRecord(editor)
        }
    }

    /// New Vendor “name”… from an Order's Vendor field: the Vendor sheet, starting on `name`,
    /// over the Order draft, which then uses the Vendor it creates.
    func beginVendorSheet(named name: String) {
        if deferNavigation({ [self] in beginVendorSheet(named: name) }) { return }
        guard !busy, orderEditor != nil, vendorSheet == nil else { return }
        let session = library
        busy = true
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false }
            do {
                let draft = try await session.newVendorDraft()
                try draft.set(SPIELORDERS_VENDOR_FIELD_NAME, to: name)
                let editor = makeEditor(draft)
                vendorSheet = editor
                if !dialogs.presentVendor(editor, model: self) { vendorSheet = nil }
            } catch {
                alert("Couldn’t start a new Vendor", details: error.localizedDescription, identifier: "vendor.new")
            }
        }
    }

    /// New Orderer “text”… and its kin in an Order field: once confirmed, the field takes
    /// `text` and its save adds it, so the Order and the new value are one guarded write. A new
    /// Order adds it when it is created.
    func addValue(_ text: String, at field: Int32, in editor: RecordEditor) {
        guard field != SPIELORDERS_ORDER_FIELD_VENDOR else {
            beginVendorSheet(named: text)
            return
        }
        if deferNavigation({ [self] in addValue(text, at: field, in: editor) }) { return }
        guard !busy else { return }
        let kind = ValueChooser.kind(of: field)
        let reach = field == SPIELORDERS_ORDER_FIELD_ORDER_GROUP
            ? groupReach(editor) : "It becomes one of the Package’s \(kind)s, offered to every Order."
        busy = true
        Task {
            let added = await confirm("Add “\(text)” as a new \(kind)?", reach,
                                      identifier: "order.editor.addValue", approve: "Add \(kind)")
            busy = false
            guard added, orderEditor === editor else { return }
            editor.add(field, text)
            editor.finishField()
        }
    }

    /// Which Orders are offered an Order Group added to `editor`'s Order, as a sentence.
    private func groupReach(_ editor: RecordEditor) -> String {
        let project = editor.field(SPIELORDERS_ORDER_FIELD_PROJECT)
        return project.isEmpty ? "It becomes an Order Group for Orders with no Project."
            : "It becomes an Order Group in “\(project)”, offered to that Project’s Orders."
    }

    func cancelVendorSheet() {
        guard !busy, let sheet = vendorSheet else { return }
        busy = true
        Task {
            let discard = sheet.dirty ? await confirm("Discard this new Vendor?", "It hasn’t been created yet. Choose Create to keep it.",
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
            afterCommit = { [weak parent = order] in parent?.finishField() }
            await refreshRecords(selectingOrder: selectedOrderID, vendor: selectedVendorID)
        }
    }

    /// Saves `editor`'s draft. A refusal keeps the draft and shows where it failed.
    private func save(_ editor: RecordEditor, then committed: @escaping @MainActor (SavedRecord) async -> Void) {
        guard let retainedFolder = folder else { return }
        let session = library
        editor.beginSave()
        busy = true
        if !continuingSave { editor.clearWarnings() }
        editor.clearProblems()
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            var completed = false
            defer {
                busy = false
                committing = false
                let reserved = navigationPending
                navigationPending = false
                let wasContinuing = continuingSave
                continuingSave = true
                editor.finishSave()
                navigationPending = reserved
                // A field that ended during this write saves next. Keep the one
                // requested navigation/corrective action until those writes settle.
                if !committing {
                    let next = afterCommit
                    afterCommit = nil
                    let continueAfterFailure = self.continueAfterFailure
                    self.continueAfterFailure = false
                    if completed || continueAfterFailure, !navigationPending { next?() }
                }
                continuingSave = wasContinuing
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let saved = try await session.save(editor.draft, cancellation: cancellation)
                await committed(saved)
                completed = true
                editor.warn(saved.warnings)
                if let group = saved.cleared[SPIELORDERS_ORDER_FIELD_ORDER_GROUP] {
                    let project = saved.draft?.field(SPIELORDERS_ORDER_FIELD_PROJECT)
                    noteClearedGroup(group, project: project, in: editor)
                }
            } catch let refusal as SaveRefusal {
                let explanation = refusal.recordChanged
                    ? "It changed on disk. Your input is kept; Revert shows the current version."
                    : refusal.stale
                    ? "Something it uses changed on disk. Your input is kept; Retry saves it."
                    : "Your input is kept. Correct the marked problems, then Retry."
                editor.refuse(refusal, failure: Notice(explanation, details: refusal.localizedDescription))
                if editor === vendorEditor, !editor.isNew, refusal.problems.contains(where: \.nameTaken) {
                    Task {
                        await waitForWork()
                        offerVendorMerge(editor)
                    }
                }
            } catch {
                let failure = Notice("Your input is kept. Retry, or reload the Package.",
                                     details: error.localizedDescription)
                editor.refuse(.unavailable("The change couldn’t be saved."), failure: failure)
            }
        }
    }

    /// Marks the Order Group the library left empty on a save: the group belongs to the
    /// Project the Order left.
    private func noteClearedGroup(_ group: String, project: String?, in editor: RecordEditor) {
        let owner = switch project {
        case nil: "the new Project has"
        case ""?: "Orders with no Project have"
        case let project?: "“\(project)” has"
        }
        let consequence = "Order Group cleared: \(owner) no group named “\(group)”."
        editor.note(consequence, at: SPIELORDERS_ORDER_FIELD_ORDER_GROUP)
    }

    /// Lists Orders and Vendors again after a commit, keeping the given selections, and shows
    /// a selection the detail has no editor for.
    func refreshRecords(selectingOrder orderID: String?, vendor vendorID: String?) async {
        let session = library
        do {
            let all = try await session.orders(query: "", caseSensitive: false, sortField: orderSortField,
                                               descending: orderSortDescending, cancellation: LibraryCancellation())
            orderCount = all.rows.count
            let orderQuery = destination == .orders ? search : RecordQuery()
            orders = orderQuery.text.isEmpty ? all.rows
                : try await searchOrders(session, orderQuery, sortField: orderSortField, descending: orderSortDescending,
                                         cancellation: LibraryCancellation()).rows
            let vendorQuery = destination == .vendors ? search : RecordQuery()
            let listedVendors = try await searchVendors(session, vendorQuery,
                                                        sortField: vendorSortField,
                                                        descending: vendorSortDescending,
                                                        cancellation: LibraryCancellation())
            vendors = listedVendors.rows
            vendorCount = vendorQuery.text.isEmpty ? vendors.count
                : try await countVendors(session, cancellation: LibraryCancellation())
            orderContext = all.rows.first { $0.id == orderID }
            selectedOrderID = orderContext?.id
            // Details are replaced once read, so the pane beside a form being typed in holds still.
            if let row = orderContext {
                do {
                    inspection = try await session.inspectOrder(row, cancellation: LibraryCancellation())
                } catch {
                    inspection = nil
                    orderEditor?.warn(Notice("The change was saved, but the Order’s details couldn’t be read. "
                        + "Reload the Package.", details: error.localizedDescription))
                }
            } else {
                inspection = nil
            }
            let allVendors = try await session.vendors(query: "", caseSensitive: false, includeHidden: true,
                sortField: vendorSortField, descending: vendorSortDescending, cancellation: LibraryCancellation())
            vendorContext = allVendors.rows.first { $0.id == vendorID }
            selectedVendorID = vendorContext?.id
            if let row = vendorContext {
                do {
                    vendorDetails = try await session.inspectVendor(row, cancellation: LibraryCancellation())
                } catch {
                    vendorDetails = nil
                    vendorEditor?.warn(Notice("The change was saved, but the Vendor’s details couldn’t be read. "
                        + "Reload the Package.", details: error.localizedDescription))
                }
            } else {
                vendorDetails = nil
            }
            showWarnings(orders: Notice.list(await session.findings + all.notices),
                         vendors: Notice.list(listedVendors.notices + allVendors.notices))
            await openSelection(session, cancellation: LibraryCancellation())
        } catch {
            requireReload(Notice("The change was saved, but the lists couldn’t be read again.",
                                 details: error.localizedDescription))
            orders = []
            vendors = []
            selectedOrderID = nil
            selectedVendorID = nil
            inspection = nil
            vendorDetails = nil
            contents.stop()
        }
    }

    /// Hides or shows the Vendor, which the detail then shows with any warnings.
    func setVendorHidden(_ hidden: Bool, vendor id: String) {
        if deferNavigation({ [self] in setVendorHidden(hidden, vendor: id) }) { return }
        guard !busy, let retainedFolder = folder,
              confirmDiscard(retry: { [self] in setVendorHidden(hidden, vendor: id) }) else { return }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let warnings = try await session.setVendorHidden(id: id, hidden: hidden, cancellation: cancellation)
                // Visibility is a separate write; the editor is read again with its current original.
                vendorEditor?.close()
                vendorEditor = nil
                await refreshRecords(selectingOrder: selectedOrderID, vendor: id)
                vendorEditor?.warn(warnings)
            } catch {
                alert(hidden ? "Couldn’t hide the Vendor" : "Couldn’t show the Vendor",
                      details: error.localizedDescription, identifier: "vendor.visibility")
            }
        }
    }
}

extension SaveRefusal {
    static func unavailable(_ message: String) -> SaveRefusal {
        SaveRefusal(result: SPIELORDERS_RESULT_REFUSED, message: message)
    }
}
