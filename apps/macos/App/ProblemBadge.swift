import AppKit
import SwiftUI

/// How much a mark asks of the owner: a problem to correct, a warning to read, or a
/// consequence to notice.
enum NoticeLevel {
    case problem, warning, note

    var symbol: String {
        switch self {
        case .problem: "exclamationmark.circle.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .note: "info.circle.fill"
        }
    }

    /// The symbol's two layers: its mark, then its fill.
    var colors: [NSColor] {
        switch self {
        case .problem: [.white, .systemRed]
        case .warning: [.black, .systemYellow]
        case .note: [.white, .controlAccentColor]
        }
    }

    var image: NSImage {
        let palette = NSImage.SymbolConfiguration(paletteColors: colors)
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil) ?? NSImage()
        return image.withSymbolConfiguration(palette) ?? image
    }
}

/// A button in a problem's popover: an action that fixes the problem, or Details.
struct NoticeAction: Identifiable {
    let title: String
    let identifier: String
    let perform: @MainActor () -> Void

    var id: String { identifier }

    init(_ title: String, identifier: String, perform: @escaping @MainActor () -> Void) {
        self.title = title
        self.identifier = identifier
        self.perform = perform
    }
}

/// The small symbol that marks a problem where it occurs. Its tooltip states the problem, and
/// clicking it opens a popover with the actions that fix it.
struct ProblemBadge: View {
    let level: NoticeLevel
    let title: String
    let lines: [String]
    var actions: [NoticeAction] = []
    /// What assistive technology calls the symbol, when the title alone would not say.
    var label: String?
    let identifier: String
    @State private var presented = false

    var body: some View {
        Button {
            presented = true
        } label: {
            Image(nsImage: level.image)
        }
        .buttonStyle(.borderless)
        .help(lines.isEmpty ? title : lines.joined(separator: "\n"))
        .accessibilityLabel(label ?? title)
        .accessibilityValue(lines.joined(separator: "\n"))
        .accessibilityIdentifier(identifier)
        .popover(isPresented: $presented, arrowEdge: .bottom) {
            NoticePopover(level: level, title: title, lines: lines, actions: actions) {
                presented = false
            }
        }
    }
}

/// What a problem symbol's popover shows: what is wrong, and the actions that fix it.
struct NoticePopover: View {
    /// More lines than this are left to Details.
    private static let shownLines = 6

    let level: NoticeLevel
    let title: String
    let lines: [String]
    let actions: [NoticeAction]
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label {
                Text(title).font(.headline)
            } icon: {
                Image(nsImage: level.image)
            }
            ForEach(Array(lines.prefix(Self.shownLines).enumerated()), id: \.offset) { _, line in
                Text(line)
                    .lineLimit(5)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            if lines.count > Self.shownLines {
                Text("\(lines.count - Self.shownLines) more in Details")
                    .foregroundStyle(.secondary)
            }
            if !actions.isEmpty {
                HStack {
                    Spacer()
                    ForEach(actions) { action in
                        Button(action.title) { perform(action) }
                            .accessibilityIdentifier(action.identifier)
                    }
                }
            }
        }
        .padding(14)
        .frame(width: 340, alignment: .leading)
    }

    /// The popover closes first, so an action's own sheet or alert can attach to the window.
    private func perform(_ action: NoticeAction) {
        close()
        DispatchQueue.main.async { action.perform() }
    }
}

extension NSView {
    /// Opens a problem's popover beside this view, as a `ProblemBadge` does in SwiftUI.
    @discardableResult
    func showNotice(level: NoticeLevel, title: String, lines: [String], actions: [NoticeAction]) -> NSPopover {
        let popover = NSPopover()
        popover.behavior = .transient
        let content = NoticePopover(level: level, title: title, lines: lines, actions: actions) { [weak popover] in
            popover?.performClose(nil)
        }
        popover.contentViewController = NSHostingController(rootView: content)
        popover.show(relativeTo: bounds, of: self, preferredEdge: .maxY)
        return popover
    }
}
