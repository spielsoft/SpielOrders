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
    }
}

@MainActor final class NativeApplication: NSObject, NSApplicationDelegate, NSWindowDelegate, ObservableObject {
    private let model = DesktopModel()
    private var window: NSWindow?
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
        guard model.confirmDiscard(retry: { NSApp.terminate(nil) }) else { return .terminateCancel }
        guard model.busy else { return .terminateNow }
        terminationPending = true
        Task {
            await model.settle()
            terminationPending = false
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}
