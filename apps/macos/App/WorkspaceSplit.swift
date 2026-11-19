import AppKit
import SwiftUI

/// A record table beside the selected record's detail. The preferred pane keeps the width the
/// owner dragged it to: AppKit restores the divider before the workspace is first drawn, and
/// whatever else changes the window's width, the other pane gives way first, down to its
/// minimum, and takes any spare width.
struct WorkspaceSplit<Content: View, Detail: View>: NSViewControllerRepresentable {
    let content: Content
    let detail: Detail
    var layout = WorkspaceLayout.records

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSViewController(context: Context) -> WorkspaceController {
        let list = NSHostingController(rootView: content)
        list.sizingOptions = [.minSize]
        let details = NSHostingController(rootView: detail)
        details.sizingOptions = [.minSize]
        context.coordinator.list = list
        context.coordinator.details = details
        return WorkspaceController(list: list, detail: details, layout: layout)
    }

    func updateNSViewController(_ controller: WorkspaceController, context: Context) {
        context.coordinator.list?.rootView = content
        context.coordinator.details?.rootView = detail
    }

    @MainActor final class Coordinator {
        var list: NSHostingController<Content>?
        var details: NSHostingController<Detail>?
    }
}

/// Which pane of a workspace keeps its width.
enum WorkspaceLayout {
    /// A table whose columns are the substance, beside a detail that shrinks first and grows
    /// back to its width before the table takes the spare width.
    case records
    /// A short list that keeps its width beside a detail that takes the spare width, as in
    /// Contacts.
    case values

    /// The narrowest table: for records, one that still shows the leading default Orders
    /// columns.
    fileprivate var listMinimum: CGFloat { self == .records ? 400 : 220 }
    /// The widest list, where it keeps its width.
    fileprivate var listMaximum: CGFloat { self == .records ? NSSplitViewItem.unspecifiedDimension : 440 }
    /// The narrowest detail; a narrower form scrolls.
    fileprivate var detailMinimum: CGFloat { self == .records ? 420 : 400 }
    /// Whether the list, rather than the detail, keeps its width.
    fileprivate var holdsList: Bool { self == .values }
    /// Where AppKit remembers the divider; every workspace of a layout shares it.
    fileprivate var autosaveName: String { self == .records ? "Records" : "Values" }
}

@MainActor final class WorkspaceController: NSSplitViewController {
    /// The narrowest table and detail together, which the window's content keeps.
    static let minimumWidth = WorkspaceLayout.records.listMinimum + WorkspaceLayout.records.detailMinimum + 1
    private let layout: WorkspaceLayout

    init(list: NSViewController, detail: NSViewController, layout: WorkspaceLayout) {
        self.layout = layout
        super.init(nibName: nil, bundle: nil)
        let table = NSSplitViewItem(viewController: list)
        table.minimumThickness = layout.listMinimum
        table.maximumThickness = layout.listMaximum
        let details = NSSplitViewItem(viewController: detail)
        details.minimumThickness = layout.detailMinimum
        // The kept pane holds its width as the window resizes; the other gives way.
        table.holdingPriority = layout.holdsList ? .defaultLow + 1 : .defaultLow
        details.holdingPriority = layout.holdsList ? .defaultLow : .defaultLow + 1
        addSplitViewItem(table)
        addSplitViewItem(details)
    }

    required init?(coder: NSCoder) { nil }

    override func viewDidLoad() {
        super.viewDidLoad()
        splitView.dividerStyle = .thin
        splitView.autosaveName = layout.autosaveName
        splitView.setAccessibilityLabel("List and detail")
        splitView.setAccessibilityIdentifier("workspace.split")
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        for case let element as any NSAccessibilityProtocol in splitView.accessibilityChildren() ?? [] {
            if element.accessibilityRole() == .splitter {
                element.setAccessibilityLabel("List width")
                element.setAccessibilityIdentifier("list.width")
            }
        }
    }
}
