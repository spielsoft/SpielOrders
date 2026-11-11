import AppKit
import SpielOrdersKit

extension DesktopModel {
    func showDiagnosis(directory: URL?) {
        guard !busy, let retainedFolder = folder,
              confirmDiscard(retry: { [self] in showDiagnosis(directory: directory) }) else { return }
        busy = true
        message = ""
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let found = try await withTaskCancellationHandler {
                    try await library.diagnose(directory: directory, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                discardEditors()
                editingOrder = nil
                text = original
                diagnosticDirectory = directory
                diagnosis = found
                diagnosticWorkspace = true
            } catch { if !Task.isCancelled { message = "Couldn’t diagnose this scope.\n" + error.localizedDescription } }
        }
    }

    func leaveDiagnosis() {
        guard !busy, canBrowseRecords, confirmDiscard(retry: { [self] in leaveDiagnosis() }) else { return }
        discardEditors()
        destination = .orders
        refreshLookup()
    }

    func chooseRepair(_ repair: RepairCase, action: RepairOption) {
        guard !busy, let diagnosis, let retainedFolder = folder,
              confirmDiscard(retry: { [self] in chooseRepair(repair, action: action) }) else { return }
        busy = true
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let values = try await library.repairValues(diagnosis, repair: repair, action: action)
                try Task.checkCancellation()
                expertDocument = nil
                recoveryEditor = RecoveryEditor(repair: repair, action: action, values: values)
            } catch { message = "Couldn’t open this repair.\n" + error.localizedDescription }
        }
    }

    func applyRepair() {
        guard !busy, let diagnosis, let editor = recoveryEditor, let retainedFolder = folder else { return }
        busy = true
        message = ""
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let plan = try await withTaskCancellationHandler {
                    try await library.planRepair(diagnosis, repair: editor.repair, action: editor.action,
                                                 values: editor.values, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                let supplied = editor.action.fields.map { editor.values[$0] ?? "" }.joined(separator: "\n")
                guard await confirm("Apply \(editor.action.title)?", editor.repair.path + "\n\n" + supplied
                              + "\n\n" + plan.operations.joined(separator: "\n"), identifier: "repair.confirmation",
                              approve: "Apply Repair") else { return }
                let result = try await library.commit(plan)
                recoveryEditor = nil
                await refreshRecovery(after: result.message, requiresReopen: result.requiresReopen)
            } catch { message = "Repair not completed. Your input is retained.\n" + error.localizedDescription }
        }
    }

    func refreshRecovery(after notice: String, requiresReopen: Bool) async {
        message = notice
        if requiresReopen {
            canBrowseRecords = false
            writable = false
            orders = []
            vendors = []
            diagnosis = nil
            return
        }
        do {
            diagnosis = try await library.diagnose(directory: diagnosticDirectory, cancellation: LibraryCancellation())
            canBrowseRecords = !(await library.diagnosticOnly)
            if canBrowseRecords { await refreshRecords(selectingOrder: selectedOrderID, vendor: selectedVendorID) }
            else { orders = []; vendors = [] }
        } catch {
            diagnosis = nil
            message = notice + "\nRevalidation is unavailable; reopen the Package.\n" + error.localizedDescription
        }
    }

    func openExpertDocument(_ artifact: ManagedJSON) {
        guard !busy, let retainedFolder = folder,
              confirmDiscard(retry: { [self] in openExpertDocument(artifact) }) else { return }
        busy = true
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let document = try await library.expertDocument(directory: diagnosticDirectory, artifact: artifact)
                try Task.checkCancellation()
                recoveryEditor = nil
                expertDocument = document
                rawText = document.text
                rawOriginal = document.text
                message = ""
            } catch { message = "Couldn’t open this managed JSON document.\n" + error.localizedDescription }
        }
    }

    func saveExpertDocument() {
        guard !busy, rawDirty, let document = expertDocument, let retainedFolder = folder else { return }
        busy = true
        let draft = rawText
        work = OwnedOperation(kind: .mutation) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            guard await confirm("Write raw JSON?", document.location.path
                                + "\n\nThis writes your text without interpreting or validating it. Invalid text remains damaged. Revalidation follows the write.",
                                identifier: "expert.write", approve: "Write Raw JSON") else { return }
            do {
                let warnings = try await document.write(draft)
                rawOriginal = draft
                expertDocument = nil
                let notice = (["Raw JSON written."] + warnings).joined(separator: "\n")
                do {
                    diagnosis = try await library.revalidateAfterExpertWrite(directory: diagnosticDirectory)
                    await refreshRecovery(after: notice, requiresReopen: false)
                } catch {
                    diagnosis = nil
                    canBrowseRecords = false
                    writable = false
                    orders = []
                    vendors = []
                    message = notice + "\nRevalidation is unavailable; reopen the Package.\n" + error.localizedDescription
                }
            } catch { message = "Raw JSON was not written. Your draft is retained.\n" + error.localizedDescription }
        }
    }
}
