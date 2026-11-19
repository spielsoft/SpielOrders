import CSpielOrders
import Observation
import SpielOrdersKit
import SwiftUI

@MainActor @Observable final class RecoveryEditor {
    let repair: RepairCase
    let action: RepairOption
    let initial: [Int32: String]
    var values: [Int32: String]
    var dirty: Bool { values != initial }

    init(repair: RepairCase, action: RepairOption, values: [Int32: String]) {
        self.repair = repair
        self.action = action
        initial = values
        self.values = values
    }
}

struct RecoveryWorkspace: View {
    @Bindable var model: DesktopModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Button("Back to Orders", systemImage: "chevron.left") { model.leaveDiagnosis() }
                    .disabled(model.busy || !model.canBrowseRecords)
                    .accessibilityIdentifier("diagnosis.back")
                if let directory = model.diagnosticDirectory {
                    Text(directory.lastPathComponent)
                        .font(.title2)
                }
                if model.reloadReason != nil {
                    ReloadBadge(model: model, explanation: "Reload the Package to keep editing.",
                                identifier: "diagnosis.reloadRequired")
                }
                Spacer()
                Button("Revalidate") { model.showDiagnosis(directory: model.diagnosticDirectory) }
                    .disabled(model.busy).accessibilityIdentifier("diagnosis.refresh")
                Button("Show in Finder") { model.reveal(model.diagnosticDirectory ?? model.root!) }
                    .accessibilityIdentifier("diagnosis.reveal")
            }
            Menu("Expert Raw JSON") {
                ForEach(model.diagnosticDirectory == nil ? [ManagedJSON.version, .vendors] : [.order], id: \.self) { artifact in
                    Button("Edit \(artifact.rawValue)…") { model.openExpertDocument(artifact) }
                        .accessibilityIdentifier("expert.open.\(artifact.rawValue)")
                }
            }
            .disabled(model.busy).accessibilityIdentifier("expert.menu")
            if let document = model.expertDocument {
                VStack(alignment: .leading, spacing: 8) {
                    Text(document.location.path).textSelection(.enabled)
                        .accessibilityIdentifier("expert.location")
                    Text("Expert correction: text is written literally. Invalid JSON remains damaged; no facts are inferred.")
                    LiteralTextEditor(text: $model.rawText, label: "Raw managed JSON", identifier: "expert.editor")
                        .disabled(model.busy)
                        .id(ObjectIdentifier(document))
                    Button("Write Raw JSON…") { model.saveExpertDocument() }
                        .disabled(model.busy || !model.rawDirty)
                        .accessibilityIdentifier("expert.write.review")
                }
            }
            if let diagnosis = model.diagnosis {
                Text(verdict(diagnosis))
                    .accessibilityIdentifier("diagnosis.verdict")
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        if !diagnosis.orders.isEmpty {
                            affectedOrders(diagnosis.orders)
                        }
                        ForEach(diagnosis.findings) { finding in
                            VStack(alignment: .leading, spacing: 4) {
                                Text("\(finding.severity): \(finding.code)").font(.headline)
                                Text(finding.location).font(.caption)
                                Text(finding.message)
                                if !finding.evidence.isEmpty {
                                    Text(finding.evidence).font(.system(.caption, design: .monospaced))
                                }
                            }
                            .accessibilityElement(children: .contain)
                            .accessibilityLabel("\(finding.severity): \(finding.code)")
                            .accessibilityIdentifier("diagnosis.finding.\(finding.id)")
                        }
                        Divider()
                        ForEach(diagnosis.cases) { repair in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(repair.code).font(.headline)
                                Text(repair.path).font(.system(.caption, design: .monospaced))
                                ForEach(repair.actions) { action in
                                    Button(action.title + "…") { model.chooseRepair(repair, action: action) }
                                        .disabled(model.busy)
                                        .accessibilityIdentifier("diagnosis.case.\(repair.id).action.\(action.id)")
                                }
                            }
                        }
                        if let editor = model.recoveryEditor, model.expertDocument == nil {
                            repairEditor(editor)
                                .id(ObjectIdentifier(editor))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("diagnosis.workspace")
    }

    /// What the diagnosis found in the scope it covers: one Order, or the Package's own files.
    private func verdict(_ diagnosis: PackageDiagnosis) -> String {
        switch (model.diagnosticDirectory != nil, diagnosis.isValid) {
        case (true, true): "This Order has no integrity errors."
        case (true, false): "This Order is damaged."
        case (false, true): diagnosis.orders.isEmpty ? "The Package has no integrity errors."
            : "The Package’s own files have no integrity errors."
        case (false, false): "The Package’s own files are damaged."
        }
    }

    /// The Orders the Package's findings concern, each opening its own diagnosis.
    private func affectedOrders(_ orders: [URL]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Orders That Need Attention").font(.headline)
            ForEach(orders, id: \.self) { order in
                HStack {
                    Text(order.lastPathComponent)
                    Button("Diagnose Order") { model.showDiagnosis(directory: order) }
                        .disabled(model.busy)
                        .accessibilityIdentifier("diagnosis.order.\(order.lastPathComponent)")
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("diagnosis.orders")
    }

    private func repairEditor(_ editor: RecoveryEditor) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Divider()
            Text(editor.action.title).font(.title3)
            Text(editor.repair.path)
            if editor.action.code == SPIELORDERS_REPAIR_ACTION_ADOPT_NEW_ORDER_IDENTITY {
                Text("This Order gets a new ID. Confirm that the other Order with the same ID is the one that should keep it. No files are combined or discarded.")
            }
            ForEach(editor.action.fields, id: \.self) { field in
                let value = Binding(get: { editor.values[field] ?? "" }, set: { editor.values[field] = $0 })
                if field == SPIELORDERS_TEXT_FIELD_CONTENT {
                    Text("Complete replacement JSON")
                    LiteralTextEditor(text: value, label: "Complete replacement JSON", identifier: "repair.replacement")
                        .frame(minHeight: 240)
                } else {
                    TextField(field == SPIELORDERS_ORDER_FIELD_VENDOR ? "Existing Vendor name" : "New Vendor name", text: value)
                        .accessibilityLabel(field == SPIELORDERS_ORDER_FIELD_VENDOR ? "Existing Vendor" : "New Vendor name")
                        .accessibilityIdentifier("repair.field.\(field)")
                }
            }
            Button("Review Repair…") { model.applyRepair() }
                .accessibilityIdentifier("repair.review")
        }
        .disabled(model.busy)
    }
}
