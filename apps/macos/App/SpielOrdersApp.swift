import AppKit
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
        .commands { ApplicationCommands() }
    }
}

private struct ApplicationCommands: Commands {
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About SpielOrders") { openWindow(id: AboutView.windowID) }
        }
    }
}

@MainActor final class NativeApplication: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private let model = DesktopModel()
    private var window: NSWindow?
    private var terminationPending = false
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

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        NSApp.terminate(nil)
        return false
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        let activeWork = model.busy ? 1 : 0
        lifecycle.notice("termination-request active-work=\(activeWork, privacy: .public)")
        guard !terminationPending else { return .terminateLater }
        guard model.confirmDiscard() else { return .terminateCancel }
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
