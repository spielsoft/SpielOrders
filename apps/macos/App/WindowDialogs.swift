import AppKit
import Observation
import SwiftUI

/// Owns presentation for the Package window, including questions about a nested sheet.
/// Waiting for an answer suspends the caller; it never starts a nested event loop.
@MainActor @Observable final class WindowDialogs {
    weak var window: NSWindow?
    private(set) var isPresenting = false
    var isTerminating = false
    private var vendorWindow: NSWindow?

    /// A question is asked where the user can see it: a hidden app or a minimized window
    /// is shown again first, so Quit from the Dock still asks about unsaved changes.
    private var parent: NSWindow? {
        guard var parent = window else { return nil }
        if NSApp.isHidden { NSApp.unhide(nil) }
        if parent.isMiniaturized { parent.deminiaturize(nil) }
        while let sheet = parent.attachedSheet { parent = sheet }
        return parent
    }

    func present(_ alert: NSAlert) async -> NSApplication.ModalResponse {
        guard !isTerminating, !isPresenting, let parent else { return .cancel }
        isPresenting = true
        defer { isPresenting = false }
        return await alert.beginSheetModal(for: parent)
    }

    func chooseFolder(_ panel: NSOpenPanel) async -> NSApplication.ModalResponse {
        guard !isTerminating, !isPresenting, let parent else { return .cancel }
        isPresenting = true
        defer { isPresenting = false }
        return await panel.beginSheetModal(for: parent)
    }

    func focus() { parent?.makeKeyAndOrderFront(nil) }

    func presentVendor(_ editor: RecordEditor, model: DesktopModel) -> Bool {
        guard let window, vendorWindow == nil, !isTerminating else { return false }
        let content = VendorEditorView(model: model, editor: editor, inSheet: true)
            .padding(20).frame(minWidth: 720, minHeight: 520)
        let sheet = NSWindow(contentViewController: NSHostingController(rootView: content))
        sheet.title = "New Vendor"
        sheet.styleMask = [.titled]
        // The application delegate resolves the Vendor and parent Order drafts on Quit.
        // AppKit otherwise rejects termination before consulting that delegate.
        sheet.preventsApplicationTerminationWhenModal = false
        vendorWindow = sheet
        window.beginSheet(sheet) { [weak self] _ in
            sheet.orderOut(nil)
            self?.vendorWindow = nil
        }
        return true
    }

    func dismissVendor() {
        guard let sheet = vendorWindow else { return }
        sheet.sheetParent?.endSheet(sheet)
    }

    func waitForDismissal(of sheet: NSWindow?) async {
        guard let sheet, let window, window.attachedSheet === sheet else { return }
        // Finish dismissing the editor before asking about the parent Order.
        let endings = NotificationCenter.default.notifications(named: NSWindow.didEndSheetNotification, object: window).map { _ in () }
        for await _ in endings { return }
    }
}

extension DesktopModel {
    func confirm(_ title: String, _ detail: String, identifier: String,
                 approve: String = "Discard Changes", destructive: Bool = false) async -> Bool {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        alert.addButton(withTitle: approve)
        alert.addButton(withTitle: "Cancel")
        alert.buttons[0].hasDestructiveAction = destructive
        if destructive {
            // An unintended destructive alternative has no default, per Apple's HIG.
            alert.buttons[0].keyEquivalent = ""
            alert.window.defaultButtonCell = nil
        }
        alert.window.setAccessibilityLabel(title)
        alert.window.setAccessibilityIdentifier(identifier + ".dialog")
        alert.buttons[0].setAccessibilityIdentifier(identifier + ".confirm")
        alert.buttons[1].setAccessibilityIdentifier(identifier + ".cancel")
        return await dialogs.present(alert) == .alertFirstButtonReturn
    }
}
