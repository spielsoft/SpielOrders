import AppKit
import SwiftUI

public struct ReadmeEditor: NSViewRepresentable {
    @Binding private var text: String
    private let isEditable: Bool

    public init(text: Binding<String>, isEditable: Bool) {
        _text = text
        self.isEditable = isEditable
    }

    public func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    public func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        let editor = scroll.documentView as! NSTextView
        editor.isRichText = false
        editor.allowsUndo = true
        editor.isSelectable = true
        editor.isEditable = isEditable
        editor.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        editor.setAccessibilityLabel("Package README")
        editor.delegate = context.coordinator
        return scroll
    }

    public func updateNSView(_ scroll: NSScrollView, context: Context) {
        let editor = scroll.documentView as! NSTextView
        context.coordinator.text = $text
        editor.isEditable = isEditable
        if editor.string != text {
            editor.string = text
            editor.undoManager?.removeAllActions()
        }
    }

    @MainActor public final class Coordinator: NSObject, NSTextViewDelegate {
        fileprivate var text: Binding<String>
        init(text: Binding<String>) { self.text = text }

        public func textDidChange(_ notification: Notification) {
            guard let editor = notification.object as? NSTextView else { return }
            text.wrappedValue = editor.string
        }
    }
}
