import AppKit
import SpielOrdersKit
import SwiftUI
import Testing

@Test @MainActor func readmeRejectsTypingWhileUnavailableAndResumesWhenEditable() throws {
    NSApplication.shared.setActivationPolicy(.prohibited)
    var text = "Original"
    let binding = Binding(get: { text }, set: { text = $0 })
    let host = NSHostingView(rootView: ReadmeEditor(text: binding, isEditable: true, label: "Package README", identifier: "package.readme.editor"))
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 160),
                          styleMask: [], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    defer { window.close() }
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    let editor = try #require(nativeTextView(in: host))
    #expect(editor.accessibilityLabel() == "Package README")
    #expect(editor.accessibilityIdentifier() == "package.readme.editor")
    window.makeFirstResponder(editor)
    editor.insertText(" X", replacementRange: NSRange(location: 8, length: 0))
    #expect(text == "Original X")

    host.rootView = ReadmeEditor(text: binding, isEditable: false, label: "Package README", identifier: "package.readme.editor")
    host.layoutSubtreeIfNeeded()
    let readOnly = try #require(nativeTextView(in: host))
    readOnly.insertText("blocked", replacementRange: NSRange(location: text.utf16.count, length: 0))
    #expect(readOnly.string == "Original X")
    #expect(text == "Original X")
    #expect(readOnly.isSelectable)

    host.rootView = ReadmeEditor(text: binding, isEditable: true, label: "Package README", identifier: "package.readme.editor")
    host.layoutSubtreeIfNeeded()
    let reopened = try #require(nativeTextView(in: host))
    reopened.insertText(" Y", replacementRange: NSRange(location: text.utf16.count, length: 0))
    #expect(text == "Original X Y")
}

@MainActor private func nativeTextView(in view: NSView) -> NSTextView? {
    if let editor = view as? NSTextView { return editor }
    return view.subviews.lazy.compactMap { nativeTextView(in: $0) }.first
}

@Test(arguments: ["Package README", "Order README"]) @MainActor
func markdownEditingKeepsPlainTextAndNativeUndo(label: String) throws {
    NSApplication.shared.setActivationPolicy(.prohibited)
    var text = ""
    let host = NSHostingView(rootView: ReadmeEditor(text: Binding(get: { text }, set: { text = $0 }),
                                                  isEditable: true, label: label, identifier: "markdown.editor"))
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 160),
                          styleMask: [], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    defer { window.close() }
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    let editor = try #require(nativeTextView(in: host))
    window.makeFirstResponder(editor)
    let markdown = "# Heading\n**Bold** and `code` [link](https://example.invalid)\n"
    let undo = try #require(editor.undoManager)
    undo.beginUndoGrouping()
    editor.insertText(markdown, replacementRange: NSRange(location: 0, length: 0))
    undo.endUndoGrouping()
    #expect(text == markdown)
    undo.undo()
    #expect(editor.string == "")
    #expect(text == "")
    undo.redo()
    #expect(text == markdown)
}

@Test @MainActor func aNewEmptyReadmeDoesNotInheritAnotherDocumentsUndo() throws {
    NSApplication.shared.setActivationPolicy(.prohibited)
    var text = ""
    let binding = Binding(get: { text }, set: { text = $0 })
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 160),
                          styleMask: [], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    defer { window.close() }
    let first = NSHostingView(rootView: ReadmeEditor(text: binding, isEditable: true, label: "Order README", identifier: "order.readme.editor"))
    window.contentView = first
    first.layoutSubtreeIfNeeded()
    let previous = try #require(nativeTextView(in: first))
    window.makeFirstResponder(previous)
    previous.insertText("Saved notes", replacementRange: NSRange(location: 0, length: 0))
    #expect(text == "Saved notes")
    text = ""
    let next = NSHostingView(rootView: ReadmeEditor(text: binding, isEditable: true, label: "Order README", identifier: "order.readme.editor"))
    window.contentView = next
    next.layoutSubtreeIfNeeded()
    let current = try #require(nativeTextView(in: next))
    window.makeFirstResponder(current)
    #expect(current.string == "")
    #expect(try #require(current.undoManager).canUndo == false)
}
