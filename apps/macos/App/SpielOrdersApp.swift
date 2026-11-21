import AppKit
import Combine
import Observation
import OSLog
import SwiftUI

@main
struct SpielOrdersApp: App {
    @NSApplicationDelegateAdaptor(NativeApplication.self) var application
    var body: some Scene {
        WindowGroup("About SpielOrders", id: AboutView.windowID, for: String.self) { _ in
            AboutView()
        }
        .windowResizability(.contentSize)
        .restorationBehavior(.disabled)
        .defaultLaunchBehavior(.suppressed)
        .handlesExternalEvents(matching: [])
        .commands { ApplicationCommands(application: application) }
    }
}

private struct ApplicationCommands: Commands {
    @ObservedObject var application: NativeApplication
    @Environment(\.openWindow) private var openWindow

    private var model: DesktopModel { application.model }

    /// Whether work or the Vendor sheet keeps the window on its current Package.
    private var packageNavigationBlocked: Bool {
        (model.busy && !model.committing) || model.vendorSheet != nil
    }

    /// Create names the new record it would write.
    private var createTitle: String {
        model.vendorSheet != nil || model.recordEditor?.kind == .vendor ? "Create Vendor" : "Create Order"
    }

    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About SpielOrders") { openWindow(id: AboutView.windowID, value: AboutView.windowID) }
                .accessibilityIdentifier("app.about")
        }
        CommandGroup(replacing: .undoRedo) {
            Button(application.undoTitle, action: application.performUndo)
                .keyboardShortcut("z", modifiers: .command)
                .disabled(!application.canUndo)
                .accessibilityIdentifier("edit.undo")
            Button(application.redoTitle, action: application.performRedo)
                .keyboardShortcut("z", modifiers: [.command, .shift])
                .disabled(!application.canRedo)
                .accessibilityIdentifier("edit.redo")
        }
        CommandGroup(replacing: .newItem) {
            Button(model.destination.newTitle) { application.inWindow { $0.newRecord() } }
                .keyboardShortcut("n", modifiers: .command)
                .disabled(!model.canStartNew)
                .accessibilityIdentifier("file.new")
            Button(createTitle) {
                NSApp.keyWindow?.makeFirstResponder(nil)
                if model.vendorSheet != nil {
                    model.saveVendorSheet()
                } else {
                    model.createRecord()
                }
            }
            .keyboardShortcut(.return, modifiers: .command)
            .disabled(!model.canCreateRecord)
            .accessibilityIdentifier("file.create")
            Button("New Package…") { application.inWindow { $0.chooseInitialization() } }
                .disabled(packageNavigationBlocked)
                .accessibilityIdentifier("file.initializePackage")
            Button("Open Package…") { application.inWindow { $0.choosePackage() } }
                .keyboardShortcut("o", modifiers: .command)
                .disabled(packageNavigationBlocked)
                .accessibilityIdentifier("file.openPackage")
            Menu("Open Recent") {
                ForEach(model.recents.entries) { recent in
                    Button(recent.name) { application.inWindow { $0.openRecent(recent) } }
                        .disabled(packageNavigationBlocked)
                }
                Divider()
                Button("Clear Menu", action: model.recents.clear)
                    .disabled(model.recents.entries.isEmpty)
                    .accessibilityIdentifier("file.openRecent.clear")
            }
            .accessibilityIdentifier("file.openRecent")
            Button("Reload Package") { application.inWindow { $0.reloadPackage() } }
                .disabled(packageNavigationBlocked || model.root == nil)
                .accessibilityIdentifier("file.reopenPackage")
            Divider()
            Button("Set Up Agent Guidance…") { application.inWindow { $0.makeAgentReady() } }
                .disabled(model.busy || model.vendorSheet != nil)
                .accessibilityIdentifier("file.agentReady")
        }
        CommandGroup(replacing: .saveItem) {
            Button("Close Window") {
                NSApp.sendAction(#selector(NSWindow.performClose(_:)), to: nil, from: nil)
            }
            .keyboardShortcut("w", modifiers: .command)
            .disabled(!application.canClose)
            .accessibilityIdentifier("file.close")
            Button("Save") {
                application.model.saveExpertDocument()
            }
            .keyboardShortcut("s", modifiers: .command)
            .disabled(!application.model.canSave)
            .accessibilityIdentifier("file.save")
        }
        CommandGroup(replacing: .textEditing) {
            Menu("Find") {
                Button("Find…", action: application.findInDestination)
                    .keyboardShortcut("f", modifiers: .command)
                    .disabled(!model.canFind)
                    .accessibilityIdentifier("edit.find")
            }
        }
        CommandMenu("Package") {
            Button("Diagnose Package") { application.inWindow { $0.showDiagnosis(directory: nil) } }
                .disabled((application.model.busy && !application.model.committing) || application.model.vendorSheet != nil || application.model.root == nil)
                .accessibilityIdentifier("package.diagnose")
            Button("Show Package in Finder") { application.model.reveal() }
                .disabled((application.model.busy && !application.model.committing) || application.model.root == nil)
                .accessibilityIdentifier("package.reveal")
        }
        RecordCommands(model: model, deleteKeyRemoves: application.deleteKeyRemoves)
        CommandGroup(replacing: .sidebar) {
            Button(application.model.sidebarVisible ? "Hide Sidebar" : "Show Sidebar") {
                application.inWindow { $0.sidebarVisible.toggle() }
            }
            .keyboardShortcut("s", modifiers: [.control, .command])
            .disabled(application.model.root == nil)
            .accessibilityIdentifier("view.sidebar")
        }
        CommandGroup(after: .sidebar) {
            ForEach(Array(DesktopModel.Destination.fixed.enumerated()), id: \.element) { index, destination in
                Button(destination.title) { application.inWindow { $0.navigate(to: destination) } }
                    .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")), modifiers: .command)
                    .disabled((application.model.busy && !application.model.committing) || !application.model.canBrowseRecords)
                    .accessibilityIdentifier("view.\(destination.title)")
            }
        }
    }
}

@MainActor final class NativeApplication: NSObject, NSApplicationDelegate, NSWindowDelegate, ObservableObject {
    let model = DesktopModel()
    private var window: NSWindow?
    private var toolbar: PackageToolbar?
    private var terminationPending = false
    private var closePending = false
    private var receivedDocument = false
    @Published private(set) var canClose = false
    @Published private(set) var canUndo = false
    @Published private(set) var canRedo = false
    @Published private(set) var undoTitle = "Undo"
    @Published private(set) var redoTitle = "Redo"
    @Published private(set) var deleteKeyRemoves = false
    private var editingObservers: [NSObjectProtocol] = []
    private var editingUpdatePending = false
    // Choice: a path-free receipt makes real lifecycle races diagnosable.
    private let lifecycle = Logger(subsystem: Bundle.main.bundleIdentifier ?? "SpielOrders", category: "Lifecycle")

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSWindow.allowsAutomaticWindowTabbing = false
        // Restoring first lets the window open at its welcome size when nothing is remembered.
        if !receivedDocument { model.restore() }
        showWindow()
        observeTitle()
        let changes: [Notification.Name] = [.NSUndoManagerCheckpoint, .NSUndoManagerDidUndoChange,
            .NSUndoManagerDidRedoChange, .NSUndoManagerDidCloseUndoGroup,
            NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification]
        editingObservers = changes.map { name in
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.scheduleEditing() }
            }
        }
        observeEditing()
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        receivedDocument = true
        showWindow()
        model.openDocument(urls)
    }

    func showWindow() {
        if let window {
            if window.isMiniaturized { window.deminiaturize(nil) }
            window.makeKeyAndOrderFront(nil)
            return
        }
        let initialSize = Self.initialSize
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: initialSize),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.title = "SpielOrders"
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.setAccessibilityIdentifier("package.window")
        window.contentViewController = PackageSplit(model: model)
        window.setContentSize(initialSize)
        window.delegate = self
        window.center()
        window.makeKeyAndOrderFront(nil)
        self.window = window
        model.dialogs.window = window
        toolbar = PackageToolbar(model: model, window: window)
        NSApp.activate(ignoringOtherApps: true)
    }

    /// Room for the destinations, the default Orders columns, and the detail, within the screen.
    /// The welcome view compacts the window while no Package is open.
    private static var initialSize: NSSize {
        let screen = NSScreen.main?.visibleFrame.size ?? NSSize(width: 1680, height: 900)
        return NSSize(width: min(1680, screen.width), height: min(900, screen.height))
    }

    func inWindow(_ action: (DesktopModel) -> Void) {
        showWindow()
        action(model)
    }

    /// Find…: the cursor goes to the destination's search field, its text selected.
    func findInDestination() {
        showWindow()
        guard let window, let field = window.contentView?.searchField(RecordSearch.field) else { return }
        if window.makeFirstResponder(field) {
            field.currentEditor()?.selectAll(nil)
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showWindow()
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    /// The title is the open Package's name as Finder shows it, as a document window's is; the
    /// proxy icon and path menu represent its folder.
    private func observeTitle() {
        withObservationTracking {
            window?.representedURL = model.root
            window?.title = model.root.map { FileManager.default.displayName(atPath: $0.path) } ?? "SpielOrders"
            window?.isDocumentEdited = model.dirty
        } onChange: { [weak self] in
            Task { @MainActor in self?.observeTitle() }
        }
    }

    func applicationWillUpdate(_ notification: Notification) {
        if !editingUpdatePending {
            editingUpdatePending = true
            refreshEditing()
            editingUpdatePending = false
        }
        let close = NSApp.keyWindow?.standardWindowButton(.closeButton)?.isEnabled == true
        if canClose != close { canClose = close }
        let delete = deleteKeyReachesMenu
        if deleteKeyRemoves != delete { deleteKeyRemoves = delete }
    }

    /// Whether the Delete key reaches the record menu's Remove Order: the keyboard is in the
    /// Orders list, or in text, which takes the key before the menu does. Anywhere else the key
    /// is not the menu's: a control keeps it, such as the Items table removing its rows, and
    /// with the keyboard nowhere, as after − or Create, it removes nothing.
    private var deleteKeyReachesMenu: Bool {
        guard let window, window.isKeyWindow, let responder = window.firstResponder else { return false }
        return responder is OrdersTableView || responder is NSText
    }

    private func observeEditing() {
        withObservationTracking { _ = model.busy } onChange: { [weak self] in
            Task { @MainActor in self?.observeEditing() }
        }
        scheduleEditing()
    }

    private func scheduleEditing() {
        guard !editingUpdatePending else { return }
        editingUpdatePending = true
        // Checkpoints arrive before a group changes. Read it after the current action,
        // and suppress the checkpoints emitted by querying the manager itself.
        Task { @MainActor [weak self] in
            guard let self else { return }
            refreshEditing()
            editingUpdatePending = false
        }
    }

    private func refreshEditing() {
        let undoHistory = model.editHistory(in: NSApp.keyWindow, redo: false)
        let redoHistory = model.editHistory(in: NSApp.keyWindow, redo: true)
        let undo = available(undoHistory) && undoHistory?.canUndo == true
        let redo = available(redoHistory) && redoHistory?.canRedo == true
        let undoName = undoHistory?.undoMenuItemTitle ?? "Undo"
        let redoName = redoHistory?.redoMenuItemTitle ?? "Redo"
        if canUndo != undo { canUndo = undo }
        if canRedo != redo { canRedo = redo }
        if undoTitle != undoName { undoTitle = undoName }
        if redoTitle != redoName { redoTitle = redoName }
    }

    func performUndo() {
        guard let window = NSApp.keyWindow, available(model.editHistory(in: window, redo: false)) else { return }
        model.undo(in: window)
    }

    func performRedo() {
        guard let window = NSApp.keyWindow, available(model.editHistory(in: window, redo: true)) else { return }
        model.redo(in: window)
    }

    /// Nothing is undone while work is under way. The window's history applies its actions
    /// through the library, so it also waits until the Package can be browsed.
    private func available(_ history: UndoManager?) -> Bool {
        !model.busy && (history !== model.undoManager || model.canBrowseRecords)
    }

    /// The Package window's Undo history is the model's, so it survives selection changes.
    func windowWillReturnUndoManager(_ window: NSWindow) -> UndoManager? {
        model.undoManager
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        let activeWork = model.busy ? 1 : 0
        lifecycle.notice("close-request active-work=\(activeWork, privacy: .public)")
        guard !closePending, !terminationPending else { return false }
        guard !model.dialogs.isPresenting, !model.navigationPending else {
            model.dialogs.focus()
            return false
        }
        guard model.root != nil else {
            NSApp.terminate(sender)
            return false
        }
        guard model.busy || model.dirty else {
            model.closePackage()
            return false
        }
        closePending = true
        model.navigationPending = true
        Task {
            let approved = await model.confirmExit()
            if approved { model.closePackage() }
            model.navigationPending = false
            closePending = false
            await model.dialogs.finishExit(showNotices: true)
        }
        return false
    }

    func applicationWillTerminate(_ notification: Notification) {
        model.willQuit()
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        let activeWork = model.busy ? 1 : 0
        lifecycle.notice("termination-request active-work=\(activeWork, privacy: .public)")
        guard !terminationPending else { return .terminateLater }
        guard !model.dialogs.isPresenting, !model.navigationPending else {
            model.dialogs.focus()
            return .terminateCancel
        }
        guard model.busy || model.dirty else {
            return .terminateNow
        }
        terminationPending = true
        model.navigationPending = true
        Task {
            let approved = await model.confirmExit()
            model.navigationPending = false
            terminationPending = false
            sender.reply(toApplicationShouldTerminate: approved)
            await model.dialogs.finishExit(showNotices: !approved)
        }
        return .terminateLater
    }
}
