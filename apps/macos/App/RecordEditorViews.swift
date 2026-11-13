import CSpielOrders
import SwiftUI
import SpielOrdersKit

/// The Order's facts and Items; existing edits commit at field boundaries.
struct OrderEditorView: View {
    @Bindable var model: DesktopModel
    let editor: RecordEditor
    @Environment(\.undoManager) private var undoManager
    @FocusState private var focus: Int32?
    /// The Order Groups of the draft's current Project, which the library reuses for it.
    @State private var groups: [String] = []
    private let prefix = "order.editor"

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            RecordHeading(title: heading, summary: summary, prefix: prefix) {
                RecordStateBadge(model: model, editor: editor, notices: notices)
            }
            if !editor.isNew, let order = model.currentOrder {
                OrderFolder(model: model, order: order, prefix: prefix)
            }
            OrderFacts(control: control)
                .disabled(!model.canType(editor))
            OrderItems(total: total, prefix: prefix) {
                DraftRowsTable(model: model, editor: editor, columns: RowColumn.orderItems,
                               label: "Order Items", identifier: "\(prefix).items", placeholder: "No Items",
                               currency: currency)
                    .disabled(!model.canType(editor))
            } actions: {
                RowButtons(model: model, editor: editor, noun: "Item", prefix: "\(prefix).items")
            }
        }
        .onAppear { editor.undoManager = undoManager }
        .task { focusOnAppear() }
        .onChange(of: focus) { old, new in
            if let new { editor.trackFocus(new, row: -1) }
            if old != nil, old != new { editor.finishField() }
        }
        .onChange(of: model.busy) { _, busy in
            if !busy, !textHoldsFocus { focusRequestedField() }
        }
        .onChange(of: editor.focusRequest?.serial) { _, _ in
            if !model.busy { focusRequestedField() }
        }
        .task(id: groupQuery) {
            groups = await model.groupChoices(in: groupQuery.project)
        }
    }

    /// Asks again when the Project changes, or when a save may have added a group.
    private var groupQuery: GroupQuery {
        GroupQuery(project: editor.field(SPIELORDERS_ORDER_FIELD_PROJECT), revision: model.ordersRevision)
    }

    private struct GroupQuery: Equatable {
        let project: String
        let revision: Int
    }

    private var heading: String {
        editor.isNew ? "New Order" : editor.field(SPIELORDERS_ORDER_FIELD_TITLE)
    }

    /// The library's notices about the Order as it was read.
    private var notices: [String] {
        guard case let .healthy(details) = model.inspection else { return [] }
        return details.notices
    }

    private var summary: String {
        OrderSummaryLine.of(status: editor.field(SPIELORDERS_ORDER_FIELD_STATUS),
                            date: editor.field(SPIELORDERS_ORDER_FIELD_DATE_ORDERED),
                            vendor: editor.field(SPIELORDERS_ORDER_FIELD_VENDOR))
    }

    private var currency: String {
        editor.field(SPIELORDERS_ORDER_FIELD_CURRENCY)
    }

    /// The library answers no Total while a row's numbers do not parse.
    private var total: String {
        editor.total.isEmpty && editor.rowCount > 0 ? "—" : Money.display(editor.total, currency: currency)
    }

    /// A new Order opens on its starting Title, selected so typing replaces it. A selected
    /// Order leaves focus in the list unless Open asked for its first field.
    private func focusOnAppear() {
        if editor.isNew {
            focus = SPIELORDERS_ORDER_FIELD_TITLE
        } else {
            focusRequestedField()
        }
    }

    /// Whether text outside the form, such as the README or the search field, holds keyboard
    /// focus. Any other field editor is the form's own.
    private var textHoldsFocus: Bool {
        guard let text = editor.window?.firstResponder as? NSTextView else { return false }
        return !text.isFieldEditor || text.delegate is NSSearchField
    }

    /// Suggesting fields take focus themselves, from their focus serial.
    private func focusRequestedField() {
        guard let request = editor.focusRequest, request.row < 0,
              OrderFieldControl(request.field) != .suggesting else { return }
        focus = request.field
    }

    private func serial(_ field: Int32) -> Int? {
        guard let request = editor.focusRequest, request.field == field, request.row < 0 else { return nil }
        return request.serial
    }

    @ViewBuilder private func control(_ field: Int32) -> some View {
        switch OrderFieldControl(field) {
        case .text: textField(field)
        case .date: dateField
        case .status: statusPicker
        case .vendor: vendorPicker
        case .suggesting: suggesting(field)
        }
    }

    private func identifier(_ field: Int32) -> String {
        "\(prefix).\(RecordFieldLabel.key(field))"
    }

    private func textField(_ field: Int32) -> some View {
        checked(field) {
            TextField("", text: binding(field), prompt: FieldProblem.prompt(editor, field))
                .labelsHidden()
                .focused($focus, equals: field)
                .accessibilityLabel(RecordFieldLabel.of(field))
                .accessibilityIdentifier(identifier(field))
                .onSubmit { editor.finishField() }
        }
    }

    private func suggesting(_ field: Int32) -> some View {
        checked(field) {
            SuggestingField(label: RecordFieldLabel.of(field), identifier: identifier(field),
                            text: binding(field), choices: choices(field),
                            commit: editor.finishField, focused: { editor.trackFocus(field, row: -1) },
                            focusSerial: serial(field), problem: FieldProblem.blank(editor, field))
                .frame(minWidth: 200, maxWidth: .infinity)
        }
    }

    private func choices(_ field: Int32) -> [String] {
        if field == SPIELORDERS_ORDER_FIELD_ORDER_GROUP {
            return groups
        }
        return model.reusableValues[field]?.map(\.name) ?? []
    }

    private var dateField: some View {
        let field = SPIELORDERS_ORDER_FIELD_DATE_ORDERED
        return checked(field) {
            DateOrderedField(text: binding(field, finishing: true), identifier: identifier(field),
                             problem: FieldProblem.blank(editor, field))
        }
    }

    private var statusPicker: some View {
        let field = SPIELORDERS_ORDER_FIELD_STATUS
        return checked(field) {
            StatusPicker(selection: binding(field, finishing: true), identifier: identifier(field))
                .focused($focus, equals: field)
        }
    }

    private var vendorPicker: some View {
        let field = SPIELORDERS_ORDER_FIELD_VENDOR
        return checked(field) {
            VendorPicker(text: binding(field, finishing: true), choices: model.vendorChoices,
                         identifier: identifier(field), newVendor: model.beginVendorSheet)
                .frame(minWidth: 200, maxWidth: .infinity)
                .focused($focus, equals: field)
        }
    }

    /// A choice finishes its field as it is made; typing finishes when the field is left.
    private func binding(_ field: Int32, finishing: Bool = false) -> Binding<String> {
        Binding(get: { editor.field(field) }, set: {
            editor.set(field, $0)
            if finishing { editor.finishField() }
        })
    }

    private func checked<Content: View>(_ field: Int32, @ViewBuilder _ content: () -> Content) -> some View {
        MarkedField(model: model, editor: editor, field: field, identifier: identifier(field), content: content())
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

    private var heading: String {
        editor.isNew ? "New Vendor" : editor.field(SPIELORDERS_VENDOR_FIELD_NAME)
    }

    /// The library's notices about the Vendor as it was read; a new Vendor in the sheet has none.
    private var notices: [String] {
        inSheet ? [] : model.vendorDetails?.notices ?? []
    }

    /// What the library read about the Vendor being edited in the window.
    private var details: VendorDetails? {
        inSheet || editor.isNew ? nil : model.vendorDetails
    }

    /// A save that settles while the search field holds focus leaves it there.
    private var searchHoldsFocus: Bool {
        (editor.window?.firstResponder as? NSTextView)?.delegate is NSSearchField
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            RecordHeading(title: heading, summary: details.map(VendorSummaryLine.of) ?? "", prefix: prefix) {
                RecordStateBadge(model: model, editor: editor, notices: notices)
                if let details, let id = editor.draft.recordID {
                    VendorVisibility(model: model, id: id, hidden: details.hidden)
                }
            }
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
            .frame(maxWidth: 700, alignment: .leading).disabled(!model.canType(editor))
            Text("Contacts").font(.headline).accessibilityAddTraits(.isHeader)
            DraftRowsTable(model: model, editor: editor, columns: [
                .init(field: SPIELORDERS_CONTACT_FIELD_NAME, width: 180, flexible: true),
                .init(field: SPIELORDERS_CONTACT_FIELD_ROLE, width: 140),
                .init(field: SPIELORDERS_CONTACT_FIELD_PHONE, width: 140),
                .init(field: SPIELORDERS_CONTACT_FIELD_EMAIL, width: 200, flexible: true),
            ], label: "Vendor Contacts", identifier: "\(prefix).contacts", placeholder: "No Contacts")
            .disabled(!model.canType(editor))
            RowButtons(model: model, editor: editor, noun: "Contact", prefix: "\(prefix).contacts")
            if inSheet {
                HStack {
                    Spacer()
                    Button("Cancel") { model.cancelVendorSheet() }
                        .keyboardShortcut(.cancelAction)
                        .accessibilityIdentifier("\(prefix).cancel")
                    Button("Create") { model.saveVendorSheet() }
                        .keyboardShortcut(.defaultAction)
                        .buttonStyle(.borderedProminent)
                        .disabled(model.busy)
                        .accessibilityIdentifier("\(prefix).create")
                }
            }
        }
        .onAppear { editor.undoManager = undoManager }
        .task {
            // A new Vendor opens on its Name; a selected one only when Open asks.
            if editor.isNew {
                focus = SPIELORDERS_VENDOR_FIELD_NAME
            } else if let request = editor.focusRequest, request.row < 0 {
                focus = request.field
            }
        }
        .onChange(of: focus) { old, new in
            if let new { editor.trackFocus(new, row: -1) }
            if old != nil, old != new { editor.finishField() }
        }
        .onChange(of: model.busy) { _, busy in
            guard !busy, !searchHoldsFocus, let request = editor.focusRequest, request.row < 0 else { return }
            focus = request.field
        }
        .onChange(of: editor.focusRequest?.serial) { _, _ in
            if !model.busy, let request = editor.focusRequest, request.row < 0 { focus = request.field }
        }
    }

    private func textField(_ field: Int32, lines: Int) -> some View {
        let identifier = "\(prefix).\(RecordFieldLabel.key(field))"
        let text = Binding(get: { editor.field(field) }, set: { editor.set(field, $0) })
        return MarkedField(model: model, editor: editor, field: field, identifier: identifier,
                           content: TextField("", text: text, prompt: FieldProblem.prompt(editor, field),
                                              axis: lines > 1 ? .vertical : .horizontal)
                               .lineLimit(lines...max(lines, lines * 2))
                               .labelsHidden()
                               .focused($focus, equals: field)
                               .accessibilityLabel(RecordFieldLabel.of(field))
                               .accessibilityIdentifier(identifier))
    }
}

/// The Vendor heading's small control that hides or shows the Vendor, a separate write.
private struct VendorVisibility: View {
    @Bindable var model: DesktopModel
    let id: String
    let hidden: Bool

    var body: some View {
        Button(hidden ? "Show Vendor" : "Hide Vendor", systemImage: hidden ? "eye" : "eye.slash") {
            model.setVendorHidden(!hidden, vendor: id)
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .disabled(model.busy || !model.canBrowseRecords)
        .help(hidden ? "Show Vendor: offer it for new use again" : "Hide Vendor: stop offering it for new use")
        .accessibilityIdentifier("vendor.editor.visibility")
    }
}

/// The Vendor heading's summary: how many Orders use the Vendor, and whether it is hidden.
enum VendorSummaryLine {
    static func of(_ details: VendorDetails) -> String {
        let count = details.orders.count
        let orders = switch count {
        case 0: "No Orders"
        case 1: "1 Order"
        default: "\(count) Orders"
        }
        return details.hidden ? orders + " · Hidden" : orders
    }
}

extension VendorDetails {
    /// Whether the Vendor is no longer offered for new use.
    var hidden: Bool {
        values[SPIELORDERS_VENDOR_FIELD_HIDDEN] == "true"
    }
}

/// A record's name above its form, with its state beside it and a one-line summary beneath.
struct RecordHeading<Badge: View>: View {
    let title: String
    let summary: String
    let prefix: String
    @ViewBuilder let badge: Badge

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(title)
                    .font(.title2)
                    .lineLimit(2)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityIdentifier("\(prefix).heading")
                badge
            }
            if !summary.isEmpty {
                Text(summary)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("\(prefix).summary")
            }
        }
    }
}

/// Cancel and Create for a new record. An existing record saves as each field ends, so it
/// has neither; its heading offers recovery when a save is refused.
struct RecordActions: View {
    @Bindable var model: DesktopModel
    let editor: RecordEditor

    var body: some View {
        if editor.isNew {
            HStack {
                Button("Cancel", action: model.cancelRecord)
                    .disabled(model.busy && !model.committing)
                    .accessibilityIdentifier("editor.cancel")
                Spacer()
                Button("Create") {
                    NSApp.keyWindow?.makeFirstResponder(nil)
                    model.createRecord()
                }
                .buttonStyle(.borderedProminent)
                .disabled(!model.canCreateRecord)
                .accessibilityIdentifier("editor.create")
            }
        }
    }
}

/// A control with the marks of its field: red with a problem symbol while the last refused
/// save located problems there, and a note while a save's consequence there is unread.
struct MarkedField<Content: View>: View {
    @Bindable var model: DesktopModel
    let editor: RecordEditor
    let field: Int32
    /// The control's identifier, which the marks extend.
    let identifier: String
    let content: Content

    var body: some View {
        let problems = editor.problems(at: field).map(\.message)
        HStack(spacing: 4) {
            content.overlay {
                if !problems.isEmpty {
                    RoundedRectangle(cornerRadius: 5)
                        .strokeBorder(.red, lineWidth: 1.5)
                        .allowsHitTesting(false)
                }
            }
            if !problems.isEmpty {
                ProblemBadge(level: .problem, title: RecordFieldLabel.of(field), lines: problems,
                             actions: model.repairs(for: editor), label: "\(RecordFieldLabel.of(field)) problem",
                             identifier: "\(identifier).problem")
            }
            if let consequence = editor.consequences[field] {
                ProblemBadge(level: .note, title: RecordFieldLabel.of(field), lines: [consequence],
                             label: "\(RecordFieldLabel.of(field)) note", identifier: "\(identifier).note")
            }
        }
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

/// Add and Remove for the rows of a record.
struct RowActions: View {
    let noun: String
    let prefix: String
    var canRemove = true
    let add: () -> Void
    let remove: () -> Void

    var body: some View {
        ControlGroup {
            Button("Add \(noun)", systemImage: "plus", action: add)
                .accessibilityIdentifier("\(prefix).add")
            Button("Remove \(noun)", systemImage: "minus", action: remove)
                .disabled(!canRemove)
                .accessibilityIdentifier("\(prefix).remove")
        }
        .labelStyle(.iconOnly)
        .fixedSize()
    }
}

/// Add and Remove over a draft's rows, held while a commit settles.
private struct RowButtons: View {
    @Bindable var model: DesktopModel
    let editor: RecordEditor
    let noun: String
    let prefix: String

    var body: some View {
        RowActions(noun: noun, prefix: prefix, canRemove: !editor.selectedRows.isEmpty,
                   add: { change { editor.addRow() } }, remove: { change { editor.removeSelection() } })
            .disabled(!model.canType(editor))
    }

    private func change(_ apply: @escaping () -> Void) {
        NSApp.keyWindow?.makeFirstResponder(nil)
        if !model.deferNavigation(afterFailure: true, apply) { apply() }
    }
}
