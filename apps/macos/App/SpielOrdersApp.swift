import AppKit
import Combine
import OSLog
import SwiftUI

@main
struct SpielOrdersApp: App {
    @NSApplicationDelegateAdaptor(NativeApplication.self) var application
    var body: some Scene {
        Window("About SpielOrders", id: AboutView.windowID) {
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

    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About SpielOrders") { openWindow(id: AboutView.windowID) }
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
            Button("New Order", action: application.model.newOrder)
                .keyboardShortcut("n", modifiers: .command)
                .disabled(application.model.busy || application.model.vendorSheet != nil || !application.model.canBrowseRecords)
            Button("Open Package…", action: application.model.choosePackage)
                .keyboardShortcut("o", modifiers: .command)
                .disabled(application.model.busy || application.model.vendorSheet != nil)
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save") {
                if application.model.vendorSheet != nil { application.model.saveVendorSheet() }
                else { application.model.save() }
            }
            .keyboardShortcut("s", modifiers: .command)
            .disabled(application.model.vendorSheet == nil ? !application.model.canSave : application.model.busy)
        }
        OrderCommands(model: application.model)
    }
}

@MainActor final class NativeApplication: NSObject, NSApplicationDelegate, NSWindowDelegate, ObservableObject {
    let model = DesktopModel()
    private var window: NSWindow?
    private var toolbar: PackageToolbar?
    private var terminationPending = false
    @Published private(set) var canUndo = false
    @Published private(set) var canRedo = false
    @Published private(set) var undoTitle = "Undo"
    @Published private(set) var redoTitle = "Redo"
    // Choice: a path-free receipt makes real lifecycle races diagnosable.
    private let lifecycle = Logger(subsystem: Bundle.main.bundleIdentifier ?? "SpielOrders", category: "Lifecycle")

    func applicationDidFinishLaunching(_ notification: Notification) {
        let initialSize = NSSize(width: 1440, height: 860)
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: initialSize),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable],
                              backing: .buffered, defer: false)
        window.title = "SpielOrders"
        let content = NSHostingView(rootView: PackageWindow(model: model))
        content.sizingOptions = [.minSize]
        window.contentView = content
        window.setContentSize(initialSize)
        window.delegate = self
        window.center()
        window.makeKeyAndOrderFront(nil)
        self.window = window
        model.dialogs.window = window
        toolbar = PackageToolbar(model: model, window: window)
        NSApp.activate(ignoringOtherApps: true)
        model.restore()
    }

    func applicationWillUpdate(_ notification: Notification) {
        let manager = NSApp.keyWindow?.firstResponder?.undoManager
        let undo = !model.busy && manager?.canUndo == true
        let redo = !model.busy && manager?.canRedo == true
        let undoName = manager?.undoMenuItemTitle ?? "Undo"
        let redoName = manager?.redoMenuItemTitle ?? "Redo"
        if canUndo != undo { canUndo = undo }
        if canRedo != redo { canRedo = redo }
        if undoTitle != undoName { undoTitle = undoName }
        if redoTitle != redoName { redoTitle = redoName }
    }

    func performUndo() {
        guard !model.busy else { return }
        NSApp.sendAction(Selector(("undo:")), to: nil, from: nil)
    }

    func performRedo() {
        guard !model.busy else { return }
        NSApp.sendAction(Selector(("redo:")), to: nil, from: nil)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        NSApp.terminate(nil)
        return false
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        let activeWork = model.busy ? 1 : 0
        lifecycle.notice("termination-request active-work=\(activeWork, privacy: .public)")
        guard !terminationPending else { return .terminateLater }
        guard !model.dialogs.isPresenting, !model.navigationPending else {
            model.dialogs.focus()
            return .terminateCancel
        }
        guard model.busy || model.dirty else { return .terminateNow }
        terminationPending = true
        model.navigationPending = true
        Task {
            let approved = await model.confirmExit()
            model.navigationPending = false
            terminationPending = false
            sender.reply(toApplicationShouldTerminate: approved)
        }
        return .terminateLater
    }
}
