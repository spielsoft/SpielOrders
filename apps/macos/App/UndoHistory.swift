import AppKit
import SpielOrdersKit

/// One edit to an existing Order or Vendor in the window's Undo history: the record, and the
/// values Undo or Redo restores. It names the record by identity rather than by editor, so it
/// outlives selecting another record.
@MainActor final class RecordChange {
    let kind: RecordDraft.Kind
    let id: String
    let values: RecordValues
    /// The values the record held once the change was saved; nil until it is. Undo applies
    /// only to a record that still holds them, so it never reverses a change made elsewhere.
    var committed: RecordValues?

    init(kind: RecordDraft.Kind, id: String, values: RecordValues) {
        self.kind = kind
        self.id = id
        self.values = values
    }

    var actionName: String { kind == .order ? "Edit Order" : "Edit Vendor" }
    private var noun: String { kind == .order ? "Order" : "Vendor" }

    /// Why Undo or Redo of this change cannot apply, for its alert.
    func refusal(_ reason: Refusal) -> String {
        switch reason {
        case .unshown: "The \(noun) couldn’t be shown. It may have been removed. Nothing was changed."
        case .changed: "The \(noun) changed after this edit. Nothing was changed."
        case .unsaved: "This edit was never saved, so there is nothing to reverse."
        }
    }

    enum Refusal { case unshown, changed, unsaved }
}

/// One README typing burst, identified by its Order or by the Package document.
@MainActor final class ReadmeChange {
    let orderID: String?
    let text: String
    var committed: String?

    init(orderID: String?, text: String) {
        self.orderID = orderID
        self.text = text
    }

    var actionName: String { orderID == nil ? "Edit Package README" : "Edit Order README" }
}

extension UndoManager.UserInfoKey {
    /// The `RecordChange` an Undo or Redo action applies.
    static let recordChange = UndoManager.UserInfoKey("recordChange")
    static let readmeChange = UndoManager.UserInfoKey("readmeChange")
}

/// The window keeps one Undo history, `undoManager`, which survives selecting another record.
/// Record edits are registered with the model as their target, a removed Order's restore with
/// that Order (see `RemovedOrder`), and a new record's staged rows with its editor. AppKit
/// keeps typing in the history of the field being edited, which reaches this one once it has
/// nothing of its own. Switching or closing the Package clears it; Reload forgets record and
/// README edits, while removal and added-value actions keep their own library guards.
extension DesktopModel {
    /// The history Edit → Undo, or Redo, acts on in `window`. A field being edited reverses
    /// its own typing first. Text the window sets in that field, as Undo of a record does,
    /// enters the field's history too; AppKit names each edit made by typing, so an unnamed
    /// action there is passed over for the window's history. AppKit redoes the window's
    /// history before a field's typing, so Redo reaches the typing only when the window has
    /// nothing to redo; input to a record ends the window's Redo, as any new edit does.
    /// Other windows, and text views with a history of their own, keep theirs.
    func editHistory(in window: NSWindow?, redo: Bool) -> UndoManager? {
        guard let window else { return nil }
        let responder = window.firstResponder?.undoManager ?? window.undoManager
        guard window.undoManager === undoManager, let field = editedField(in: window),
              let history = field.undoManager, history !== undoManager else { return responder }
        let typing = redo ? (undoManager.canRedo ? "" : history.redoActionName) : history.undoActionName
        return typing.isEmpty ? undoManager : history
    }

    /// Edit → Undo in `window`.
    func undo(in window: NSWindow) {
        let history = editHistory(in: window, redo: false)
        if history === undoManager {
            undoEdit()
        } else if let field = editedField(in: window), let history, history === field.undoManager {
            history.undo()
            announce(field)
        } else {
            NSApp.sendAction(Selector(("undo:")), to: nil, from: nil)
        }
    }

    /// Edit → Redo in `window`.
    func redo(in window: NSWindow) {
        let history = editHistory(in: window, redo: true)
        if history === undoManager {
            redoEdit()
        } else if let field = editedField(in: window), let history, history === field.undoManager {
            history.redo()
            announce(field)
        } else {
            NSApp.sendAction(Selector(("redo:")), to: nil, from: nil)
        }
    }

    /// The field editor of the text field being edited in `window`.
    private func editedField(in window: NSWindow) -> NSTextView? {
        guard let field = window.firstResponder as? NSTextView, field.isFieldEditor else { return nil }
        return field
    }

    /// Announces the text that Undo or Redo of typing left in `field`. A SwiftUI text field
    /// ignores a change made while its history is undoing, so its binding would keep text the
    /// field no longer shows, and ending the field would save it.
    private func announce(_ field: NSTextView) {
        field.didChangeText()
    }

    /// Makes `change` the most recent action, or, while undoing, the one Redo repeats.
    func register(_ change: RecordChange) {
        undoManager.registerUndo(withTarget: self) { $0.apply(change) }
        undoManager.setActionName(change.actionName)
        undoManager.setActionUserInfoValue(change, forKey: .recordChange)
    }

    func register(_ change: ReadmeChange) {
        undoManager.registerUndo(withTarget: self) { $0.apply(change) }
        undoManager.setActionName(change.actionName)
        undoManager.setActionUserInfoValue(change, forKey: .readmeChange)
    }

    /// Undo of the window's history. An edit to a record the window doesn't show selects that
    /// record first, so it is reversed where it can be seen.
    private func undoEdit() {
        if let change = undoManager.undoActionUserInfoValue(forKey: .readmeChange) as? ReadmeChange {
            whenShown(change, verb: "Undo") { [self] in undoManager.undo() }
            return
        }
        let change = undoManager.undoActionUserInfoValue(forKey: .recordChange) as? RecordChange
        whenShown(change, verb: "Undo") { [self] in undoManager.undo() }
    }

    /// Redo of the window's history, which shows the record it changes as Undo does.
    private func redoEdit() {
        if let change = undoManager.redoActionUserInfoValue(forKey: .readmeChange) as? ReadmeChange {
            whenShown(change, verb: "Redo") { [self] in undoManager.redo() }
            return
        }
        let change = undoManager.redoActionUserInfoValue(forKey: .recordChange) as? RecordChange
        whenShown(change, verb: "Redo") { [self] in undoManager.redo() }
    }

    private func whenShown(_ change: RecordChange?, verb: String, then perform: @escaping @MainActor () -> Void) {
        guard let change, editor(showing: change) == nil else {
            perform()
            return
        }
        guard !dirty, vendorSheet == nil, canBrowseRecords else {
            alert("Couldn’t \(verb.lowercased()) “\(change.actionName)”", "Finish the current change, then try again.",
                  identifier: "edit.history")
            return
        }
        // A record that can't be shown leaves the change to explain why it can't apply.
        showRecord(change.kind, id: change.id, then: perform)
    }

    private func whenShown(_ change: ReadmeChange, verb: String, then perform: @escaping @MainActor () -> Void) {
        if showing(change) != nil {
            perform()
            return
        }
        guard !dirty, vendorSheet == nil, canBrowseRecords else {
            alert("Couldn’t \(verb.lowercased()) “\(change.actionName)”", "Finish the current change, then try again.",
                  identifier: "edit.history")
            return
        }
        showReadme(change, then: perform)
    }

    private func showing(_ change: ReadmeChange) -> ReadmeSession? {
        guard let readme, readme.order?.id == change.orderID else { return nil }
        return readme
    }

    private func apply(_ change: ReadmeChange) {
        let verb = undoManager.isUndoing ? "Undo" : "Redo"
        guard let readme = showing(change), readme.writable else {
            alert("Couldn’t \(verb.lowercased()) “\(change.actionName)”", "The README couldn’t be opened. Nothing was changed.",
                  identifier: "edit.history")
            return
        }
        let current = readme.draft.text
        guard change.committed.map({ $0.utf8.elementsEqual(current.utf8) }) ?? true else {
            alert("Couldn’t \(verb.lowercased()) “\(change.actionName)”", "The README changed after this edit. Nothing was changed.",
                  identifier: "edit.history")
            return
        }
        guard change.committed != nil || readme.draft.unsaved else {
            alert("Couldn’t \(verb.lowercased()) “\(change.actionName)”", "This edit was never saved, so there is nothing to reverse.",
                  identifier: "edit.history")
            return
        }
        let inverse = ReadmeChange(orderID: change.orderID, text: current)
        register(inverse)
        readme.restore(change.text, completing: inverse)
    }

    /// Undo or Redo of `change`, in the editor showing its record. The way back is registered
    /// at once, as it must be while undoing; a change that can't apply is dropped, saying why.
    private func apply(_ change: RecordChange) {
        let verb = undoManager.isUndoing ? "Undo" : "Redo"
        guard let editor = editor(showing: change) else {
            return refuse(change, .unshown, verb: verb)
        }
        let current = RecordValues(editor)
        if let committed = change.committed, committed != current {
            return refuse(change, .changed, verb: verb)
        }
        if change.committed == nil, !editor.dirty {
            return refuse(change, .unsaved, verb: verb)
        }
        let inverse = RecordChange(kind: change.kind, id: change.id, values: current)
        register(inverse)
        editor.restore(change.values, completing: inverse)
    }

    private func refuse(_ change: RecordChange, _ reason: RecordChange.Refusal, verb: String) {
        alert("Couldn’t \(verb.lowercased()) “\(change.actionName)”", change.refusal(reason), identifier: "edit.history")
    }

    /// The editor of the existing record `change` edits, when the window shows it.
    private func editor(showing change: RecordChange) -> RecordEditor? {
        let editor = change.kind == .order ? orderEditor : vendorEditor
        guard let editor, !editor.isNew, editor.draft.recordID == change.id else { return nil }
        return editor
    }

    /// Reload keeps the restores of removed Orders, which the library can still apply, and
    /// forgets edits, which were made to records as they were read before.
    func forgetEdits() {
        undoManager.removeAllActions(withTarget: self)
        if let fieldEditor = dialogs.window?.fieldEditor(false, for: nil) {
            undoManager.removeAllActions(withTarget: fieldEditor)
        }
    }

    /// A change that Undo can't reverse, such as Move to Trash, ends what Redo could repeat,
    /// as any new action does. Registering an action clears Redo; this one is then removed.
    func endRedo() {
        guard undoManager.canRedo, undoManager.groupingLevel == 0 else { return }
        let marker = NSObject()
        undoManager.groupsByEvent = false
        undoManager.beginUndoGrouping()
        undoManager.registerUndo(withTarget: marker) { _ in }
        undoManager.endUndoGrouping()
        undoManager.groupsByEvent = true
        undoManager.removeAllActions(withTarget: marker)
    }
}
