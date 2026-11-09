import AppKit
import CSpielOrders
import Observation
import SpielOrdersKit

@MainActor @Observable final class DesktopModel {
    enum Destination: String, CaseIterable { case orders = "Orders", vendors = "Vendors", packageReadme = "Package README" }
    var destination = Destination.orders
    var ordersRevision = 0
    var orders: [OrderRow] = [] { didSet { ordersRevision &+= 1 } }
    var orderCount = 0
    var selectedOrderID: String?
    var inspection: OrderInspection?
    var inspectorVisible = UserDefaults.standard.object(forKey: "inspectorVisible") as? Bool ?? true {
        didSet { UserDefaults.standard.set(inspectorVisible, forKey: "inspectorVisible") }
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
    var vendors: [VendorRow] = []
    var selectedVendorID: String?
    var vendorDetails: VendorDetails?
    var includeHiddenVendors = UserDefaults.standard.object(forKey: "includeHiddenVendors") as? Bool ?? false {
        didSet { UserDefaults.standard.set(includeHiddenVendors, forKey: "includeHiddenVendors") }
    }
    var vendorSortField = SPIELORDERS_VENDOR_FIELD_NAME
    var vendorSortDescending = false
    private var canBrowseRecords = false
    var text = ""
    var original = ""
    var editingOrder: OrderRow?
    var root: URL?
    var message = ""
    var busy = false
    var writable = false
    var query = ""
    var caseSensitive = false
    var spotlightMode = false
    var refreshingLookup = false
    var searchAnswer: MaterialAnswer?
    var searching = false
    private var library = PackageSession()
    private let preferences = UserDefaults.standard
    private var folder: AuthorizedFolder?
    private var work: OwnedOperation?
    private var activeSearch: SpotlightSearch?
    private var lookupRevision = 0
    var dirty: Bool { text != original }
    var editingReadme: Bool { destination == .packageReadme || editingOrder != nil }

    func restore() {
        guard let bookmark = preferences.data(forKey: "packageBookmark") else { return }
        do { open(try AuthorizedFolder.restore(bookmark), preserveSelection: false) }
        catch { message = "Package access needs renewal. Open the Package again.\n" + error.localizedDescription }
    }

    func choosePackage() {
        guard !busy, confirmDiscard() else { return }
        let picker = NSOpenPanel()
        picker.canChooseDirectories = true
        picker.canChooseFiles = false
        picker.allowsMultipleSelection = false
        picker.prompt = "Open Package"
        picker.message = "Choose a SpielOrder Package."
        guard picker.runModal() == .OK, let url = picker.url else { return }
        do { open(try AuthorizedFolder(url), preserveSelection: false) }
        catch { message = error.localizedDescription }
    }

    func reopenPackage() {
        guard !busy, let folder, confirmDiscard() else { return }
        open(folder, preserveSelection: true)
    }

    func navigate(to destination: Destination) {
        guard !busy, (destination != self.destination || editingOrder != nil), confirmDiscard() else { return }
        if destination == .packageReadme {
            beginReadmeEdit(order: nil)
            return
        }
        text = original
        editingOrder = nil
        self.destination = destination
        if destination != .packageReadme { refreshLookup() }
    }

    func editOrderReadme(_ row: OrderRow) {
        guard !busy, row.damage == nil, confirmDiscard() else { return }
        beginReadmeEdit(order: row)
    }

    private func beginReadmeEdit(order: OrderRow?) {
        guard let retainedFolder = folder else { return }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let loaded = try await withTaskCancellationHandler {
                    if let order { return try await library.editOrderReadme(order, cancellation: cancellation) }
                    return try await library.editPackageReadme(cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                editingOrder = order
                destination = order == nil ? .packageReadme : .orders
                text = loaded
                original = loaded
                writable = await library.readmeWritable
                message = writable ? "" : "README is unreadable. Reopen the Package before editing."
            } catch { if !Task.isCancelled { message = "Couldn’t open the README.\n" + error.localizedDescription } }
        }
    }

    func backToOrders() {
        guard !busy, let order = editingOrder, confirmDiscard() else { return }
        text = original
        editingOrder = nil
        selectedOrderID = nil
        selectOrder(order.id, revealInFinder: false)
    }

    func selectOrder(_ id: String?, revealInFinder: Bool) {
        guard !busy, (id != selectedOrderID || revealInFinder), confirmDiscard() else { return }
        selectedOrderID = id
        inspection = nil
        message = ""
        guard let row = orders.first(where: { $0.id == id }), let retainedFolder = folder else { return }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let details = try await withTaskCancellationHandler {
                    try await library.inspectOrder(row, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                inspection = details
                if revealInFinder { reveal(row.directory) }
            } catch {
                selectedOrderID = nil
                message = "Couldn’t read this Order. Reopen the Package to refresh.\n" + error.localizedDescription
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
                    try await library.orders(query: query, caseSensitive: caseSensitive, sortField: field, descending: descending, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                let row = listed.retainingSelection(orders.first { $0.id == selectedOrderID })
                orderSortField = field
                orderSortDescending = descending
                orders = listed.rows
                message = listed.notices.joined(separator: "\n")
                selectedOrderID = row?.id
                inspection = nil
                if let row {
                    do {
                        inspection = try await withTaskCancellationHandler {
                            try await library.inspectOrder(row, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedOrderID = nil
                        message += "\nThe selection changed. Reopen the Package to refresh.\n" + error.localizedDescription
                    }
                }
            } catch { if !Task.isCancelled { message = "Couldn’t sort Orders.\n" + error.localizedDescription } }
        }
        return true
    }

    func selectVendor(_ id: String?) {
        guard !busy, id != selectedVendorID, confirmDiscard() else { return }
        selectedVendorID = id
        vendorDetails = nil
        message = ""
        guard let row = vendors.first(where: { $0.id == id }), let retainedFolder = folder else { return }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let details = try await withTaskCancellationHandler {
                    try await library.inspectVendor(row, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                vendorDetails = details
            } catch {
                selectedVendorID = nil
                message = "Couldn’t read this Vendor. Reopen the Package to refresh.\n" + error.localizedDescription
            }
        }
    }

    func changeVendorList(includeHidden: Bool, field: Int32, descending: Bool) -> Bool {
        guard !busy, canBrowseRecords, let retainedFolder = folder else { return false }
        guard includeHidden != includeHiddenVendors || field != vendorSortField || descending != vendorSortDescending else { return true }
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let listed = try await withTaskCancellationHandler {
                    try await library.vendors(query: query, caseSensitive: caseSensitive, includeHidden: includeHidden, sortField: field, descending: descending,
                                              cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                includeHiddenVendors = includeHidden
                vendorSortField = field
                vendorSortDescending = descending
                vendors = listed.rows
                message = listed.notices.joined(separator: "\n")
                let row = vendors.first { $0.id == selectedVendorID }
                selectedVendorID = row?.id
                vendorDetails = nil
                if let row {
                    do {
                        vendorDetails = try await withTaskCancellationHandler {
                            try await library.inspectVendor(row, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedVendorID = nil
                        message += "\nThe Vendor is no longer available.\n" + error.localizedDescription
                    }
                }
            } catch { if !Task.isCancelled { message = "Couldn’t list Vendors.\n" + error.localizedDescription } }
        }
        return true
    }

    func chooseInitialization() {
        guard !busy, confirmDiscard() else { return }
        let picker = NSOpenPanel()
        picker.canChooseDirectories = true
        picker.canChooseFiles = false
        picker.canCreateDirectories = true
        picker.allowsMultipleSelection = false
        picker.prompt = "Choose Package Folder"
        guard picker.runModal() == .OK, let url = picker.url else { return }

        let configuration = NSAlert()
        configuration.messageText = "Initialize Package?"
        configuration.informativeText = url.lastPathComponent + "\nExisting user content is preserved. Directory names are relative to this folder; leave blank for the conventional defaults."
        configuration.addButton(withTitle: "Initialize Package")
        configuration.addButton(withTitle: "Cancel")
        let orders = NSTextField(string: "")
        orders.placeholderString = "Default Orders directory"
        orders.setAccessibilityLabel("Orders directory")
        let removed = NSTextField(string: "")
        removed.placeholderString = "Default Removed Orders directory"
        removed.setAccessibilityLabel("Removed Orders directory")
        let fields = NSStackView(views: [NSTextField(labelWithString: "Orders directory"), orders,
                                       NSTextField(labelWithString: "Removed Orders directory"), removed])
        fields.orientation = .vertical
        fields.alignment = .leading
        fields.spacing = 6
        fields.frame = NSRect(x: 0, y: 0, width: 340, height: 104)
        orders.widthAnchor.constraint(equalToConstant: 340).isActive = true
        removed.widthAnchor.constraint(equalToConstant: 340).isActive = true
        configuration.accessoryView = fields
        configuration.window.initialFirstResponder = orders
        guard configuration.runModal() == .alertFirstButtonReturn else { return }
        do {
            initialize(try AuthorizedFolder(url), ordersPath: orders.stringValue, removedPath: removed.stringValue)
        } catch { message = error.localizedDescription }
    }

    private func initialize(_ selected: AuthorizedFolder, ordersPath: String, removedPath: String) {
        busy = true
        message = ""
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
                text = initialized.readmeText ?? ""
                original = text
                editingOrder = nil
                writable = await candidate.readmeWritable && initialized.readmeText != nil
                searchAnswer = nil
                message = initialized.message
                orders = []
                orderCount = 0
                query = ""
                spotlightMode = false
                selectedOrderID = nil
                inspection = nil
                vendors = []
                selectedVendorID = nil
                vendorDetails = nil
                destination = .orders
                let canBrowse = !(await candidate.diagnosticOnly)
                canBrowseRecords = initialized.readmeText != nil && canBrowse
                if initialized.readmeText != nil && canBrowse {
                    do {
                        let listed = try await candidate.orders(query: "", caseSensitive: false, sortField: orderSortField, descending: orderSortDescending,
                                                                cancellation: cancellation)
                        orders = listed.rows
                        orderCount = orders.count
                        vendors = try await candidate.vendors(query: "", caseSensitive: false, includeHidden: includeHiddenVendors, sortField: vendorSortField,
                                                              descending: vendorSortDescending, cancellation: cancellation).rows
                        if !listed.notices.isEmpty { message += "\n" + listed.notices.joined(separator: "\n") }
                    } catch {
                        message += "\nThe Package is initialized, but its records could not be loaded. Reopen before continuing.\n" + error.localizedDescription
                    }
                }
                remember(selected)
            } catch { message = "Couldn’t complete initialization.\n" + error.localizedDescription }
        }
    }

    private func remember(_ selected: AuthorizedFolder) {
        do { preferences.set(try selected.bookmark(), forKey: "packageBookmark") }
        catch { message += "\nAccess cannot be remembered: " + error.localizedDescription }
    }

    private func open(_ selected: AuthorizedFolder, preserveSelection: Bool) {
        guard !busy else { return }
        busy = true
        message = ""
        let cancellation = LibraryCancellation()
        let candidate = PackageSession()
        let priorSelection = preserveSelection ? selectedOrderID : nil
        let priorVendorSelection = preserveSelection ? selectedVendorID : nil
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false }
            do {
                let loaded = try await withTaskCancellationHandler {
                    try await candidate.open(selected.url, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                let canWrite = await candidate.readmeWritable
                var rows: [OrderRow] = []
                var vendorRows: [VendorRow] = []
                var notices: [String]
                let diagnosticOnly = await candidate.diagnosticOnly
                if diagnosticOnly {
                    notices = await candidate.findings
                } else {
                    let listed = try await withTaskCancellationHandler {
                        try await candidate.orders(query: "", caseSensitive: false, sortField: orderSortField, descending: orderSortDescending,
                                                   cancellation: cancellation)
                    } onCancel: { cancellation.request() }
                    rows = listed.rows
                    notices = listed.notices
                    vendorRows = try await withTaskCancellationHandler {
                        try await candidate.vendors(query: "", caseSensitive: false, includeHidden: includeHiddenVendors, sortField: vendorSortField,
                                                     descending: vendorSortDescending, cancellation: cancellation).rows
                    } onCancel: { cancellation.request() }
                }
                try Task.checkCancellation()
                folder = selected
                library = candidate
                root = selected.url
                text = loaded
                original = loaded
                editingOrder = nil
                writable = canWrite
                canBrowseRecords = !diagnosticOnly
                searchAnswer = nil
                orders = rows
                orderCount = rows.count
                query = ""
                spotlightMode = false
                selectedOrderID = rows.first(where: { $0.id == priorSelection && $0.damage == nil })?.id
                inspection = nil
                vendors = vendorRows
                selectedVendorID = vendorRows.first { $0.id == priorVendorSelection }?.id
                vendorDetails = nil
                message = notices.joined(separator: "\n")
                remember(selected)
                if let selectedRow = rows.first(where: { $0.id == selectedOrderID }) {
                    do {
                        inspection = try await withTaskCancellationHandler {
                            try await candidate.inspectOrder(selectedRow, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedOrderID = nil
                        message += "\nThe selection is no longer available.\n" + error.localizedDescription
                    }
                }
                if let selectedVendor = vendorRows.first(where: { $0.id == selectedVendorID }) {
                    do {
                        vendorDetails = try await withTaskCancellationHandler {
                            try await candidate.inspectVendor(selectedVendor, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                    } catch {
                        selectedVendorID = nil
                        message += "\nThe Vendor selection is no longer available.\n" + error.localizedDescription
                    }
                }
            } catch { message = error.localizedDescription }
        }
    }

    func save() {
        guard !busy, editingReadme, writable, dirty, let retainedFolder = folder else { return }
        busy = true
        message = ""
        let draft = text
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let saved = try await withTaskCancellationHandler {
                    try await library.saveReadme(draft, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                original = saved.refreshedText ?? draft
                text = original
                writable = saved.canEdit
                message = saved.canEdit && !saved.hasWarnings ? "" : saved.message
                if let order = editingOrder {
                    do { inspection = try await library.inspectOrder(order, cancellation: cancellation) }
                    catch {
                        inspection = nil
                        writable = false
                        message = saved.message + "\nReopen the Package to refresh this Order.\n" + error.localizedDescription
                    }
                }
            } catch { message = "Couldn’t save. Your draft is retained.\n" + error.localizedDescription }
        }
    }

    func reveal() {
        guard let root else { return }
        reveal(root)
    }

    func reveal(_ location: URL) {
        do {
            guard try location.checkResourceIsReachable() else { throw CocoaError(.fileNoSuchFile) }
            NSWorkspace.shared.activateFileViewerSelecting([location])
        } catch {
            message = "The location is no longer available. Reopen the Package or refresh search.\n" + error.localizedDescription
        }
    }

    func updateQuery(_ value: String) {
        guard value != query else { return }
        query = value
        refreshLookup()
    }

    func setCaseSensitive(_ value: Bool) {
        guard value != caseSensitive else { return }
        caseSensitive = value
        refreshLookup()
    }

    func setSpotlightMode(_ value: Bool) {
        guard value != spotlightMode else { return }
        spotlightMode = value
        refreshLookup()
    }

    func refreshLookup() {
        guard canBrowseRecords, destination != .packageReadme, let retainedFolder = folder else { return }
        lookupRevision &+= 1
        if refreshingLookup {
            activeSearch?.cancel()
            return
        }
        guard !busy else { return }
        busy = true
        refreshingLookup = true
        message = ""
        searchAnswer = nil
        work = OwnedOperation(kind: .search) { [self] in
            defer {
                busy = false
                refreshingLookup = false
                searching = false
                activeSearch = nil
                withExtendedLifetime(retainedFolder) {}
            }
            while !Task.isCancelled {
                let revision = lookupRevision
                let requestedQuery = query
                let requestedCase = caseSensitive
                let requestedDestination = destination
                let useSpotlight = spotlightMode && requestedDestination == .orders
                let cancellation = LibraryCancellation()
                searching = useSpotlight
                searchAnswer = nil
                do {
                    if useSpotlight {
                        let nativeSearch = SpotlightSearch()
                        activeSearch = nativeSearch
                        let answer: MaterialAnswer? = try await withTaskCancellationHandler {
                            let directory = try await library.materialDirectory()
                            try Task.checkCancellation()
                            guard revision == lookupRevision else { return nil }
                            let hits = await nativeSearch.run(directory: directory, text: requestedQuery)
                            try Task.checkCancellation()
                            return try await library.materialSearch(hits, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                        activeSearch = nil
                        guard !Task.isCancelled else { return }
                        if revision != lookupRevision { continue }
                        guard let answer else { return }
                        searchAnswer = answer
                        message = ([searchStatus(answer.state)] + answer.diagnostics).joined(separator: "\n")
                    } else if requestedDestination == .orders {
                        let listed = try await withTaskCancellationHandler {
                            try await library.orders(query: requestedQuery, caseSensitive: requestedCase, sortField: orderSortField,
                                                     descending: orderSortDescending, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                        guard !Task.isCancelled else { return }
                        if revision != lookupRevision { continue }
                        let selected = listed.retainingSelection(orders.first { $0.id == selectedOrderID })
                        orders = listed.rows
                        if requestedQuery.isEmpty { orderCount = orders.count }
                        selectedOrderID = selected?.id
                        if selected == nil { inspection = nil }
                        message = listed.notices.joined(separator: "\n")
                    } else {
                        let listed = try await withTaskCancellationHandler {
                            try await library.vendors(query: requestedQuery, caseSensitive: requestedCase, includeHidden: includeHiddenVendors,
                                                      sortField: vendorSortField, descending: vendorSortDescending, cancellation: cancellation)
                        } onCancel: { cancellation.request() }
                        guard !Task.isCancelled else { return }
                        if revision != lookupRevision { continue }
                        vendors = listed.rows
                        if !vendors.contains(where: { $0.id == selectedVendorID }) { selectedVendorID = nil; vendorDetails = nil }
                        message = listed.notices.joined(separator: "\n")
                    }
                } catch {
                    activeSearch = nil
                    guard !Task.isCancelled else { return }
                    if revision != lookupRevision { continue }
                    message = "Couldn’t search.\n" + error.localizedDescription
                }
                return
            }
        }
    }

    func stopSearch() { activeSearch?.cancel() }

    func openFile(_ location: URL) {
        do {
            guard try location.checkResourceIsReachable() else { throw CocoaError(.fileNoSuchFile) }
            guard NSWorkspace.shared.open(location) else { throw CocoaError(.fileReadUnknown) }
        } catch {
            message = "The file could not be opened. Refresh search or reveal it in Finder.\n" + error.localizedDescription
        }
    }

    private func searchStatus(_ state: Int32) -> String {
        switch state {
        case SPIELORDERS_PROVIDER_STATE_COMPLETE: "Search complete."
        case SPIELORDERS_PROVIDER_STATE_CANCELLED: "Search stopped. Available results are shown."
        default: "Search did not complete."
        }
    }

    func removeOrderToTrash() {
        guard !busy, !dirty, let root, let retainedFolder = folder else { return }
        let picker = NSOpenPanel()
        picker.canChooseDirectories = true
        picker.canChooseFiles = false
        picker.allowsMultipleSelection = false
        picker.directoryURL = root
        picker.prompt = "Select Order"
        picker.message = "Choose the Order Directory to remove and move to Trash."
        guard picker.runModal() == .OK, let directory = picker.url else { return }
        let confirmation = NSAlert()
        confirmation.messageText = "Move this Order to Trash?"
        confirmation.informativeText = directory.lastPathComponent
        confirmation.addButton(withTitle: "Move to Trash")
        confirmation.addButton(withTitle: "Cancel")
        guard confirmation.runModal() == .alertFirstButtonReturn else { return }
        busy = true
        message = ""
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let disposition = try await withTaskCancellationHandler {
                    try await library.removeToTrash(directory, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                searchAnswer = nil
                message = disposition.message
                if disposition.requiresReopen { writable = false }
            } catch { message = "Couldn’t complete removal.\n" + error.localizedDescription }
        }
    }

    func confirmDiscard() -> Bool {
        guard dirty else { return true }
        let alert = NSAlert()
        alert.messageText = "Discard unsaved changes?"
        alert.informativeText = "Your README draft has not been saved."
        alert.addButton(withTitle: "Discard Changes")
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }

    func settle() async {
        await work?.settleForExit()
    }
}
