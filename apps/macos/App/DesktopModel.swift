import AppKit
import CSpielOrders
import Observation
import SpielOrdersKit

@MainActor @Observable final class DesktopModel {
    var destination = Destination.orders
    var ordersRevision = 0
    var orders: [OrderRow] = [] {
        didSet {
            ordersRevision &+= 1
            let listed = multipleSelection.filter { id in orders.contains { $0.id == id } }
            multipleSelection = listed.count > 1 ? listed : []
        }
    }
    var orderCount = 0
    var selectedOrderID: String?
    /// The Orders selected together, in the order the owner selected them; empty unless more
    /// than one is. While it holds any, no single Order is selected or shown.
    var multipleSelection: [String] = []
    var orderContext: OrderRow?
    var inspection: OrderInspection?
    /// Whether the destination sidebar shows while a Package is open.
    var sidebarVisible = UserDefaults.standard.object(forKey: "sidebarVisible") as? Bool ?? true {
        didSet { UserDefaults.standard.set(sidebarVisible, forKey: "sidebarVisible") }
    }
    var orderSortField: Int32 = OrderColumn(rawValue: UserDefaults.standard.string(forKey: "orderSortColumn") ?? "")?.field ?? -1 {
        didSet {
            let column = OrderColumn.allCases.first { $0.field == orderSortField }
            UserDefaults.standard.set(column?.rawValue, forKey: "orderSortColumn")
        }
    }
    var orderSortDescending = UserDefaults.standard.bool(forKey: "orderSortDescending") {
        didSet { UserDefaults.standard.set(orderSortDescending, forKey: "orderSortDescending") }
    }
    var vendors: [VendorRow] = [] {
        didSet {
            let listed = multipleVendorSelection.filter { id in vendors.contains { $0.id == id } }
            multipleVendorSelection = listed.count > 1 ? listed : []
        }
    }
    /// How many Vendors the list holds before search, hidden ones included.
    var vendorCount = 0
    var selectedVendorID: String?
    /// The Vendors selected together, in the order the owner selected them; empty unless more
    /// than one is. While it holds any, no single Vendor is selected or shown.
    var multipleVendorSelection: [String] = []
    var vendorContext: VendorRow?
    var vendorDetails: VendorDetails?
    var vendorSortField = SPIELORDERS_VENDOR_FIELD_NAME
    var vendorSortDescending = false
    var canBrowseRecords = false
    var diagnosticWorkspace = false
    var diagnosticDirectory: URL?
    var diagnosis: PackageDiagnosis?
    var recoveryEditor: RecoveryEditor?
    var expertDocument: ExpertDocument?
    var rawText = ""
    var rawOriginal = ""
    var rawDirty: Bool { expertDocument != nil && rawText != rawOriginal }
    /// The README the window shows, the Package's or the selected Order's.
    var readme: ReadmeSession?
    var root: URL?
    /// Package and listing warnings, by the sidebar destination they concern.
    var warnings: [Destination: [Notice]] = [:]
    /// Why records cannot be edited until the Package is reloaded.
    var reloadReason: Notice?
    var starting = true
    var activity = "Working…"
    private var documentPending = false
    let dialogs = WindowDialogs()
    private var operationBusy = false
    var navigationPending = false
    private var discardingForNavigation = false
    var busy: Bool {
        get { operationBusy || navigationPending || dialogs.isPresenting }
        set { operationBusy = newValue }
    }
    var search = RecordQuery()
    var refreshingLookup = false
    let contents = FolderContents()
    var orderEditor: RecordEditor?
    var vendorEditor: RecordEditor?
    var selectionRead: SelectionRead?
    var vendorSheet: RecordEditor? {
        didSet { if vendorSheet == nil { dialogs.dismissVendor() } }
    }
    var vendorChoices: [String] = []
    /// All Vendor names, including hidden ones, which must not be offered as new Vendors.
    var knownVendors: [String] = []
    /// The current values of each reusable-value fact, offered as suggestions and listed by
    /// the value destinations.
    var reusableValues: [Int32: [ReusableValue]] = [:]
    /// The Package's Projects with their Order Groups, listed by the Projects destination.
    var projects: [ProjectGroups] = []
    /// The rows selected in the Orderers, Institutions, or Projects table.
    var valueSelection: Set<ValueItem> = []
    /// The value a table row names in place: a new one, or one being renamed.
    var naming: ValueItem?
    /// Name input belongs to this Package even after the field that held it disappears.
    var valueNames: [ValueNameEdit] = []
    var library = PackageSession() {
        // Work asked for in one Package never applies to the next.
        didSet {
            waiting.clear()
            valueNames = []
            valueSelection = []
            naming = nil
        }
    }
    private let preferences = UserDefaults.standard
    var folder: AuthorizedFolder?
    let recents = RecentPackages()
    /// The window's one Undo history; see `RecordChange`.
    let undoManager = UndoManager()
    var work: OwnedOperation?
    var committing = false
    var continuingSave = false
    var afterCommit: (() -> Void)?
    var continueAfterFailure = false
    /// Ends of fields and names that other work held, applied once the window is free.
    @ObservationIgnored lazy var waiting = WaitingWork { [weak self] in self?.busy ?? false }
    private var lookupRevision = 0
    var dirty: Bool {
        readme?.draft.unsaved == true || rawDirty || recoveryEditor?.dirty == true
            || [orderEditor, vendorEditor, vendorSheet].contains { $0?.dirty == true }
            || valueNames.contains { $0.draft.dirty }
    }
    /// File → Save writes only the expert document editor's raw JSON.
    var canSave: Bool { !busy && rawDirty }

    func restore() {
        guard let bookmark = preferences.data(forKey: "packageBookmark") else { starting = false; return }
        do { open(try AuthorizedFolder.restore(bookmark), preserveSelection: false) }
        catch {
            starting = false
            let values = URL.resourceValues(forKeys: [.pathKey, .nameKey], fromBookmarkData: bookmark)
            let name = (values?.path ?? values?.name).map { URL(fileURLWithPath: $0).packageName }
            let target = name.map { "“\($0)”" } ?? "The Package"
            let failure = error as NSError
            if failure.domain == NSCocoaErrorDomain,
               [NSFileNoSuchFileError, NSFileReadNoSuchFileError].contains(failure.code) {
                alert("\(target) couldn’t be found",
                      "The Package folder could not be found. If it was moved or is on a disconnected drive, "
                        + "make it available and choose File → Open Package…",
                      identifier: "package.access")
            } else {
                alert("\(target) couldn’t be reopened", "Open it again with File → Open Package…",
                      details: error.localizedDescription, identifier: "package.access")
            }
        }
    }

    func choosePackage() {
        if deferNavigation({ [self] in choosePackage() }) { return }
        guard !busy, confirmDiscard(retry: { [self] in choosePackage() }) else { return }
        let picker = NSOpenPanel()
        picker.canChooseDirectories = true
        picker.canChooseFiles = false
        picker.treatsFilePackagesAsDirectories = true
        picker.allowsMultipleSelection = false
        picker.prompt = "Open Package"
        picker.message = "Choose a SpielOrder Package."
        picker.setAccessibilityLabel("Open Package")
        picker.setAccessibilityIdentifier("package.open.dialog")
        activity = "Opening Package…"
        busy = true
        Task {
            let response = await dialogs.chooseFolder(picker)
            busy = false
            guard response == .OK, let url = picker.url else { return }
            do { open(try AuthorizedFolder(url), preserveSelection: false) }
            catch { refuseAccess(error) }
        }
    }

    /// Opens a recently opened Package through the same guarded load as Open Package. One that
    /// is missing or no longer accessible says so and can be removed from the list.
    func openRecent(_ recent: RecentPackage) {
        if deferNavigation({ [self] in openRecent(recent) }) { return }
        guard !busy, confirmDiscard(retry: { [self] in openRecent(recent) }) else { return }
        do {
            let selected = try AuthorizedFolder.restore(recent.bookmark)
            guard try selected.url.checkResourceIsReachable() else { throw CocoaError(.fileNoSuchFile) }
            open(selected, preserveSelection: false)
        } catch {
            offerRemoval(of: recent)
        }
    }

    private func offerRemoval(of recent: RecentPackage) {
        busy = true
        Task {
            let remove = await confirm("“\(recent.name)” can’t be opened",
                "It may have been moved, renamed, or deleted, or SpielOrders can no longer access it. "
                    + "Remove it from the recent Packages?",
                identifier: "package.recent.missing", approve: "Remove from List")
            busy = false
            if remove { recents.remove(recent) }
        }
    }

    /// Finder supplies a folder grant just as the open panel does. Retain it before
    /// waiting for existing work, then use the same guarded Package load.
    func openDocument(_ urls: [URL]) {
        guard urls.count == 1, let url = urls.first, url.isFileURL else {
            documentRefusal("Open one Package at a time.")
            return
        }
        guard !navigationPending, !dialogs.isPresenting else {
            documentRefusal("Finish the current dialog or navigation, then open the Package again.")
            dialogs.focus()
            return
        }
        do {
            let selected = try AuthorizedFolder(url)
            documentPending = true
            navigationPending = true
            Task {
                if starting { await waitForWork() }
                else { await settle() }
                navigationPending = false
                documentPending = false
                openDocument(selected)
            }
        } catch {
            documentRefusal("Folder access couldn’t be authorized. Use File → Open Package to choose it again.")
        }
    }

    private func openDocument(_ selected: AuthorizedFolder) {
        guard !busy else {
            starting = false
            documentRefusal("Another operation started. Finish it, then open the Package again.")
            return
        }
        guard confirmDiscard(retry: { [self] in openDocument(selected) }) else { return }
        open(selected, preserveSelection: false)
    }

    private func documentRefusal(_ detail: String) {
        if work == nil { starting = false }
        Task { await dialogs.notify("Package not opened", detail, identifier: "package.openRefusal") }
    }

    func reloadPackage() {
        if deferNavigation({ [self] in reloadPackage() }) { return }
        guard !busy, let folder else { return }
        guard dirty else { open(folder, preserveSelection: true); return }
        busy = true
        Task {
            let reload = await confirm("Reload Package?", "Unsaved changes will be discarded. Saved changes are kept.",
                identifier: "package.reopen", approve: "Reload", destructive: true)
            busy = false
            if reload { open(folder, preserveSelection: true) }
        }
    }

    func navigate(to destination: Destination) {
        if destination == self.destination, !diagnosticWorkspace { return }
        if deferNavigation({ [self] in navigate(to: destination) }) { return }
        guard !busy, canBrowseRecords,
              (destination != self.destination || diagnosticWorkspace),
              confirmDiscard(retry: { [self] in navigate(to: destination) }) else { return }
        if destination != .orders { restartFolderContents() }
        if destination.searchable, !search.scope.isOffered(in: destination) { search.scope = .allFields }
        if destination == .packageReadme {
            openPackageReadme()
            return
        }
        discardEditors()
        valueSelection = []
        naming = nil
        self.destination = destination
        refreshLookup()
    }

    /// Shows an Order's directory in Finder once the library still finds it: selected in its
    /// parent, or opened to show its contents.
    func revealOrder(_ id: String?, contents: Bool = false) {
        if deferNavigation({ [self] in revealOrder(id, contents: contents) }) { return }
        guard !busy, let retainedFolder = folder, let row = orderRow(id) else { return }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                _ = try await withTaskCancellationHandler {
                    try await library.inspectOrder(row, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                reveal(row.directory, contents: contents)
            } catch {
                if !Task.isCancelled {
                    alert("The Order couldn’t be shown in Finder", "Reload the Package, then try again.",
                          details: error.localizedDescription, identifier: "order.reveal")
                }
            }
        }
    }

    func changeOrderSort(field: Int32, descending: Bool) -> Bool {
        guard !busy, canBrowseRecords, let retainedFolder = folder else { return false }
        guard field != orderSortField || descending != orderSortDescending else { return true }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let listed = try await withTaskCancellationHandler {
                    try await searchOrders(library, search, sortField: field, descending: descending,
                                           cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                let row = listed.retainingSelection(orderContext ?? orders.first { $0.id == selectedOrderID })
                    ?? (!search.text.isEmpty || orderEditor != nil ? orderContext : nil)
                orderSortField = field
                orderSortDescending = descending
                orders = listed.rows
                await showListing(listed.notices, of: .orders, session: library)
                selectedOrderID = row?.id
                if orderEditor == nil { inspection = nil }
                if let row, orderEditor == nil {
                    do {
                        inspection = try await withTaskCancellationHandler {
                            try await library.inspectOrder(row, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedOrderID = nil
                        warn(.orders, "The selected Order couldn’t be read. Reload the Package, then try again.", error: error)
                    }
                }
            } catch {
                if !Task.isCancelled { warn(.orders, "Couldn’t sort Orders.", error: error) }
            }
        }
        return true
    }

    func changeVendorSort(field: Int32, descending: Bool) -> Bool {
        guard !busy, canBrowseRecords, let retainedFolder = folder else { return false }
        guard field != vendorSortField || descending != vendorSortDescending else { return true }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let listed = try await withTaskCancellationHandler {
                    try await searchVendors(library, search, sortField: field,
                                            descending: descending, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                let total = search.text.isEmpty ? listed.rows.count
                    : try await countVendors(library, cancellation: cancellation)
                try Task.checkCancellation()
                vendorCount = total
                vendorSortField = field
                vendorSortDescending = descending
                vendors = listed.rows
                await showListing(listed.notices, of: .vendors, session: library)
                let row = vendors.first { $0.id == selectedVendorID }
                if vendorEditor == nil {
                    selectedVendorID = row?.id
                    vendorDetails = nil
                }
                if let row, vendorEditor == nil {
                    do {
                        vendorDetails = try await withTaskCancellationHandler {
                            try await library.inspectVendor(row, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedVendorID = nil
                        warn(.vendors, "The selected Vendor couldn’t be read. Reload the Package, then try again.", error: error)
                    }
                }
            } catch {
                if !Task.isCancelled { warn(.vendors, "Couldn’t list Vendors.", error: error) }
            }
        }
        return true
    }

    func chooseInitialization() {
        if deferNavigation({ [self] in chooseInitialization() }) { return }
        guard !busy, confirmDiscard(retry: { [self] in chooseInitialization() }) else { return }
        let picker = NSOpenPanel()
        picker.canChooseDirectories = true
        picker.canChooseFiles = false
        picker.treatsFilePackagesAsDirectories = true
        picker.canCreateDirectories = true
        picker.allowsMultipleSelection = false
        picker.prompt = "Choose Package Folder"
        picker.setAccessibilityLabel("Choose Package Folder")
        picker.setAccessibilityIdentifier("package.initialize.dialog")
        activity = "Preparing Package…"
        busy = true
        Task {
            guard await dialogs.chooseFolder(picker) == .OK, let url = picker.url else {
                busy = false
                return
            }
            await dialogs.waitForDismissal(of: picker)
            guard let options = await dialogs.configurePackage(url) else {
                busy = false
                return
            }
            busy = false
            do {
                initialize(try AuthorizedFolder(url), ordersPath: options.orders, removedPath: options.removed)
            } catch { refuseAccess(error) }
        }
    }

    private func initialize(_ selected: AuthorizedFolder, ordersPath: String, removedPath: String) {
        activity = "Creating Package…"
        busy = true
        let cancellation = LibraryCancellation()
        let candidate = PackageSession()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false }
            do {
                let initialized = try await withTaskCancellationHandler {
                    try await candidate.initialize(selected.url, ordersPath: ordersPath, removedPath: removedPath,
                                                   cancellation: cancellation)
                } onCancel: { cancellation.request() }
                folder = selected
                library = candidate
                root = selected.url
                reloadReason = nil
                undoManager.removeAllActions()
                discardEditors()
                contents.stop()
                let findings = Notice.list(initialized.notices) + Notice.list(await candidate.findings)
                warnings = [.orders: findings]
                orders = []
                orderCount = 0
                search = RecordQuery()
                selectedOrderID = nil
                multipleSelection = []
                orderContext = nil
                inspection = nil
                vendors = []
                vendorCount = 0
                selectedVendorID = nil
                multipleVendorSelection = []
                vendorContext = nil
                vendorDetails = nil
                destination = .orders
                canBrowseRecords = initialized.canBrowseRecords
                if canBrowseRecords {
                    do {
                        let listed = try await candidate.orders(query: "", caseSensitive: false, sortField: orderSortField,
                                                                descending: orderSortDescending, cancellation: cancellation)
                        orders = listed.rows
                        orderCount = orders.count
                        let listedVendors = try await candidate.vendors(query: "", caseSensitive: false,
                                                                        includeHidden: true,
                                                                        sortField: vendorSortField,
                                                                        descending: vendorSortDescending,
                                                                        cancellation: cancellation)
                        vendors = listedVendors.rows
                        vendorCount = vendors.count
                        showWarnings(orders: findings + Notice.list(listed.notices), vendors: Notice.list(listedVendors.notices))
                    } catch {
                        requireReload(Notice("The Package was created, but its records couldn’t be listed.",
                                             details: error.localizedDescription))
                    }
                } else {
                    requireReload(Notice("The Package was created, but it couldn’t be read back.",
                                         details: initialized.message))
                }
                remember(selected)
            } catch {
                alert("Couldn’t create the Package", details: error.localizedDescription, identifier: "package.create")
            }
        }
    }

    private func remember(_ selected: AuthorizedFolder) {
        do {
            let bookmark = try selected.bookmark()
            preferences.set(bookmark, forKey: "packageBookmark")
            recents.note(selected.url, bookmark: bookmark)
        } catch {
            preferences.removeObject(forKey: "packageBookmark")
            alert("SpielOrders can’t reopen this Package by itself",
                  "It stays open now. Next time, open it with File → Open Package.",
                  details: error.localizedDescription, identifier: "package.remember")
        }
    }

    /// A chosen folder the sandbox would not grant access to.
    private func refuseAccess(_ error: Error) {
        alert("Folder access couldn’t be authorized", "Choose the folder again.",
              details: error.localizedDescription, identifier: "package.access")
    }

    private func open(_ selected: AuthorizedFolder, preserveSelection: Bool) {
        guard !busy else { return }
        activity = "Opening Package…"
        busy = true
        let cancellation = LibraryCancellation()
        let candidate = PackageSession()
        let priorSelection = preserveSelection ? selectedOrderID : nil
        let priorVendorSelection = preserveSelection ? selectedVendorID : nil
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; if !documentPending { starting = false } }
            do {
                _ = try await withTaskCancellationHandler {
                    try await candidate.open(selected.url, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                var rows: [OrderRow] = []
                var vendorRows: [VendorRow] = []
                var orderNotices = await candidate.findings
                var vendorNotices: [Problem] = []
                let diagnosticOnly = await candidate.diagnosticOnly
                if !diagnosticOnly {
                    let listed = try await withTaskCancellationHandler {
                        try await candidate.orders(query: "", caseSensitive: false, sortField: orderSortField, descending: orderSortDescending,
                                                   cancellation: cancellation)
                    } onCancel: { cancellation.request() }
                    rows = listed.rows
                    orderNotices += listed.notices
                    let listedVendors = try await withTaskCancellationHandler {
                        try await candidate.vendors(query: "", caseSensitive: false, includeHidden: true, sortField: vendorSortField,
                                                     descending: vendorSortDescending, cancellation: cancellation)
                    } onCancel: { cancellation.request() }
                    vendorRows = listedVendors.rows
                    vendorNotices = listedVendors.notices
                }
                // Reload keeps the Package README shown, read again.
                let packageReadme = destination == .packageReadme
                    ? try await candidate.readme(of: nil, cancellation: cancellation) : nil
                try Task.checkCancellation()
                folder = selected
                library = candidate
                root = selected.url
                reloadReason = nil
                if preserveSelection {
                    forgetEdits()
                } else {
                    undoManager.removeAllActions()
                }
                discardEditors()
                if let packageReadme { showReadme(packageReadme, of: nil) }
                canBrowseRecords = !diagnosticOnly
                contents.stop()
                orders = rows
                orderCount = rows.count
                search = RecordQuery()
                selectedOrderID = rows.first(where: { $0.id == priorSelection && $0.damage == nil })?.id
                multipleSelection = []
                orderContext = rows.first { $0.id == selectedOrderID }
                inspection = nil
                vendors = vendorRows
                vendorCount = vendorRows.count
                selectedVendorID = vendorRows.first { $0.id == priorVendorSelection }?.id
                multipleVendorSelection = []
                vendorContext = vendorRows.first { $0.id == selectedVendorID }
                vendorDetails = nil
                warnings = [:]
                showWarnings(orders: Notice.list(orderNotices), vendors: Notice.list(vendorNotices))
                if diagnosticOnly {
                    diagnosis = try await candidate.diagnose(directory: nil, cancellation: cancellation)
                    diagnosticDirectory = nil
                    diagnosticWorkspace = true
                }
                remember(selected)
                if let selectedRow = rows.first(where: { $0.id == selectedOrderID }) {
                    do {
                        inspection = try await withTaskCancellationHandler {
                            try await candidate.inspectOrder(selectedRow, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedOrderID = nil
                        warn(.orders, "The selected Order couldn’t be read. Reload the Package, then try again.", error: error)
                    }
                }
                if let selectedVendor = vendorRows.first(where: { $0.id == selectedVendorID }) {
                    do {
                        vendorDetails = try await withTaskCancellationHandler {
                            try await candidate.inspectVendor(selectedVendor, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedVendorID = nil
                        warn(.vendors, "The selected Vendor couldn’t be read. Reload the Package, then try again.", error: error)
                    }
                }
                await openSelection(candidate, cancellation: cancellation)
                if !diagnosticOnly {
                    await listShownValues(candidate)
                }
            } catch {
                if !Task.isCancelled {
                    alert("The Package couldn’t be opened", details: error.localizedDescription, identifier: "package.open")
                }
            }
        }
    }

    /// Lists what the shown destination presents when it lists values rather than records.
    private func listShownValues(_ session: PackageSession) async {
        do {
            switch destination {
            case let .values(list):
                reusableValues[list.field] = try await session.values(list.field)
            case .projects:
                projects = try await session.projectGroups()
            case .orders, .vendors, .packageReadme:
                return
            }
        } catch {
            warn(destination, "Couldn’t list \(destination.title).", error: error)
        }
    }

    func reveal() {
        guard let root else { return }
        reveal(root)
    }

    func reveal(_ location: URL, contents: Bool = false) {
        do {
            guard try location.checkResourceIsReachable() else { throw CocoaError(.fileNoSuchFile) }
            if contents {
                NSWorkspace.shared.open(location)
            } else {
                NSWorkspace.shared.activateFileViewerSelecting([location])
            }
        } catch {
            alert("“\(location.lastPathComponent)” couldn’t be shown in Finder",
                  "It may have been moved or deleted. Reload the Package, then try again.",
                  details: error.localizedDescription, identifier: "finder.reveal")
        }
    }

    func updateQuery(_ value: String) {
        guard value != search.text else { return }
        search.text = value
        restartFolderContents()
        refreshLookup()
    }

    func setScope(_ scope: SearchScope) {
        guard scope != search.scope else { return }
        search.scope = scope
        restartFolderContents()
        refreshLookup()
    }

    func setMatchCase(_ value: Bool) {
        guard value != search.matchCase else { return }
        search.matchCase = value
        refreshLookup()
    }

    /// Stops Spotlight for a query or scope that no longer applies; the next listing asks again.
    private func restartFolderContents() {
        contents.stop()
    }

    func refreshLookup() {
        guard canBrowseRecords, destination != .packageReadme, let retainedFolder = folder else { return }
        lookupRevision &+= 1
        // A running lookup sees the new revision and lists again.
        guard !refreshingLookup, !busy else { return }
        busy = true
        refreshingLookup = true
        work = OwnedOperation(kind: .search) { [self] in
            defer {
                busy = false
                refreshingLookup = false
                withExtendedLifetime(retainedFolder) {}
            }
            while !Task.isCancelled {
                let revision = lookupRevision
                let requested = search
                let cancellation = LibraryCancellation()
                do {
                    if case let .values(list) = destination {
                        let values = try await library.values(list.field)
                        guard !Task.isCancelled else { return }
                        if revision != lookupRevision { continue }
                        reusableValues[list.field] = values
                        warnings[destination] = nil
                    } else if destination == .projects {
                        let listed = try await library.projectGroups()
                        guard !Task.isCancelled else { return }
                        if revision != lookupRevision { continue }
                        projects = listed
                        warnings[.projects] = nil
                    } else if destination == .orders {
                        let listed = try await withTaskCancellationHandler {
                            try await searchOrders(library, requested, sortField: orderSortField,
                                                   descending: orderSortDescending, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                        guard !Task.isCancelled else { return }
                        if revision != lookupRevision { continue }
                        let selected = listed.retainingSelection(orderContext ?? orders.first { $0.id == selectedOrderID })
                        orders = listed.rows
                        if requested.text.isEmpty { orderCount = orders.count }
                        if requested.text.isEmpty, orderEditor == nil {
                            selectedOrderID = selected?.id
                            if selected == nil { inspection = nil; orderContext = nil }
                        }
                        await showListing(listed.notices, of: .orders, session: library)
                    } else {
                        let listed = try await withTaskCancellationHandler {
                            try await searchVendors(library, requested,
                                                    sortField: vendorSortField, descending: vendorSortDescending,
                                                    cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                        guard !Task.isCancelled else { return }
                        if revision != lookupRevision { continue }
                        vendors = listed.rows
                        if requested.text.isEmpty { vendorCount = vendors.count }
                        if requested.text.isEmpty, vendorEditor == nil, vendorContext?.hidden != true,
                           !vendors.contains(where: { $0.id == selectedVendorID }) {
                            selectedVendorID = nil; vendorContext = nil; vendorDetails = nil
                        }
                        await showListing(listed.notices, of: .vendors, session: library)
                    }
                    // Navigation leaves the destination's selection without its editor.
                    await openSelection(library, cancellation: cancellation)
                    if revision != lookupRevision { continue }
                } catch {
                    guard !Task.isCancelled else { return }
                    if revision != lookupRevision { continue }
                    warn(destination, "Couldn’t list \(destination.title).", error: error)
                }
                return
            }
        }
    }

    /// The Orders `query` selects. Folder Contents joins the Supporting Material Spotlight
    /// found for its text, and asks Spotlight when it has not answered that text yet.
    func searchOrders(_ session: PackageSession, _ query: RecordQuery, sortField: Int32, descending: Bool,
                      cancellation: LibraryCancellation) async throws -> OrderListing {
        guard query.scope == .folderContents else {
            return try await session.orders(query: query.text, field: query.scope.field, caseSensitive: query.matchCase,
                                            sortField: sortField, descending: descending, cancellation: cancellation)
        }
        let material = contents.answer(for: query.text)
        if material == nil, !query.text.isEmpty { gatherMaterial(query.text, session: session) }
        return try await session.folderContents(query: query.text, caseSensitive: query.matchCase, material: material,
                                                sortField: sortField, descending: descending,
                                                cancellation: cancellation)
    }

    /// The Vendors table's rows: every Vendor matching `query`, hidden ones marked rather than left out.
    func searchVendors(_ session: PackageSession, _ query: RecordQuery, sortField: Int32,
                       descending: Bool, cancellation: LibraryCancellation) async throws -> VendorListing {
        try await session.vendors(query: query.text, field: query.scope.field, caseSensitive: query.matchCase,
                                  includeHidden: true, sortField: sortField, descending: descending,
                                  cancellation: cancellation)
    }

    private func gatherMaterial(_ text: String, session: PackageSession) {
        guard let folder else { return }
        contents.gather(text, session: session, folder: folder) { [weak self] in
            self?.materialArrived()
        }
    }

    /// Spotlight's answer refines the grid once the window's current work settles.
    private func materialArrived() {
        Task {
            await waitForWork()
            refreshLookup()
        }
    }

    func openFile(_ location: URL) {
        do {
            guard try location.checkResourceIsReachable() else { throw CocoaError(.fileNoSuchFile) }
            guard NSWorkspace.shared.open(location) else { throw CocoaError(.fileReadUnknown) }
        } catch {
            alert("“\(location.lastPathComponent)” couldn’t be opened",
                  "It may have been moved or deleted. Search again, or show it in Finder.",
                  details: error.localizedDescription, identifier: "finder.open")
        }
    }

    var currentOrder: OrderRow? {
        if diagnosticWorkspace {
            guard let diagnosticDirectory else { return nil }
            return orders.first { $0.directory == diagnosticDirectory }
        }
        guard destination == .orders else { return nil }
        let id: String?
        if let orderEditor { id = orderEditor.draft.recordID }
        else { id = selectedOrderID }
        return orders.first { $0.id == id } ?? (orderContext?.id == id ? orderContext : nil)
    }

    var canChangeDirectory: Bool {
        canBrowseRecords && !busy && !dirty && destination == .orders && currentOrder?.damage == nil && currentOrder != nil
    }

    func changeOrderDirectory(_ action: DirectoryAction, id: String? = nil) {
        guard !busy, !dirty else { return }
        let target: OrderRow?
        if let id { target = orders.first { $0.id == id } }
        else { target = currentOrder }
        guard let row = target, row.damage == nil, let retainedFolder = folder else { return }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let plan = try await withTaskCancellationHandler {
                    try await library.planDirectoryChange(row, action: action, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                // Remove Order is recoverable, so it commits at once and Edit → Undo restores it.
                if action != .remove {
                    let explanation = action == .trash
                        ? "The Order goes to the Trash. If that fails, it stays recoverable in Removed Orders."
                        : "Only the directory name changes; the Order’s facts and files are retained."
                    guard await dialogs.review(action.title + "?", explanation: explanation,
                        details: row.directoryName + "\n\n" + plan.operations.joined(separator: "\n"),
                        identifier: "order.directory.confirmation", approve: action.title,
                        destructive: action == .trash) else { return }
                }
                let disposition = try await library.commit(plan)
                discardEditors()
                contents.stop()
                // A Trash move that failed leaves the Order recoverable where removal put it.
                if action == .trash, let removed = disposition.finalDirectory {
                    alert("The Order couldn’t be moved to the Trash",
                          "It was removed, and stays recoverable in the Removed Orders folder.",
                          details: disposition.message, identifier: "order.trash", location: removed)
                }
                if action == .remove, let directory = disposition.finalDirectory {
                    offerRestore(RemovedOrder(id: row.id, directory: directory))
                } else {
                    endRedo()
                }
                if disposition.requiresReopen {
                    requireReload(Notice(disposition.summary, details: disposition.message))
                    clearOrders()
                } else {
                    await refreshRecords(selectingOrder: action == .align ? row.id : nil, vendor: selectedVendorID)
                    addWarnings(disposition.notices, to: .orders)
                }
            } catch {
                alert("Couldn’t \(action.title)", details: error.localizedDescription, identifier: "order.directory")
            }
        }
    }
    /// Navigation returns now; only an approved decision retries the caller.
    /// The retry bypass is synchronous so it cannot authorize unrelated navigation.
    func confirmDiscard(retry: @escaping @MainActor () -> Void) -> Bool {
        if deferNavigation(retry) { return false }
        if discardingForNavigation { return true }
        guard !navigationPending, !dialogs.isPresenting else { return false }
        guard dirty else { return true }
        navigationPending = true
        Task {
            let approved = await resolveDrafts()
            navigationPending = false
            guard approved else { return }
            discardingForNavigation = true
            defer { discardingForNavigation = false }
            retry()
        }
        return false
    }

    /// A click that ended a field waits for that field's save instead of being swallowed. One
    /// action waits at a time: a later navigation replaces a waiting one, as the owner's latest
    /// choice, but a change to a record's rows (`afterFailure`) is never replaced and never
    /// replaces one; the later action beeps instead.
    func deferNavigation(afterFailure: Bool = false, _ action: @escaping () -> Void) -> Bool {
        guard committing else { return false }
        guard afterCommit == nil || !afterFailure && !continueAfterFailure else {
            NSSound.beep()
            return true
        }
        afterCommit = action
        continueAfterFailure = afterFailure
        return true
    }

    /// The delegate reserves navigation while Close or Quit resolves owned work and drafts.
    func confirmExit() async -> Bool {
        dialogs.beginExit()
        dialogs.isTerminating = true
        await settle()
        dialogs.isTerminating = false
        return await resolveDrafts()
    }

    /// Remember the welcome view only once Quit is approved, including after a failed restore.
    func willQuit() {
        if root == nil { preferences.removeObject(forKey: "packageBookmark") }
    }

    /// Release the settled active session and its active restoration bookmark.
    func closePackage() {
        undoManager.removeAllActions()
        discardEditors()
        library = PackageSession()
        folder = nil
        root = nil
        starting = false
        work = nil
        destination = .orders
        orders = []
        orderCount = 0
        vendors = []
        vendorCount = 0
        selectedOrderID = nil
        multipleSelection = []
        selectedVendorID = nil
        multipleVendorSelection = []
        orderContext = nil
        vendorContext = nil
        inspection = nil
        vendorDetails = nil
        vendorChoices = []
        knownVendors = []
        reusableValues = [:]
        projects = []
        valueSelection = []
        naming = nil
        canBrowseRecords = false
        warnings = [:]
        reloadReason = nil
        diagnosticDirectory = nil
        search = RecordQuery()
        contents.stop()
        refreshingLookup = false
        afterCommit = nil
        continueAfterFailure = false
        preferences.removeObject(forKey: "packageBookmark")
    }

    /// Resolve the nested Vendor first, then its parent. Approval does not destroy an
    /// outer draft: a navigation that subsequently fails or is cancelled still shows it.
    private func resolveDrafts() async -> Bool {
        while dirty {
            if let name = valueNames.first(where: { $0.draft.dirty }) {
                let reserved = navigationPending
                navigationPending = false
                finishName(name.draft)
                let saving = operationBusy ? work : nil
                navigationPending = reserved
                guard let saving else { return false }
                await saving.settleForExit()
                guard !name.draft.dirty, !name.draft.refused else { return false }
                continue
            }
            if readme?.draft.unsaved == true {
                guard await settleReadme() else { return false }
                continue
            }
            if let sheet = vendorSheet, !sheet.dirty {
                let vendorWindow = dialogs.window?.attachedSheet
                sheet.close()
                vendorSheet = nil
                await dialogs.waitForDismissal(of: vendorWindow)
                continue
            }
            let sheet = vendorSheet?.dirty == true
            let vendorWindow = sheet ? dialogs.window?.attachedSheet : nil
            let editor = sheet ? vendorSheet : recordEditor
            if let editor, !editor.isNew {
                navigationPending = false
                commitRecord(editor)
                navigationPending = true
                await settle()
                guard !editor.dirty, !editor.refused else { return false }
                continue
            }
            if let editor, editor.isNew {
                let discard = await confirm("Discard this new \(editor.kind == .order ? "Order" : "Vendor")?",
                    "It hasn’t been created yet. Choose Create to keep it.",
                    identifier: "draft.unsaved", approve: "Discard", destructive: true)
                guard discard else { return false }
                if sheet {
                    vendorSheet?.close()
                    vendorSheet = nil
                    await dialogs.waitForDismissal(of: vendorWindow)
                    continue
                }
                return true
            }
            let wasRaw = rawDirty
            let repair = recoveryEditor
            let draft = expertDocument != nil ? "raw JSON document" : recoveryEditor != nil ? "repair"
                : sheet || recordEditor?.kind == .vendor ? "Vendor" : "Order"
            let prefix = "draft.unsaved"
            let alert = NSAlert()
            alert.messageText = "Do you want to save the changes to this \(draft)?"
            alert.informativeText = "Your changes will be lost if you don’t save them."
            alert.addButton(withTitle: "Save")
            alert.addButton(withTitle: "Don’t Save")
            alert.addButton(withTitle: "Cancel")
            alert.buttons[1].hasDestructiveAction = true
            alert.buttons[1].keyEquivalent = "d"
            alert.buttons[1].keyEquivalentModifierMask = .command
            alert.window.setAccessibilityIdentifier(prefix + ".dialog")
            for (button, name) in zip(alert.buttons, ["save", "discard", "cancel"]) {
                button.setAccessibilityIdentifier(prefix + "." + name)
            }
            let response = await dialogs.present(alert)
            switch response {
            case .alertFirstButtonReturn:
                // Save's entry guards must see idle state. Restore the reservation
                // before suspending, so it protects the whole asynchronous save.
                navigationPending = false
                if sheet {
                    saveVendorSheet()
                } else if expertDocument != nil {
                    saveExpertDocument()
                } else {
                    applyRepair()
                }
                let saving = operationBusy ? work : nil
                navigationPending = true
                guard let saving else { return false }
                await saving.settleForExit()
                if wasRaw && rawDirty { return false }
                if let repair, recoveryEditor === repair { return false }
                if let editor, (recordEditor === editor || vendorSheet === editor), editor.dirty { return false }
                if sheet {
                    await dialogs.waitForDismissal(of: vendorWindow)
                    continue
                }
                return true
            case .alertSecondButtonReturn:
                if sheet {
                    vendorSheet?.close()
                    vendorSheet = nil
                    await dialogs.waitForDismissal(of: vendorWindow)
                    continue
                }
                return true
            default:
                return false
            }
        }
        return true
    }

    func settle() async {
        contents.stop()
        while let current = work {
            await current.settleForExit()
            if work === current { break }
        }
        await contents.settle()
    }

    /// How many Vendors the list holds without search, for the count above it.
    func countVendors(_ session: PackageSession, cancellation: LibraryCancellation) async throws -> Int {
        let listed = try await withTaskCancellationHandler {
            try await session.vendors(query: "", caseSensitive: false, includeHidden: true,
                                      sortField: vendorSortField, descending: vendorSortDescending,
                                      cancellation: cancellation)
        } onCancel: { cancellation.request() }
        return listed.rows.count
    }

    func waitForWork() async {
        while let current = work {
            await current.wait()
            if work === current { return }
        }
    }
}
