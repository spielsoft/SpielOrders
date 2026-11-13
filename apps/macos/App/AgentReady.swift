import AppKit
import SpielOrdersKit

extension DesktopModel {
    func makeAgentReady() {
        // This is not Package navigation: do not finish, save, or discard its draft.
        guard !busy, vendorSheet == nil else { return }
        let picker = NSOpenPanel()
        picker.title = "Set Up Agent Guidance"
        picker.message = "Choose a folder for agent guidance."
        picker.prompt = "Choose Folder"
        picker.canChooseDirectories = true
        picker.canChooseFiles = false
        picker.canCreateDirectories = false
        picker.allowsMultipleSelection = false
        picker.resolvesAliases = false
        picker.treatsFilePackagesAsDirectories = true
        picker.setAccessibilityLabel("Agent guidance folder")
        picker.setAccessibilityIdentifier("agentReady.folder")
        activity = "Choosing a folder for agent guidance…"
        busy = true
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false }
            let response = await dialogs.chooseFolder(picker)
            await dialogs.waitForDismissal(of: picker)
            guard response == .OK, let selected = picker.url else { return }
            let access: AuthorizedFolder
            do { access = try AuthorizedFolder(selected) }
            catch {
                await agentNotice("Folder access unavailable", "Choose the folder again to allow guidance to be written.", diagnostic: error.localizedDescription)
                return
            }
            defer { withExtendedLifetime(access) {} }
            do {
                try Task.checkCancellation()
                activity = "Setting up agent guidance…"
                let cancellation = LibraryCancellation()
                let notices = try await withTaskCancellationHandler {
                    try await AgentGuidance.provision(in: access.url, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                // A cancellation during commit cannot undo its completed writes.
                // Present the actual report without checking cancellation again.
                await agentNotice("“\(selected.lastPathComponent)” is ready for agents.", "",
                                  diagnostic: notices.isEmpty ? nil : diagnosticText(notices))
            } catch {
                if error is CancellationError {
                    await agentNotice("Agent guidance cancelled", "No guidance was written.")
                } else {
                    let explanation = "Guidance did not complete in “\(selected.lastPathComponent)”. "
                        + "AGENTS.md and your other files are untouched. Read Details for what was written."
                    await agentNotice("Agent guidance incomplete", explanation, diagnostic: error.localizedDescription)
                }
            }
        }
    }

    private func agentNotice(_ title: String, _ detail: String, diagnostic: String? = nil) async {
        await dialogs.notify(title, detail, identifier: "agentReady", diagnostic: diagnostic)
    }
}
