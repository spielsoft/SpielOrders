import CSpielOrders
import SwiftUI
import SpielOrdersKit

/// The full Order editor: facts, Items, and the Total, saved explicitly.
struct OrderEditorView: View {
    @Bindable var model: DesktopModel
    let editor: RecordEditor
    @Environment(\.undoManager) private var undoManager
    @FocusState private var focus: Int32?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Button("Back to Orders", systemImage: "chevron.left") { model.closeRecordEditor() }
                    .keyboardShortcut(.cancelAction).disabled(model.busy)
                    .accessibilityIdentifier("order.editor.back")
                Text(editor.isNew ? "New Order" : editor.field(SPIELORDERS_ORDER_FIELD_TITLE)).font(.title2)
                    .accessibilityIdentifier("order.editor.heading")
            }
            if editor.refused { ProblemSummary(editor: editor, prefix: "order.editor") }
            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 12, verticalSpacing: 8) {
                GridRow {
                    label(SPIELORDERS_ORDER_FIELD_TITLE)
                    textField(SPIELORDERS_ORDER_FIELD_TITLE).gridCellColumns(3)
                }
                GridRow {
                    label(SPIELORDERS_ORDER_FIELD_DATE_ORDERED)
                    textField(SPIELORDERS_ORDER_FIELD_DATE_ORDERED)
                    label(SPIELORDERS_ORDER_FIELD_STATUS)
                    statusPicker
                }
                GridRow {
                    label(SPIELORDERS_ORDER_FIELD_VENDOR)
                    checked(SPIELORDERS_ORDER_FIELD_VENDOR) {
                        SuggestingField(label: "Vendor", identifier: "order.editor.vendor",
                                        text: binding(SPIELORDERS_ORDER_FIELD_VENDOR),
                                        actions: ["New Vendor…"], choices: model.vendorChoices, restricted: true,
                                        focusSerial: serial(SPIELORDERS_ORDER_FIELD_VENDOR),
                                        problem: FieldProblem.blank(editor, SPIELORDERS_ORDER_FIELD_VENDOR),
                                        onAction: { _ in model.beginVendorSheet() })
                        .frame(minWidth: 200, maxWidth: .infinity)
                    }
                    label(SPIELORDERS_ORDER_FIELD_CURRENCY)
                    textField(SPIELORDERS_ORDER_FIELD_CURRENCY)
                }
                GridRow {
                    label(SPIELORDERS_ORDER_FIELD_ORDERER)
                    suggesting(SPIELORDERS_ORDER_FIELD_ORDERER)
                    label(SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION)
                    suggesting(SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION)
                }
                GridRow {
                    label(SPIELORDERS_ORDER_FIELD_PROJECT)
                    suggesting(SPIELORDERS_ORDER_FIELD_PROJECT)
                    label(SPIELORDERS_ORDER_FIELD_ORDER_GROUP)
                    suggesting(SPIELORDERS_ORDER_FIELD_ORDER_GROUP)
                }
            }
            .frame(maxWidth: 900, alignment: .leading)
            Text("Items").font(.headline).accessibilityAddTraits(.isHeader)
            DraftRowsTable(editor: editor, columns: [
                .init(field: SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, width: 140),
                .init(field: SPIELORDERS_ITEM_FIELD_DESCRIPTION, width: 420, flexible: true),
                .init(field: SPIELORDERS_ITEM_FIELD_QUANTITY, width: 80, numeric: true),
                .init(field: SPIELORDERS_ITEM_FIELD_UNIT_PRICE, width: 100, numeric: true),
                .init(field: SPIELORDERS_ITEM_FIELD_TOTAL, width: 110, editable: false, numeric: true),
            ], label: "Order Items", identifier: "order.editor.items")
            .frame(minHeight: 160, maxHeight: .infinity)
            HStack {
                RowButtons(editor: editor, noun: "Item", prefix: "order.editor.items", disabled: model.busy)
                Spacer()
                Text("Total").font(.headline)
                // The library answers no Total while a row's numbers do not parse.
                Text(editor.total.isEmpty && editor.rowCount > 0 ? "—" : Money.display(editor.total))
                    .monospacedDigit()
                    .accessibleValue("Total", value: editor.total, identifier: "order.editor.total")
            }
        }
        .disabled(model.busy)
        .onAppear { editor.undoManager = undoManager }
        // A new Order opens on its starting Title, selected so typing replaces it.
        .task { if editor.isNew { focus = SPIELORDERS_ORDER_FIELD_TITLE } }
        .onChange(of: editor.focusRequest?.serial) { _, _ in
            if let request = editor.focusRequest, request.row < 0 { focus = request.field }
        }
        .sheet(isPresented: Binding(get: { model.vendorSheet != nil }, set: { if !$0 { model.cancelVendorSheet() } })) {
            if let sheet = model.vendorSheet {
                VendorEditorView(model: model, editor: sheet, inSheet: true)
                    .padding(20)
                    .frame(minWidth: 720, minHeight: 520)
            }
        }
    }

    private func label(_ field: Int32) -> some View {
        Text(RecordFieldLabel.of(field)).gridColumnAlignment(.trailing)
    }

    private func serial(_ field: Int32) -> Int? {
        guard let request = editor.focusRequest, request.field == field, request.row < 0 else { return nil }
        return request.serial
    }

    private func textField(_ field: Int32) -> some View {
        checked(field) {
            TextField("", text: Binding(get: { editor.field(field) }, set: { editor.set(field, $0) }),
                      prompt: FieldProblem.prompt(editor, field))
                .labelsHidden()
                .focused($focus, equals: field)
                .accessibilityLabel(RecordFieldLabel.of(field))
                .accessibilityIdentifier("order.editor.\(RecordFieldLabel.key(field))")
        }
    }

    private func suggesting(_ field: Int32) -> some View {
        checked(field) {
            SuggestingField(label: RecordFieldLabel.of(field), identifier: "order.editor.\(RecordFieldLabel.key(field))",
                            text: binding(field), choices: model.reusableValues[field] ?? [],
                            focusSerial: serial(field), problem: FieldProblem.blank(editor, field))
                .frame(minWidth: 200, maxWidth: .infinity)
        }
    }

    private var statusPicker: some View {
        checked(SPIELORDERS_ORDER_FIELD_STATUS) {
            Picker("Status", selection: Binding(get: { editor.field(SPIELORDERS_ORDER_FIELD_STATUS) },
                                                set: { editor.set(SPIELORDERS_ORDER_FIELD_STATUS, $0) })) {
                ForEach(OrderStatus.all, id: \.name) { status in Text(status.label).tag(status.name) }
            }
            .labelsHidden()
            .fixedSize()
            .focused($focus, equals: SPIELORDERS_ORDER_FIELD_STATUS)
            .accessibilityLabel("Status")
            .accessibilityIdentifier("order.editor.status")
        }
    }

    private func binding(_ field: Int32) -> Binding<String> {
        Binding(get: { editor.field(field) }, set: { editor.set(field, $0) })
    }

    private func checked<Content: View>(_ field: Int32, @ViewBuilder _ content: () -> Content) -> some View {
        FieldProblems(editor: editor, field: field, identifier: "order.editor.\(RecordFieldLabel.key(field)).problem", content: content())
    }
}

/// The full Vendor editor: shared facts and Contacts, in the window or in a sheet over
/// an Order draft.
struct VendorEditorView: View {
    @Bindable var model: DesktopModel
    let editor: RecordEditor
    var inSheet = false
    @Environment(\.undoManager) private var undoManager
    @FocusState private var focus: Int32?

    private var prefix: String { inSheet ? "vendor.sheet" : "vendor.editor" }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                if inSheet {
                    Text("New Vendor").font(.title2).accessibilityIdentifier("\(prefix).heading")
                    Spacer()
                    Button("Cancel") { model.cancelVendorSheet() }
                        .keyboardShortcut(.cancelAction)
                        .accessibilityIdentifier("\(prefix).cancel")
                    Button("Save") { model.saveVendorSheet() }
                        .keyboardShortcut("s", modifiers: .command)
                        .buttonStyle(.borderedProminent)
                        .disabled(model.busy)
                        .accessibilityIdentifier("\(prefix).save")
                } else {
                    Button("Back to Vendors", systemImage: "chevron.left") { model.closeRecordEditor() }
                        .keyboardShortcut(.cancelAction).disabled(model.busy)
                        .accessibilityIdentifier("\(prefix).back")
                    Text(editor.isNew ? "New Vendor" : editor.field(SPIELORDERS_VENDOR_FIELD_NAME)).font(.title2)
                        .accessibilityIdentifier("\(prefix).heading")
                }
            }
            if editor.refused { ProblemSummary(editor: editor, prefix: prefix) }
            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 12, verticalSpacing: 8) {
                ForEach([SPIELORDERS_VENDOR_FIELD_NAME, SPIELORDERS_VENDOR_FIELD_PHONE, SPIELORDERS_VENDOR_FIELD_EMAIL,
                         SPIELORDERS_VENDOR_FIELD_WEBSITE], id: \.self) { field in
                    GridRow {
                        Text(RecordFieldLabel.of(field)).gridColumnAlignment(.trailing)
                        textField(field, lines: 1)
                    }
                }
                ForEach([SPIELORDERS_VENDOR_FIELD_ADDRESS, SPIELORDERS_VENDOR_FIELD_PURCHASING_NOTES], id: \.self) { field in
                    GridRow {
                        Text(RecordFieldLabel.of(field)).gridColumnAlignment(.trailing)
                        textField(field, lines: 3)
                    }
                }
            }
            .frame(maxWidth: 700, alignment: .leading)
            Text("Contacts").font(.headline).accessibilityAddTraits(.isHeader)
            DraftRowsTable(editor: editor, columns: [
                .init(field: SPIELORDERS_CONTACT_FIELD_NAME, width: 180, flexible: true),
                .init(field: SPIELORDERS_CONTACT_FIELD_ROLE, width: 140),
                .init(field: SPIELORDERS_CONTACT_FIELD_PHONE, width: 140),
                .init(field: SPIELORDERS_CONTACT_FIELD_EMAIL, width: 200, flexible: true),
            ], label: "Vendor Contacts", identifier: "\(prefix).contacts")
            .frame(minHeight: 120, maxHeight: .infinity)
            RowButtons(editor: editor, noun: "Contact", prefix: "\(prefix).contacts", disabled: model.busy)
        }
        .disabled(model.busy)
        .onAppear { editor.undoManager = undoManager }
        .task { if editor.isNew { focus = SPIELORDERS_VENDOR_FIELD_NAME } }
        .onChange(of: editor.focusRequest?.serial) { _, _ in
            if let request = editor.focusRequest, request.row < 0 { focus = request.field }
        }
    }

    private func textField(_ field: Int32, lines: Int) -> some View {
        FieldProblems(editor: editor, field: field, identifier: "\(prefix).\(RecordFieldLabel.key(field)).problem",
                      content: TextField("", text: Binding(get: { editor.field(field) }, set: { editor.set(field, $0) }),
                                         prompt: FieldProblem.prompt(editor, field), axis: lines > 1 ? .vertical : .horizontal)
                        .lineLimit(lines...max(lines, lines * 2))
                        .labelsHidden()
                        .focused($focus, equals: field)
                        .accessibilityLabel(RecordFieldLabel.of(field))
                        .accessibilityIdentifier("\(prefix).\(RecordFieldLabel.key(field))"))
    }
}

enum FieldProblem {
    /// What a blank field shows inside itself when the refused save located a problem there.
    @MainActor static func blank(_ editor: RecordEditor, _ field: Int32) -> String? {
        editor.field(field).isEmpty && !editor.problems(at: field).isEmpty ? "Required" : nil
    }

    @MainActor static func prompt(_ editor: RecordEditor, _ field: Int32) -> Text? {
        blank(editor, field).map { Text($0).foregroundStyle(.red) }
    }
}

/// A control outlined in red while the last refused save located problems at its field.
/// A blank field says so inside itself; a filled one carries the library's message beneath.
private struct FieldProblems<Content: View>: View {
    let editor: RecordEditor
    let field: Int32
    let identifier: String
    let content: Content

    var body: some View {
        let problems = editor.problems(at: field)
        let message = problems.map(\.message).joined(separator: "\n")
        VStack(alignment: .leading, spacing: 2) {
            content.overlay {
                if !problems.isEmpty {
                    RoundedRectangle(cornerRadius: 5).strokeBorder(.red, lineWidth: 1.5)
                        .allowsHitTesting(false)
                        .accessibilityElement()
                        .accessibilityLabel(Text("\(RecordFieldLabel.of(field)) problem"))
                        .accessibilityValue(Text(message))
                        .accessibilityIdentifier(identifier)
                }
            }
            if !problems.isEmpty, FieldProblem.blank(editor, field) == nil {
                Text(message).font(.caption).foregroundStyle(.red).accessibilityHidden(true)
            }
        }
    }
}

/// Problems the form cannot show at a field: those about the record as a whole, and those
/// in a row, whose cell shows only red.
private struct ProblemSummary: View {
    let editor: RecordEditor
    let prefix: String

    var body: some View {
        let lines = editor.problems.filter { $0.field < 0 || $0.row >= 0 }.map { problem in
            let place = RecordFieldLabel.of(problem.field)
            let row = problem.row >= 0 ? "Row \(problem.row + 1), " : ""
            return place.isEmpty ? problem.message : "\(row)\(place): \(problem.message)"
        }
        if !lines.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                Label("Couldn’t save", systemImage: "exclamationmark.triangle.fill")
                    .font(.headline).foregroundStyle(.red)
                Text(lines.joined(separator: "\n")).font(.callout)
            }
            .accessibleValue("Couldn’t save", value: lines.joined(separator: "\n"), identifier: "\(prefix).problems")
        }
    }
}

private struct RowButtons: View {
    let editor: RecordEditor
    let noun: String
    let prefix: String
    let disabled: Bool

    var body: some View {
        ControlGroup {
            Button("Add \(noun)", systemImage: "plus") { editor.addRow() }
                .accessibilityIdentifier("\(prefix).add")
            Button("Remove \(noun)", systemImage: "minus") { editor.removeSelectedRows() }
                .disabled(editor.selectedRows.isEmpty)
                .accessibilityIdentifier("\(prefix).remove")
        }
        .labelStyle(.iconOnly)
        .fixedSize()
        .disabled(disabled)
    }
}
