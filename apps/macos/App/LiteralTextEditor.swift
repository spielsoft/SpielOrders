import AppKit
import SwiftUI

/// Literal managed JSON must not inherit prose editing's smart quotes or substitutions.
struct LiteralTextEditor: NSViewRepresentable {
    @Binding var text: String
    let label: String
    let identifier: String

    func makeCoordinator() -> Coordinator { Coordinator($text) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        let editor = scroll.documentView as! NSTextView
        editor.isRichText = false
        editor.allowsUndo = true
        editor.isAutomaticQuoteSubstitutionEnabled = false
        editor.isAutomaticDashSubstitutionEnabled = false
        editor.isAutomaticTextReplacementEnabled = false
        editor.isAutomaticSpellingCorrectionEnabled = false
        editor.isAutomaticLinkDetectionEnabled = false
        editor.isContinuousSpellCheckingEnabled = false
        editor.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        editor.setAccessibilityLabel(label)
        editor.setAccessibilityIdentifier(identifier)
        editor.delegate = context.coordinator
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        let editor = scroll.documentView as! NSTextView
        context.coordinator.text = $text
        editor.isEditable = context.environment.isEnabled
        if editor.string != text {
            editor.string = text
            context.coordinator.history.removeAllActions()
        }
    }

    @MainActor final class Coordinator: NSObject, NSTextViewDelegate {
        var text: Binding<String>
        let history = UndoManager()
        init(_ text: Binding<String>) { self.text = text }
        func undoManager(for view: NSTextView) -> UndoManager? { history }
        func textDidChange(_ notification: Notification) {
            guard let editor = notification.object as? NSTextView else { return }
            text.wrappedValue = editor.string
        }
    }
}
