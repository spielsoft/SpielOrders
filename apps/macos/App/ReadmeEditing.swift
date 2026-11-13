import SpielOrdersKit

/// The shown README saves itself, as a record's fields do: when typing pauses or its text
/// loses focus, one save at a time, keeping input typed while a save is written.
extension DesktopModel {
    /// Whether `readme` accepts input. It stays live while its own save or a record's
    /// automatic save is written.
    func canType(_ readme: ReadmeSession) -> Bool {
        guard canBrowseRecords, readme.writable else { return false }
        guard busy else { return true }
        return committing && !navigationPending && !dialogs.isPresenting
    }

    /// Shows `text` as the README of `order`, or of the Package when `order` is nil.
    func showReadme(_ text: ReadmeText, of order: OrderRow?) {
        let session = ReadmeSession(order: order, readme: text)
        session.onPause = { [weak self, weak session] in
            guard let self, let session, readme === session else { return }
            saveReadme()
        }
        readme = session
    }

    /// Reads the Package README and shows it in the content area.
    func openPackageReadme() {
        readReadme(of: nil) { [self] text in
            discardEditors()
            destination = .packageReadme
            showReadme(text, of: nil)
        }
    }

    /// Saves the shown README's unsaved text. While other work runs, it waits for the next pause.
    func saveReadme() {
        guard let readme, readme.draft.unsaved, !readme.draft.saving else { return }
        guard canBrowseRecords, readme.writable, let retainedFolder = folder else {
            guard !readme.refused else { return }
            let explanation = "This README has text that can’t be written until the Package is reloaded. "
                + "Choose File → Reload Package; it asks before discarding the text."
            readme.refuse(Notice(explanation, details: reloadReason?.details ?? ""))
            return
        }
        guard !busy else {
            readme.waitForPause()
            return
        }
        guard let text = readme.submit() else { return }
        let session = library
        busy = true
        committing = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            var completed = false
            defer {
                busy = false
                committing = false
                if readme.draft.unsaved, !readme.refused, readme.writable { readme.waitForPause() }
                let field = readme.afterSave
                readme.afterSave = nil
                field?()
                // A navigation that waited for this save proceeds once it is written.
                if !committing {
                    let next = afterCommit
                    afterCommit = nil
                    let continueAfterFailure = self.continueAfterFailure
                    self.continueAfterFailure = false
                    if completed || continueAfterFailure, !navigationPending { next?() }
                }
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let saved = try await withTaskCancellationHandler {
                    try await session.saveReadme(text, of: readme.order, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                readme.saved(saved)
                if !saved.canEdit, readme.draft.unsaved {
                    readme.refuse(Notice("The submitted text was saved. Later text is kept here; reload the Package "
                        + "before continuing. Reload asks before discarding it.", details: saved.message))
                }
                completed = true
            } catch {
                readme.refuse(Notice("Your text is kept. Retry, or Revert to the saved README.",
                                     details: error.localizedDescription))
            }
        }
    }

    /// Saves the shown README's unsaved text and waits for it; false when it stays unsaved.
    func settleReadme() async -> Bool {
        guard let readme else { return true }
        while readme.draft.unsaved {
            let reserved = navigationPending
            navigationPending = false
            saveReadme()
            navigationPending = reserved
            await waitForWork()
            if readme.refused || !readme.writable { return !readme.draft.unsaved }
        }
        return true
    }

    /// Revert: replaces README text a save refused with the README as it is saved.
    func revertReadme() {
        guard !busy, let readme else { return }
        busy = true
        Task {
            let discard = await confirm("Discard unsaved README changes?",
                                        "The text that could not be saved will be lost.",
                                        identifier: "readme.revert", approve: "Revert", destructive: true)
            busy = false
            guard discard, self.readme === readme else { return }
            readReadme(of: readme.order) { [self] text in
                showReadme(text, of: readme.order)
            }
        }
    }

    private func readReadme(of order: OrderRow?, then show: @escaping @MainActor (ReadmeText) -> Void) {
        guard let retainedFolder = folder else { return }
        let session = library
        busy = true
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer {
                busy = false
                withExtendedLifetime(retainedFolder) {}
            }
            do {
                let text = try await withTaskCancellationHandler {
                    try await session.readme(of: order, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                show(text)
            } catch {
                if !Task.isCancelled {
                    alert("The README couldn’t be read", "Reload the Package, then try again.",
                          details: error.localizedDescription, identifier: "readme.read")
                }
            }
        }
    }
}
