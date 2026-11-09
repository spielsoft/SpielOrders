import AppKit
import CSpielOrders
import Observation
import SpielOrdersKit

@MainActor @Observable final class DesktopModel {
    var text = ""
    var original = ""
    var root: URL?
    var message = ""
    var busy = false
    var writable = false
    var query = ""
    var searchAnswer: MaterialAnswer?
    var searching = false
    private var library = PackageSession()
    private let preferences = UserDefaults.standard
    private var folder: AuthorizedFolder?
    private var work: OwnedOperation?
    private var activeSearch: SpotlightSearch?
    var dirty: Bool { text != original }

    func restore() {
        guard let bookmark = preferences.data(forKey: "packageBookmark") else { return }
        do { open(try AuthorizedFolder.restore(bookmark)) }
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
        do { open(try AuthorizedFolder(url)) }
        catch { message = error.localizedDescription }
    }

    private func open(_ selected: AuthorizedFolder) {
        guard !busy else { return }
        busy = true
        message = ""
        let cancellation = LibraryCancellation()
        let candidate = PackageSession()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false }
            do {
                let loaded = try await withTaskCancellationHandler {
                    try await candidate.open(selected.url, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                let canWrite = await candidate.readmeWritable
                try Task.checkCancellation()
                folder = selected
                library = candidate
                root = selected.url
                text = loaded
                original = loaded
                writable = canWrite
                searchAnswer = nil
                do { preferences.set(try selected.bookmark(), forKey: "packageBookmark") }
                catch { message = "Opened, but access cannot be remembered: " + error.localizedDescription }
            } catch { message = error.localizedDescription }
        }
    }

    func save() {
        guard !busy, writable, dirty, let retainedFolder = folder else { return }
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
                message = saved.message
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

    func searchMaterial() {
        guard !busy, let retainedFolder = folder else { return }
        busy = true
        searching = true
        message = ""
        searchAnswer = nil
        let requestedQuery = query
        let nativeSearch = SpotlightSearch()
        activeSearch = nativeSearch
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .search) { [self] in
            defer {
                busy = false
                searching = false
                activeSearch = nil
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let answer = try await withTaskCancellationHandler {
                    let directory = try await library.materialDirectory()
                    let hits = await nativeSearch.run(directory: directory, text: requestedQuery)
                    try Task.checkCancellation()
                    return try await library.materialSearch(hits, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                guard !Task.isCancelled, query == requestedQuery else { return }
                searchAnswer = answer
                message = searchStatus(answer.state)
                if !answer.diagnostics.isEmpty { message += "\n" + answer.diagnostics.joined(separator: "\n") }
            } catch { if !Task.isCancelled { message = "Couldn’t search.\n" + error.localizedDescription } }
        }
    }

    func stopSearch() { activeSearch?.cancel() }

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
