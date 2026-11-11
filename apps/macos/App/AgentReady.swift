import AppKit
import SpielOrdersKit

extension DesktopModel {
    func makeAgentReady() {
        guard !busy, let root, canBrowseRecords else { return }
        busy = true
        message = "Authorize the bundled CLI for this Package."
        let authorization = CLIAuthorization()
        work = OwnedOperation(kind: .read) { [self] in
            do {
                let approved = try await authorization.authorize(root)
                try Task.checkCancellation()
                guard approved else {
                    busy = false
                    message = "Agent Ready cancelled. No guidance was written."
                    return
                }
                provisionAgentGuidance()
            } catch {
                busy = false
                message = Task.isCancelled ? "Agent Ready cancelled. No guidance was written."
                    : "Agent Ready is incomplete.\n" + error.localizedDescription
            }
        }
    }

    private func provisionAgentGuidance() {
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false }
            do {
                let guidance = try await withTaskCancellationHandler {
                    try await library.provisionAgentGuidance(cancellation: cancellation)
                } onCancel: { cancellation.request() }
                let executable = Bundle.main.bundleURL
                    .appendingPathComponent(AgentAuthorizationContract.relativeBundle)
                    .appendingPathComponent("Contents/MacOS/spielorders")
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(guidance.prompt, forType: .string)
                message = guidance.message + "\nCLI folder access authorized.\nCLI: " + executable.path
                    + "\nThe agent setup request is copied. Paste it into your agent."
            } catch {
                message = "Agent Ready guidance is incomplete. The CLI folder grant is saved.\n" + error.localizedDescription
            }
        }
    }
}
