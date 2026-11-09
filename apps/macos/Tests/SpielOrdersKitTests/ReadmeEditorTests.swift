import AppKit
import SpielOrdersKit
import SwiftUI
import Testing

@Test @MainActor func readmeRejectsTypingWhileUnavailableAndResumesWhenEditable() throws {
    NSApplication.shared.setActivationPolicy(.prohibited)
    var text = "Original"
    let binding = Binding(get: { text }, set: { text = $0 })
    let host = NSHostingView(rootView: ReadmeEditor(text: binding, isEditable: true))
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 160),
                          styleMask: [], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    defer { window.close() }
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    let editor = try #require(nativeTextView(in: host))
    window.makeFirstResponder(editor)
    editor.insertText(" X", replacementRange: NSRange(location: 8, length: 0))
    #expect(text == "Original X")

    host.rootView = ReadmeEditor(text: binding, isEditable: false)
    host.layoutSubtreeIfNeeded()
    let readOnly = try #require(nativeTextView(in: host))
    readOnly.insertText("blocked", replacementRange: NSRange(location: text.utf16.count, length: 0))
    #expect(readOnly.string == "Original X")
    #expect(text == "Original X")
    #expect(readOnly.isSelectable)

    host.rootView = ReadmeEditor(text: binding, isEditable: true)
    host.layoutSubtreeIfNeeded()
    let reopened = try #require(nativeTextView(in: host))
    reopened.insertText(" Y", replacementRange: NSRange(location: text.utf16.count, length: 0))
    #expect(text == "Original X Y")
}

@MainActor private func nativeTextView(in view: NSView) -> NSTextView? {
    if let editor = view as? NSTextView { return editor }
    return view.subviews.lazy.compactMap { nativeTextView(in: $0) }.first
}
