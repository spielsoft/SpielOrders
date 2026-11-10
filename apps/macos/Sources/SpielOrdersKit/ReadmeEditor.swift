import AppKit
import SwiftUI

public struct ReadmeEditor: NSViewRepresentable {
    @Binding private var text: String
    private let isEditable: Bool
    private let label: String
    private let identifier: String

    public init(text: Binding<String>, isEditable: Bool, label: String, identifier: String) {
        _text = text
        self.isEditable = isEditable
        self.label = label
        self.identifier = identifier
    }

    public func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    public func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        let editor = scroll.documentView as! NSTextView
        editor.isRichText = false
        editor.allowsUndo = true
        editor.isSelectable = true
        editor.isEditable = isEditable
        editor.isAutomaticQuoteSubstitutionEnabled = false
        editor.isAutomaticDashSubstitutionEnabled = false
        editor.isAutomaticTextReplacementEnabled = false
        editor.isAutomaticSpellingCorrectionEnabled = false
        editor.isAutomaticLinkDetectionEnabled = false
        editor.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        editor.setAccessibilityLabel(label)
        editor.setAccessibilityIdentifier(identifier)
        editor.delegate = context.coordinator
        return scroll
    }

    public func updateNSView(_ scroll: NSScrollView, context: Context) {
        let editor = scroll.documentView as! NSTextView
        context.coordinator.text = $text
        context.coordinator.editor = editor
        editor.isEditable = isEditable
        editor.setAccessibilityLabel(label)
        editor.setAccessibilityIdentifier(identifier)
        if editor.string != text {
            editor.string = text
            editor.undoManager?.removeAllActions()
            context.coordinator.highlight(editor)
        }
    }

    @MainActor public final class Coordinator: NSObject, NSTextViewDelegate {
        fileprivate var text: Binding<String>
        fileprivate weak var editor: NSTextView?
        private let history = UndoManager()
        init(text: Binding<String>) {
            self.text = text
            super.init()
            for name in [Notification.Name.NSUndoManagerDidUndoChange, .NSUndoManagerDidRedoChange] {
                NotificationCenter.default.addObserver(self, selector: #selector(undoChanged(_:)), name: name, object: history)
            }
        }

        public func undoManager(for view: NSTextView) -> UndoManager? { history }

        @objc private func undoChanged(_ notification: Notification) {
            guard let editor else { return }
            text.wrappedValue = editor.string
            highlight(editor)
        }

        public func textDidChange(_ notification: Notification) {
            guard let editor = notification.object as? NSTextView else { return }
            self.editor = editor
            text.wrappedValue = editor.string
            highlight(editor)
        }

        fileprivate func highlight(_ editor: NSTextView) {
            guard let storage = editor.textStorage else { return }
            let undo = editor.undoManager
            undo?.disableUndoRegistration()
            defer { undo?.enableUndoRegistration() }
            let range = NSRange(location: 0, length: storage.length)
            let font = NSFont.monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
            let strong = NSFont.monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .semibold)
            storage.beginEditing()
            storage.setAttributes([.font: font, .foregroundColor: NSColor.textColor], range: range)
            let styles: [(String, [NSAttributedString.Key: Any])] = [
                (#"(?m)^#{1,6} .*$"#, [.font: strong]),
                (#"\*\*[^*\n]+\*\*"#, [.font: strong]),
                (#"`[^`\n]+`"#, [.foregroundColor: NSColor.systemOrange]),
                (#"\[[^\]\n]+\]\([^\)\n]+\)"#, [.foregroundColor: NSColor.linkColor]),
            ]
            for (pattern, attributes) in styles {
                guard let expression = try? NSRegularExpression(pattern: pattern) else { continue }
                for match in expression.matches(in: editor.string, range: range) {
                    storage.addAttributes(attributes, range: match.range)
                }
            }
            storage.endEditing()
        }
    }
}
