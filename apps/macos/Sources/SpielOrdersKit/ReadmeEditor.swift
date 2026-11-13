import AppKit
import SwiftUI

/// A plain-text Markdown editor with light highlighting and its own native Undo.
public struct ReadmeEditor: NSViewRepresentable {
    @Binding private var text: String
    private let isEditable: Bool
    private let label: String
    private let identifier: String
    private let height: ClosedRange<CGFloat>?
    private let onEnd: () -> Void

    /// `height` bounds an editor that grows with its text and then scrolls; without it, the
    /// editor fills the space offered. `onEnd` runs when the text stops being edited.
    public init(text: Binding<String>, isEditable: Bool, label: String, identifier: String,
                height: ClosedRange<CGFloat>?, onEnd: @escaping () -> Void) {
        _text = text
        self.isEditable = isEditable
        self.label = label
        self.identifier = identifier
        self.height = height
        self.onEnd = onEnd
    }

    public func makeCoordinator() -> Coordinator { Coordinator(text: $text, onEnd: onEnd) }

    public func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        scroll.borderType = .bezelBorder
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
        editor.textContainerInset = NSSize(width: 4, height: 6)
        editor.setAccessibilityLabel(label)
        editor.setAccessibilityIdentifier(identifier)
        editor.delegate = context.coordinator
        return scroll
    }

    public func updateNSView(_ scroll: NSScrollView, context: Context) {
        let editor = scroll.documentView as! NSTextView
        context.coordinator.text = $text
        context.coordinator.onEnd = onEnd
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

    public func sizeThatFits(_ proposal: ProposedViewSize, nsView scroll: NSScrollView,
                             context: Context) -> CGSize? {
        guard let height, let width = proposal.width, width.isFinite else { return nil }
        let editor = scroll.documentView as! NSTextView
        let fitted = Self.textHeight(text, in: editor, width: width)
        return CGSize(width: width, height: min(max(fitted, height.lowerBound), height.upperBound))
    }

    /// The height `text` takes in `editor` at `width`, including a final empty line.
    @MainActor private static func textHeight(_ text: String, in editor: NSTextView, width: CGFloat) -> CGFloat {
        let inset = editor.textContainerInset
        let padding = editor.textContainer?.lineFragmentPadding ?? 0
        let measured = text.isEmpty || text.hasSuffix("\n") ? text + " " : text
        let font = editor.font ?? .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        let bounds = NSAttributedString(string: measured, attributes: [.font: font]).boundingRect(
            with: NSSize(width: max(width - 2 * (inset.width + padding), 1), height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading])
        return ceil(bounds.height) + 2 * inset.height
    }

    @MainActor public final class Coordinator: NSObject, NSTextViewDelegate {
        fileprivate var text: Binding<String>
        fileprivate var onEnd: () -> Void
        fileprivate weak var editor: NSTextView?
        private let history = UndoManager()
        init(text: Binding<String>, onEnd: @escaping () -> Void) {
            self.text = text
            self.onEnd = onEnd
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

        public func textDidEndEditing(_ notification: Notification) {
            onEnd()
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
