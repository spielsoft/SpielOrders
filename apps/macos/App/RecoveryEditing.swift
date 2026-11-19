import AppKit
import SpielOrdersKit

extension DesktopModel {
    func showDiagnosis(directory: URL?) {
        if deferNavigation({ [self] in showDiagnosis(directory: directory) }) { return }
        guard !busy, let retainedFolder = folder,
              confirmDiscard(retry: { [self] in showDiagnosis(directory: directory) }) else { return }
        busy = true
        diagnosis = nil
        let cancellation = LibraryCancellation()
        work = OwnedOperation(kind: .read) { [self] in
            defer { busy = false; withExtendedLifetime(retainedFolder) {} }
            do {
                let found = try await withTaskCancellationHandler {
                    try await library.diagnose(directory: directory, cancellation: cancellation)
                } onCancel: { cancellation.request() }
                try Task.checkCancellation()
                discardEditors()
                diagnosticDirectory = directory
                diagnosis = found
                diagnosticWorkspace = true
            } catch {
                if !Task.isCancelled {
                    alert("Couldn’t diagnose", "Reload the Package, then try again.",
                          details: error.localizedDescription, identifier: "diagnosis.read")
                }
            }
        }
    }

    /// Diagnoses where `notices` are located: the one Order they all concern, or otherwise the
    /// Package, whose diagnosis lists each Order its findings concern.
    func diagnose(_ notices: [Notice]) {
        let orders = Set(notices.map(\.order))
        showDiagnosis(directory: orders.count == 1 ? orders.first ?? nil : nil)
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
            } catch {
                alert("Couldn’t open this repair", details: error.localizedDescription, identifier: "repair.open")
            }
        }
    }

    func applyRepair() {
        guard !busy, let diagnosis, let editor = recoveryEditor, let retainedFolder = folder else { return }
        busy = true
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
                guard await dialogs.review("Apply \(editor.action.title)?", explanation: "Review the values and file changes below before choosing Apply Repair.",
                    details: editor.repair.path + "\n\n" + supplied + "\n\n" + plan.operations.joined(separator: "\n"),
                    identifier: "repair.confirmation", approve: "Apply Repair", destructive: false) else { return }
                let result = try await library.commit(plan)
                recoveryEditor = nil
                await refreshRecovery(after: Notice(result.summary, details: result.message),
                                      notices: result.notices, requiresReopen: result.requiresReopen)
            } catch {
                alert("The repair wasn’t applied", "Your input is kept.",
                      details: error.localizedDescription, identifier: "repair.apply")
            }
        }
    }

    /// Diagnoses the scope again after `change` was written, unless it requires Reload.
    func refreshRecovery(after change: Notice, notices: [String], requiresReopen: Bool) async {
        if requiresReopen {
            requireReload(change)
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
            addWarnings(notices, to: .orders)
        } catch {
            diagnosis = nil
            requireReload(Notice(change.summary + " It couldn’t be diagnosed again.",
                                 details: [change.details, error.localizedDescription].joined(separator: "\n\n")))
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
            } catch {
                alert("Couldn’t open the JSON document", details: error.localizedDescription, identifier: "expert.open")
            }
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
                let written = Notice("Raw JSON written.", details: (["Raw JSON written."] + warnings).joined(separator: "\n"))
                do {
                    diagnosis = try await library.revalidate(directory: diagnosticDirectory)
                    await refreshRecovery(after: written, notices: warnings, requiresReopen: false)
                } catch {
                    diagnosis = nil
                    requireReload(Notice(written.summary + " It couldn’t be diagnosed again.",
                                         details: written.details + "\n\n" + error.localizedDescription))
                    orders = []
                    vendors = []
                }
            } catch {
                alert("The raw JSON wasn’t written", "Your text is kept.",
                      details: error.localizedDescription, identifier: "expert.write")
            }
        }
    }
}
