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
    private var defersNotices = false
    private var notifying = false
    private var pending: [PendingAlert] = []

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
        defer { didPresent(alert.window) }
        return await alert.beginSheetModal(for: parent)
    }

    func chooseFolder(_ panel: NSOpenPanel) async -> NSApplication.ModalResponse {
        guard !isTerminating, !isPresenting, let parent else { return .cancel }
        isPresenting = true
        defer { didPresent(panel) }
        return await panel.beginSheetModal(for: parent)
    }

    func configurePackage(_ folder: URL) async -> PackageOptions? {
        guard !isTerminating, !isPresenting, let parent else { return nil }
        isPresenting = true
        let sheet = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 540, height: 260),
                             styleMask: [.titled], backing: .buffered, defer: false)
        defer { didPresent(sheet) }
        sheet.title = "New Package"
        sheet.setAccessibilityIdentifier("package.initialize.confirmation")
        sheet.preventsApplicationTerminationWhenModal = false
        var options: PackageOptions?
        sheet.contentView = NSHostingView(rootView: PackageSetup(folder: folder, choose: { [weak self, weak sheet] directory in
            guard let self, let sheet else { return nil }
            return await chooseDirectory(directory, in: folder, parent: sheet)
        }) { [weak sheet, weak parent] selected in
            guard let sheet, let parent else { return }
            options = selected
            parent.endSheet(sheet, returnCode: selected == nil ? .cancel : .OK)
        })
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            parent.beginSheet(sheet) { _ in
                sheet.orderOut(nil)
                continuation.resume()
            }
        }
        return options
    }

    private func chooseDirectory(_ directory: SetupDirectory, in root: URL, parent: NSWindow) async -> String? {
        while true {
            let panel = NSOpenPanel()
            panel.title = "Choose \(directory.rawValue)"
            panel.message = directory.help + " Choose an existing folder inside \(root.lastPathComponent)."
            panel.prompt = "Choose Folder"
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.canCreateDirectories = false
            panel.allowsMultipleSelection = false
            panel.resolvesAliases = false
            panel.treatsFilePackagesAsDirectories = true
            panel.directoryURL = root
            panel.setAccessibilityLabel(panel.title)
            panel.setAccessibilityIdentifier(directory.identifier + ".picker")
            let response = await panel.beginSheetModal(for: parent)
            await waitForDismissal(of: panel)
            guard response == .OK, let selected = panel.url else { return nil }
            let base = root.standardizedFileURL.pathComponents
            let target = selected.standardizedFileURL.pathComponents
            if target.count > base.count, target.prefix(base.count).elementsEqual(base) {
                return target.dropFirst(base.count).joined(separator: "/")
            }
            let alert = NSAlert()
            alert.messageText = "Choose a folder inside the Package"
            alert.informativeText = "The \(directory.rawValue.lowercased()) must be inside \(root.lastPathComponent), not the Package folder itself. Choose another folder, or type a folder name in the setup form."
            alert.addButton(withTitle: "Choose Again")
            alert.addButton(withTitle: "Cancel")
            alert.window.setAccessibilityIdentifier(directory.identifier + ".outside")
            alert.buttons[0].setAccessibilityIdentifier(directory.identifier + ".chooseAgain")
            alert.buttons[1].setAccessibilityIdentifier(directory.identifier + ".cancelChoice")
            let retry = await alert.beginSheetModal(for: parent) == .alertFirstButtonReturn
            await waitForDismissal(of: alert.window)
            if !retry { return nil }
        }
    }

    func focus() { parent?.makeKeyAndOrderFront(nil) }

    func showDetails(_ notices: [Notice]) async {
        guard !isTerminating, !isPresenting, let parent else { return }
        isPresenting = true
        let sheet = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 720, height: 500),
                             styleMask: [.titled], backing: .buffered, defer: false)
        defer { didPresent(sheet) }
        sheet.title = "Details"
        sheet.setAccessibilityIdentifier("feedback.sheet")
        sheet.contentView = NSHostingView(rootView: FeedbackDetails(notices: notices) { [weak sheet, weak parent] in
            guard let sheet, let parent else { return }
            parent.endSheet(sheet)
        })
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            parent.beginSheet(sheet) { _ in
                sheet.orderOut(nil)
                continuation.resume()
            }
        }
    }

    func review(_ title: String, explanation: String, details: String, identifier: String,
                approve: String, destructive: Bool) async -> Bool {
        guard !isTerminating, !isPresenting, let parent else { return false }
        isPresenting = true
        let sheet = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 720, height: 500),
                             styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        defer { didPresent(sheet) }
        sheet.title = title
        sheet.setAccessibilityIdentifier(identifier + ".dialog")
        var approved = false
        sheet.contentView = NSHostingView(rootView: OperationReview(title: title, explanation: explanation,
            details: details, identifier: identifier, approve: approve, destructive: destructive) { [weak sheet, weak parent] result in
                guard let sheet, let parent else { return }
                approved = result
                parent.endSheet(sheet)
            })
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            parent.beginSheet(sheet) { _ in
                sheet.orderOut(nil)
                continuation.resume()
            }
        }
        return approved
    }

    /// Attaches `content` as a sheet and waits until it finishes with an answer; nil when it is
    /// cancelled or the window cannot ask.
    func ask<Answer, Content: View>(_ title: String, identifier: String,
                                    @ViewBuilder content: (@escaping (Answer?) -> Void) -> Content) async -> Answer? {
        guard !isTerminating, !isPresenting, let parent else { return nil }
        isPresenting = true
        let sheet = NSWindow(contentRect: .zero, styleMask: [.titled], backing: .buffered, defer: false)
        defer { didPresent(sheet) }
        sheet.title = title
        sheet.setAccessibilityIdentifier(identifier + ".dialog")
        var answer: Answer?
        let view = NSHostingView(rootView: content { [weak sheet, weak parent] chosen in
            guard let sheet, let parent else { return }
            answer = chosen
            parent.endSheet(sheet)
        })
        sheet.contentView = view
        sheet.setContentSize(view.fittingSize)
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            parent.beginSheet(sheet) { _ in
                sheet.orderOut(nil)
                continuation.resume()
            }
        }
        return answer
    }

    private func didPresent(_ sheet: NSWindow) {
        isPresenting = false
        Task {
            await waitForDismissal(of: sheet)
            await showNotices()
        }
    }

    /// Outcomes wait behind an existing sheet and the whole exit decision, so a
    /// notification cannot hold up the work that an exit is settling.
    func notify(_ title: String, _ detail: String, identifier: String, diagnostic: String? = nil,
                location: URL? = nil) async {
        pending.append(PendingAlert(title: title, detail: detail, identifier: identifier,
                                    diagnostic: diagnostic, location: location))
        await showNotices()
    }

    private func showNotices() async {
        guard !notifying, !defersNotices, !isTerminating, !isPresenting, window != nil else { return }
        notifying = true
        defer { notifying = false }
        // A completion callback can precede AppKit detaching the sheet. A newly
        // queued notice must wait too, not attach itself to that closing panel.
        if let sheet = window?.attachedSheet { await waitForDismissal(of: sheet) }
        while !pending.isEmpty, !defersNotices, !isTerminating, !isPresenting {
            let notice = pending.removeFirst()
            let alert = notice.alert
            let response = await present(alert)
            await waitForDismissal(of: alert.window)
            switch notice.choice(response) {
            case .details:
                await showDetails([Notice(notice.title, details: notice.diagnostic ?? "")])
            case .reveal:
                if let location = notice.location { NSWorkspace.shared.activateFileViewerSelecting([location]) }
            case nil:
                break
            }
        }
    }

    func beginExit() { defersNotices = true }

    func finishExit(showNotices: Bool) async {
        if !showNotices { pending.removeAll() }
        defersNotices = false
        if showNotices { await self.showNotices() }
    }

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
        guard let sheet, let parent = sheet.sheetParent, parent.attachedSheet === sheet else { return }
        // Finish dismissing a child sheet before presenting its next sibling.
        let endings = NotificationCenter.default.notifications(named: NSWindow.didEndSheetNotification, object: parent).map { _ in () }
        for await _ in endings { return }
    }
}

/// An alert waiting for the window: OK, with Details and Show in Finder when there is
/// something to show.
private struct PendingAlert {
    enum Choice { case details, reveal }

    let title: String
    let detail: String
    let identifier: String
    let diagnostic: String?
    let location: URL?

    /// The choices after OK, in button order.
    private var choices: [Choice] {
        (diagnostic == nil ? [] : [.details]) + (location == nil ? [] : [.reveal])
    }

    @MainActor var alert: NSAlert {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        alert.window.setAccessibilityIdentifier(identifier + ".result")
        alert.addButton(withTitle: "OK").setAccessibilityIdentifier(identifier + ".dismiss")
        for choice in choices {
            let button = alert.addButton(withTitle: choice == .details ? "Details" : "Show in Finder")
            button.setAccessibilityIdentifier(identifier + (choice == .details ? ".details" : ".reveal"))
        }
        return alert
    }

    func choice(_ response: NSApplication.ModalResponse) -> Choice? {
        let index = response.rawValue - NSApplication.ModalResponse.alertSecondButtonReturn.rawValue
        return choices.indices.contains(index) ? choices[index] : nil
    }
}

struct PackageOptions {
    let orders: String
    let removed: String
}

private enum SetupDirectory: String {
    case orders = "Orders folder"
    case removed = "Removed Orders folder"

    var identifier: String {
        "package.initialize." + (self == .orders ? "ordersDirectory" : "removedDirectory")
    }

    var help: String {
        switch self {
        case .orders: "Holds one folder per Order. Enter a name, or choose a folder inside the Package. Leave blank for the default."
        case .removed: "Keeps removed Orders recoverable. Separate from the macOS Trash. Leave blank for the default."
        }
    }
}

private struct PackageSetup: View {
    let folder: URL
    let choose: (SetupDirectory) async -> String?
    let complete: (PackageOptions?) -> Void
    @State private var orders = ""
    @State private var removed = ""
    @State private var choosing = false
    @FocusState private var focus: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New Package in “\(folder.lastPathComponent)”").font(.headline)
            Text("Existing content is preserved. Leave folder names blank for the defaults.")
                .foregroundStyle(.secondary)
            Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 10) {
                GridRow {
                    Text("Orders folder").help(SetupDirectory.orders.help)
                    TextField("Default Orders folder", text: $orders)
                        .accessibilityLabel("Orders folder")
                        .accessibilityIdentifier("package.initialize.ordersDirectory")
                        .help(SetupDirectory.orders.help).accessibilityHint(SetupDirectory.orders.help)
                        .focused($focus)
                    picker(.orders)
                }
                GridRow {
                    Text("Removed Orders folder").help(SetupDirectory.removed.help)
                    TextField("Default Removed Orders folder", text: $removed)
                        .accessibilityLabel("Removed Orders folder")
                        .accessibilityIdentifier("package.initialize.removedDirectory")
                        .help(SetupDirectory.removed.help).accessibilityHint(SetupDirectory.removed.help)
                    picker(.removed)
                }
            }
            HStack {
                Spacer()
                Button("Cancel") { complete(nil) }
                    .keyboardShortcut(.cancelAction).accessibilityIdentifier("package.initialize.cancel")
                Button("Create Package") { complete(PackageOptions(orders: orders, removed: removed)) }
                    .keyboardShortcut(.defaultAction).accessibilityIdentifier("package.initialize.confirm")
            }
        }
        .padding(20).frame(width: 540)
        .disabled(choosing)
        .task { focus = true }
    }

    private func picker(_ directory: SetupDirectory) -> some View {
        Button("Choose…") {
            choosing = true
            Task {
                defer { choosing = false }
                guard let selected = await choose(directory) else { return }
                if directory == .orders { orders = selected }
                else { removed = selected }
            }
        }
        .controlSize(.small)
        .accessibilityLabel("Choose \(directory.rawValue)")
        .accessibilityIdentifier(directory.identifier + ".choose")
        .help(directory.help).accessibilityHint(directory.help)
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
