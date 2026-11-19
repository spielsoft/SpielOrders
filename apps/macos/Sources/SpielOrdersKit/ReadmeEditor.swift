import AppKit
import Markdown
import SwiftUI

/// A plain-text Markdown editor that highlights the Markdown it parses. Its text binding owns Undo, so
/// the app can keep document-scoped changes in the window's shared history after navigation.
public struct ReadmeEditor: NSViewRepresentable {
    @Binding private var text: String
    /// Raw text is a view input, including a changed Unicode spelling that compares equal
    /// as a Swift String. Height follows the same input.
    private let shownText: [UInt8]
    private let isEditable: Bool
    private let label: String
    private let identifier: String
    private let height: ClosedRange<CGFloat>?
    private let onEnd: () -> Void

    /// `height` bounds an editor that grows with its text and then scrolls, and that takes any
    /// further height a frame that may grow offers; without it, the editor fills the space offered. `onEnd` runs when the text stops being edited.
    public init(text: Binding<String>, isEditable: Bool, label: String, identifier: String,
                height: ClosedRange<CGFloat>?, onEnd: @escaping () -> Void) {
        _text = text
        shownText = Array(text.wrappedValue.utf8)
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
        editor.allowsUndo = false
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
        editor.isEditable = isEditable
        editor.setAccessibilityLabel(label)
        editor.setAccessibilityIdentifier(identifier)
        if !editor.string.utf8.elementsEqual(text.utf8) {
            editor.string = text
            context.coordinator.highlight(editor)
        }
    }

    public func sizeThatFits(_ proposal: ProposedViewSize, nsView scroll: NSScrollView,
                             context: Context) -> CGSize? {
        guard let height, let width = proposal.width, width.isFinite else { return nil }
        let editor = scroll.documentView as! NSTextView
        let fitted = Self.textHeight(String(decoding: shownText, as: UTF8.self), in: editor, width: width)
        let grown = min(max(fitted, height.lowerBound), height.upperBound)
        guard let offered = proposal.height, offered.isFinite else { return CGSize(width: width, height: grown) }
        return CGSize(width: width, height: max(grown, offered))
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
        init(text: Binding<String>, onEnd: @escaping () -> Void) {
            self.text = text
            self.onEnd = onEnd
            super.init()
        }

        public func textDidChange(_ notification: Notification) {
            guard let editor = notification.object as? NSTextView else { return }
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
            let text = editor.string
            let font = NSFont.monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
            storage.beginEditing()
            storage.setAttributes([.font: font, .foregroundColor: NSColor.textColor],
                                  range: NSRange(location: 0, length: storage.length))
            var styles = MarkdownStyles(text)
            styles.visit(Document(parsing: text))
            for (range, style) in styles.found {
                switch style {
                case let .traits(traits):
                    storage.enumerateAttribute(.font, in: range) { value, run, _ in
                        let current = value as? NSFont ?? font
                        let descriptor = current.fontDescriptor.withSymbolicTraits(
                            current.fontDescriptor.symbolicTraits.union(traits))
                        storage.addAttribute(.font, value: NSFont(descriptor: descriptor, size: 0) ?? current,
                                             range: run)
                    }
                case let .color(color):
                    storage.addAttribute(.foregroundColor, value: color, range: range)
                case .struck:
                    storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
                }
            }
            storage.endEditing()
        }
    }
}

/// How a Markdown element shows in the editor, its syntax left in place.
private enum MarkdownStyle {
    case traits(NSFontDescriptor.SymbolicTraits)
    case color(NSColor)
    case struck

    /// Code's color: orange where it reads on a dark background, brown on a light one.
    static let code = MarkdownStyle.color(NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? .systemOrange : .systemBrown
    })
}

/// The styled ranges of a parsed Markdown text, outer elements before the elements inside them.
private struct MarkdownStyles: MarkupWalker {
    private let text: String
    /// Where each line starts in the text, as a UTF-8 offset.
    private let lineStarts: [Int]
    private(set) var found: [(NSRange, MarkdownStyle)] = []

    init(_ text: String) {
        self.text = text
        var starts = [0]
        for (offset, byte) in text.utf8.enumerated() where byte == UInt8(ascii: "\n") {
            starts.append(offset + 1)
        }
        lineStarts = starts
    }

    mutating func visitHeading(_ heading: Heading) { mark(heading, .traits(.bold)) }
    mutating func visitStrong(_ strong: Strong) { mark(strong, .traits(.bold)) }
    mutating func visitEmphasis(_ emphasis: Emphasis) { mark(emphasis, .traits(.italic)) }
    mutating func visitStrikethrough(_ strikethrough: Strikethrough) { mark(strikethrough, .struck) }
    mutating func visitInlineCode(_ code: InlineCode) { mark(code, .code) }
    mutating func visitCodeBlock(_ code: CodeBlock) { mark(code, .code) }
    mutating func visitLink(_ link: Markdown.Link) { mark(link, .color(.linkColor)) }
    mutating func visitImage(_ image: Markdown.Image) { mark(image, .color(.linkColor)) }
    mutating func visitBlockQuote(_ quote: BlockQuote) { mark(quote, .color(.secondaryLabelColor)) }
    mutating func visitThematicBreak(_ rule: ThematicBreak) { mark(rule, .color(.secondaryLabelColor)) }
    mutating func visitHTMLBlock(_ html: HTMLBlock) { mark(html, .color(.secondaryLabelColor)) }
    mutating func visitInlineHTML(_ html: InlineHTML) { mark(html, .color(.secondaryLabelColor)) }

    private mutating func mark(_ markup: Markup, _ style: MarkdownStyle) {
        if let range = markup.range, let lower = offset(range.lowerBound), let upper = offset(range.upperBound),
           lower < upper {
            let utf8 = text.utf8
            let span = utf8.index(utf8.startIndex, offsetBy: lower)..<utf8.index(utf8.startIndex, offsetBy: upper)
            found.append((NSRange(span, in: text), style))
        }
        descendInto(markup)
    }

    /// The UTF-8 offset of a parsed location, whose column counts UTF-8 bytes from 1.
    private func offset(_ location: SourceLocation) -> Int? {
        guard lineStarts.indices.contains(location.line - 1) else { return nil }
        let offset = lineStarts[location.line - 1] + location.column - 1
        return min(offset, text.utf8.count)
    }
}
