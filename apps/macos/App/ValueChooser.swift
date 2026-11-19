import AppKit
import CSpielOrders
import SpielOrdersKit
import SwiftUI

/// Chooses one of a field's values. The field completes inline and filters its list as you
/// type; when the text names no value, the list ends with New Orderer “text”… (named for the
/// field), which offers to add it. The field ends on a value it lists, in that value's
/// spelling, or on none, which the library judges; text that names no value is not kept,
/// and the field returns to the draft's value.
struct ValueChooser: NSViewRepresentable {
    let label: String
    let identifier: String
    /// What one value is called in the New item, such as Orderer.
    let kind: String
    /// The draft's value, which the field shows.
    let value: String
    let values: [String]
    /// Values the Package has, including ones not offered for new use, such as hidden Vendors;
    /// the field ends only on one of these, or on none.
    let knownValues: [String]
    /// Ends the field with `text`, typed or chosen.
    let choose: (String) -> Void
    /// Asks to add `text` as a new value.
    let add: (String) -> Void
    let focused: () -> Void
    var focusSerial: Int?
    /// Shown in red inside the empty field after a refused save.
    var problem: String?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSComboBox {
        let box = ChooserBox()
        box.focused = { [weak coordinator = context.coordinator] in coordinator?.parent.focused() }
        box.usesDataSource = true
        box.dataSource = context.coordinator
        box.completes = true
        box.numberOfVisibleItems = 14
        box.delegate = context.coordinator
        box.setAccessibilityLabel(label)
        box.setAccessibilityIdentifier(identifier)
        return box
    }

    func updateNSView(_ box: NSComboBox, context: Context) {
        let coordinator = context.coordinator
        let knownChanged = !coordinator.parent.knownValues.elementsEqual(
            knownValues, by: { $0.utf8.elementsEqual($1.utf8) })
        coordinator.parent = self
        box.isEnabled = context.environment.isEnabled
        if knownChanged || !coordinator.values.elementsEqual(values, by: { $0.utf8.elementsEqual($1.utf8) }) {
            coordinator.values = values
            coordinator.list(box)
        }
        // While editing, only a change made elsewhere, such as Undo, replaces the text.
        if box.currentEditor() == nil || coordinator.shown?.utf8.elementsEqual(value.utf8) != true {
            if !box.stringValue.utf8.elementsEqual(value.utf8) {
                box.stringValue = value
                coordinator.list(box)
            }
            coordinator.shown = value
        }
        if coordinator.problem != problem {
            coordinator.problem = problem
            let attributes: [NSAttributedString.Key: Any] = [.foregroundColor: NSColor.problemText,
                                                             .font: box.font ?? .systemFont(ofSize: 0)]
            box.placeholderAttributedString = problem.map { NSAttributedString(string: $0, attributes: attributes) }
        }
        if box.isEnabled, let focusSerial, focusSerial != coordinator.focusSerial {
            coordinator.focusSerial = focusSerial
            DispatchQueue.main.async { [weak box, weak coordinator] in
                guard let box, let coordinator, box.isEnabled,
                      coordinator.parent.focusSerial == focusSerial else { return }
                if box.currentEditor() == nil { box.window?.makeFirstResponder(box) }
            }
        }
    }

    /// One row of the list: a value, or the New item for text that names none.
    enum Item {
        case value(String)
        case new(String)
    }

    @MainActor final class Coordinator: NSObject, NSComboBoxDelegate, NSComboBoxDataSource {
        var parent: ValueChooser
        var values: [String] = []
        /// What the list shows for the text being typed.
        private(set) var items: [Item] = []
        var focusSerial: Int?
        /// The draft's value as this field last showed it.
        var shown: String?
        var problem: String?

        init(_ parent: ValueChooser) {
            self.parent = parent
        }

        /// Lists the values matching the text typed in `box`, all of them while it shows the
        /// draft's value, then the New item when no value is named so.
        func list(_ box: NSComboBox) {
            let typed = box.stringValue.utf8.elementsEqual(parent.value.utf8) ? "" : Self.typed(in: box)
            let text = box.stringValue.trimmingCharacters(in: .whitespaces)
            let matches = typed.isEmpty ? values : values.filter { $0.localizedCaseInsensitiveContains(typed) }
            let named = parent.knownValues.contains { valueMatches($0, text) }
            items = matches.map(Item.value) + (text.isEmpty || named ? [] : [.new(text)])
            box.noteNumberOfItemsChanged()
            box.reloadData()
        }

        /// The text typed so far, without the completion selected after it.
        private static func typed(in box: NSComboBox) -> String {
            let text = box.stringValue as NSString
            guard let editor = box.currentEditor() else { return text as String }
            let selection = editor.selectedRange
            guard selection.length > 0, NSMaxRange(selection) == text.length else { return text as String }
            return text.substring(to: selection.location)
        }

        private func title(_ item: Item) -> String {
            switch item {
            case let .value(value): value
            case let .new(text): "New \(parent.kind) “\(text)”…"
            }
        }

        private func item(at index: Int) -> Item? {
            items.indices.contains(index) ? items[index] : nil
        }

        func numberOfItems(in comboBox: NSComboBox) -> Int { items.count }

        func comboBox(_ comboBox: NSComboBox, objectValueForItemAt index: Int) -> Any? {
            item(at: index).map(title)
        }

        func comboBox(_ comboBox: NSComboBox, completedString string: String) -> String? {
            values.first { $0.range(of: string, options: [.caseInsensitive, .anchored]) != nil }
        }

        func comboBox(_ comboBox: NSComboBox, indexOfItemWithStringValue string: String) -> Int {
            items.firstIndex { title($0).utf8.elementsEqual(string.utf8) } ?? NSNotFound
        }

        func controlTextDidBeginEditing(_ notification: Notification) { parent.focused() }

        func controlTextDidChange(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            // Completion settles after this notification; list what it leaves.
            DispatchQueue.main.async { [weak self, weak box] in
                guard let self, let box else { return }
                list(box)
                if !items.isEmpty, box.cell?.isAccessibilityExpanded() == false {
                    box.cell?.setAccessibilityExpanded(true)
                }
            }
        }

        func comboBoxSelectionDidChange(_ notification: Notification) { parent.focused() }

        /// A click or Return on a row chooses it at once; Escape leaves the text as typed.
        func comboBoxWillDismiss(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            let escaped = NSApp.currentEvent?.type == .keyDown && NSApp.currentEvent?.keyCode == 53
            DispatchQueue.main.async { [weak self, weak box] in
                guard let self, let box, let chosen = item(at: box.indexOfSelectedItem) else { return }
                if escaped, case .new = chosen {
                    restore(box)
                } else {
                    finish(box, with: chosen)
                }
            }
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            guard let box = notification.object as? NSComboBox else { return }
            finish(box, with: item(at: box.indexOfSelectedItem) ?? .value(box.stringValue))
        }

        private func finish(_ box: NSComboBox, with item: Item) {
            switch item {
            case let .value(text):
                guard let value = chosen(text) else {
                    NSSound.beep()
                    restore(box)
                    return
                }
                shown = value
                parent.choose(value)
            case let .new(text):
                // The field shows the draft's value until the new one is added.
                restore(box)
                parent.add(text)
            }
        }

        /// The value `text` chooses: the draft's own value, none, or the Package value it
        /// names, as the library compares them; nil when it names no value.
        private func chosen(_ text: String) -> String? {
            if text.utf8.elementsEqual(parent.value.utf8) { return text }
            if text.trimmingCharacters(in: .whitespaces).isEmpty { return "" }
            return parent.knownValues.first { valueMatches($0, text) }
        }

        private func restore(_ box: NSComboBox) {
            if box.indexOfSelectedItem >= 0 { box.deselectItem(at: box.indexOfSelectedItem) }
            box.stringValue = parent.value
            shown = parent.value
            list(box)
        }
    }
}

@MainActor private final class ChooserBox: NSComboBox {
    var focused: (() -> Void)?

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        if accepted { focused?() }
        return accepted
    }
}

extension ValueChooser {
    /// What the New item calls one value of `field`.
    static func kind(of field: Int32) -> String {
        switch field {
        case SPIELORDERS_ORDER_FIELD_VENDOR: "Vendor"
        case SPIELORDERS_ORDER_FIELD_ORDERER: "Orderer"
        case SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION: "Institution"
        case SPIELORDERS_ORDER_FIELD_PROJECT: "Project"
        default: "Order Group"
        }
    }
}
