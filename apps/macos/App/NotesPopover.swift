import AppKit

/// A row's multiline Notes, edited in a popover beside the row so the row keeps one height.
///
/// Text reaches `onChange` as typed. Tab and Shift-Tab leave the popover in that direction,
/// Escape or a click elsewhere closes it, and `onClose` follows every close.
@MainActor final class NotesPopover: NSObject, NSPopoverDelegate, NSTextViewDelegate {
    /// The row being edited, and the row structure it was opened in.
    let row: Int
    let rowsRevision: Int
    var onChange: (String) -> Void = { _ in }
    var onTab: (_ forward: Bool) -> Void = { _ in }
    var onClose: () -> Void = {}
    private let popover = NSPopover()
    private let text: NSTextView

    init(row: Int, rowsRevision: Int, notes: String, identifier: String) {
        self.row = row
        self.rowsRevision = rowsRevision
        let scroll = NSTextView.scrollableTextView()
        text = scroll.documentView as! NSTextView
        super.init()
        configureText(notes, identifier: identifier)
        let content = NSViewController()
        content.view = Self.container(scroll)
        popover.contentViewController = content
        popover.contentSize = NSSize(width: 360, height: 160)
        popover.behavior = .transient
        popover.delegate = self
    }

    var isEditable: Bool {
        get { text.isEditable }
        set { text.isEditable = newValue }
    }

    func show(relativeTo rect: NSRect, of view: NSView) {
        popover.show(relativeTo: rect, of: view, preferredEdge: .maxY)
        text.window?.makeFirstResponder(text)
        // Notes are added to more often than replaced, so typing continues after them.
        text.setSelectedRange(NSRange(location: (text.string as NSString).length, length: 0))
    }

    func close() {
        popover.close()
    }

    private func configureText(_ notes: String, identifier: String) {
        text.string = notes
        text.isRichText = false
        text.allowsUndo = true
        text.font = .systemFont(ofSize: NSFont.systemFontSize)
        text.textContainerInset = NSSize(width: 4, height: 6)
        // Notes hold quote references and part numbers, which substitutions would alter.
        text.isAutomaticQuoteSubstitutionEnabled = false
        text.isAutomaticDashSubstitutionEnabled = false
        text.isAutomaticTextReplacementEnabled = false
        text.isAutomaticSpellingCorrectionEnabled = false
        text.setAccessibilityLabel("Notes")
        text.setAccessibilityIdentifier(identifier)
        text.delegate = self
    }

    private static func container(_ scroll: NSScrollView) -> NSView {
        let view = NSView()
        scroll.borderType = .bezelBorder
        scroll.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scroll)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 10),
            scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -10),
            scroll.topAnchor.constraint(equalTo: view.topAnchor, constant: 10),
            scroll.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -10),
        ])
        return view
    }

    func textDidChange(_ notification: Notification) {
        onChange(text.string)
    }

    func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(NSResponder.insertTab(_:)):
            onTab(true)
        case #selector(NSResponder.insertBacktab(_:)):
            onTab(false)
        case #selector(NSResponder.cancelOperation(_:)):
            close()
        default:
            return false
        }
        return true
    }

    func popoverDidClose(_ notification: Notification) {
        onClose()
    }
}
