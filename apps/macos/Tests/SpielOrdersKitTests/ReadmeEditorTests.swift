import AppKit
import SpielOrdersKit
import SwiftUI
import Testing

@MainActor private func editor(_ text: Binding<String>, isEditable: Bool = true, label: String = "Package README",
                               identifier: String = "package.readme.editor", height: ClosedRange<CGFloat>? = nil,
                               onEnd: @escaping () -> Void = {}) -> ReadmeEditor {
    ReadmeEditor(text: text, isEditable: isEditable, label: label, identifier: identifier, height: height, onEnd: onEnd)
}

@MainActor private func hostingWindow(_ host: NSView) -> NSWindow {
    NSApplication.shared.setActivationPolicy(.prohibited)
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 160),
                          styleMask: [], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    return window
}

@MainActor private func nativeTextView(in view: NSView) -> NSTextView? {
    if let editor = view as? NSTextView { return editor }
    return view.subviews.lazy.compactMap { nativeTextView(in: $0) }.first
}

@Test @MainActor func editorAvailability() throws {
    var text = "Original"
    let binding = Binding(get: { text }, set: { text = $0 })
    let host = NSHostingView(rootView: editor(binding))
    let window = hostingWindow(host)
    defer { window.close() }
    let native = try #require(nativeTextView(in: host))
    #expect(native.accessibilityLabel() == "Package README")
    #expect(native.accessibilityIdentifier() == "package.readme.editor")
    window.makeFirstResponder(native)
    native.insertText(" X", replacementRange: NSRange(location: 8, length: 0))
    #expect(text == "Original X")

    host.rootView = editor(binding, isEditable: false)
    host.layoutSubtreeIfNeeded()
    let readOnly = try #require(nativeTextView(in: host))
    readOnly.insertText("blocked", replacementRange: NSRange(location: text.utf16.count, length: 0))
    #expect(readOnly.string == "Original X")
    #expect(text == "Original X")
    #expect(readOnly.isSelectable)

    host.rootView = editor(binding)
    host.layoutSubtreeIfNeeded()
    let reopened = try #require(nativeTextView(in: host))
    reopened.insertText(" Y", replacementRange: NSRange(location: text.utf16.count, length: 0))
    #expect(text == "Original X Y")
}

// Decision source: Prototype the SpielOrders desktop experience, Order and Vendor editing.
@Test @MainActor func endingEdit() throws {
    var text = ""
    var ended = 0
    let host = NSHostingView(rootView: editor(Binding(get: { text }, set: { text = $0 }), onEnd: { ended += 1 }))
    let window = hostingWindow(host)
    defer { window.close() }
    let native = try #require(nativeTextView(in: host))
    window.makeFirstResponder(native)
    native.insertText("Notes", replacementRange: NSRange(location: 0, length: 0))
    #expect(ended == 0)
    window.makeFirstResponder(nil)
    #expect(ended == 1)
    #expect(text == "Notes")
}

@Test @MainActor func boundedGrowth() throws {
    var text = "One line"
    let binding = Binding(get: { text }, set: { text = $0 })
    let bounds: ClosedRange<CGFloat> = 80...240
    let host = NSHostingView(rootView: editor(binding, height: bounds).frame(width: 320))
    let window = hostingWindow(host)
    defer { window.close() }
    #expect(host.fittingSize.height == bounds.lowerBound)

    text = (1...8).map { "Line \($0)" }.joined(separator: "\n")
    host.rootView = editor(binding, height: bounds).frame(width: 320)
    let grown = host.fittingSize.height
    #expect(grown > bounds.lowerBound)
    #expect(grown < bounds.upperBound)

    text = (1...80).map { "Line \($0)" }.joined(separator: "\n")
    host.rootView = editor(binding, height: bounds).frame(width: 320)
    #expect(host.fittingSize.height == bounds.upperBound)
}

@Test(arguments: ["Package README", "Order README"]) @MainActor
func markdownUndo(label: String) throws {
    var text = ""
    let host = NSHostingView(rootView: editor(Binding(get: { text }, set: { text = $0 }), label: label,
                                              identifier: "markdown.editor"))
    let window = hostingWindow(host)
    defer { window.close() }
    let native = try #require(nativeTextView(in: host))
    window.makeFirstResponder(native)
    let markdown = "# Heading\n**Bold** and `code` [link](https://example.invalid)\n"
    let undo = try #require(native.undoManager)
    undo.beginUndoGrouping()
    native.insertText(markdown, replacementRange: NSRange(location: 0, length: 0))
    undo.endUndoGrouping()
    #expect(text == markdown)
    undo.undo()
    #expect(native.string == "")
    #expect(text == "")
    undo.redo()
    #expect(text == markdown)
}

@Test @MainActor func documentUndo() throws {
    var text = ""
    let binding = Binding(get: { text }, set: { text = $0 })
    let first = NSHostingView(rootView: editor(binding, label: "Order README", identifier: "order.readme.editor"))
    let window = hostingWindow(first)
    defer { window.close() }
    let previous = try #require(nativeTextView(in: first))
    window.makeFirstResponder(previous)
    previous.insertText("Saved notes", replacementRange: NSRange(location: 0, length: 0))
    #expect(text == "Saved notes")
    text = ""
    let next = NSHostingView(rootView: editor(binding, label: "Order README", identifier: "order.readme.editor"))
    window.contentView = next
    next.layoutSubtreeIfNeeded()
    let current = try #require(nativeTextView(in: next))
    window.makeFirstResponder(current)
    #expect(current.string == "")
    #expect(try #require(current.undoManager).canUndo == false)
}
