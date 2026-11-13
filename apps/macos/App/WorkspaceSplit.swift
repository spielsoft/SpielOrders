import AppKit
import SwiftUI

/// A record table beside the selected record's detail. The table takes the spare width, and the
/// detail keeps the width the user last gave it.
struct WorkspaceSplit<Content: View, Detail: View>: NSViewControllerRepresentable {
    let content: Content
    let detail: Detail

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSViewController(context: Context) -> WorkspaceController {
        let list = NSHostingController(rootView: content)
        list.sizingOptions = [.minSize]
        let details = NSHostingController(rootView: detail)
        details.sizingOptions = [.minSize]
        context.coordinator.list = list
        context.coordinator.details = details
        return WorkspaceController(list: list, detail: details)
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

@MainActor final class WorkspaceController: NSSplitViewController {
    private static let listWidth: CGFloat = 320
    private static let detailWidth: CGFloat = 700
    private static let widthKey = "detailWidth"
    private var appeared = false
    private var restoring = false

    init(list: NSViewController, detail: NSViewController) {
        super.init(nibName: nil, bundle: nil)
        let table = NSSplitViewItem(viewController: list)
        table.minimumThickness = Self.listWidth
        let details = NSSplitViewItem(viewController: detail)
        details.minimumThickness = Self.detailWidth
        details.holdingPriority = .defaultLow + 1
        addSplitViewItem(table)
        addSplitViewItem(details)
    }

    required init?(coder: NSCoder) { nil }

    override func viewDidLoad() {
        super.viewDidLoad()
        splitView.dividerStyle = .thin
        splitView.setAccessibilityLabel("List and detail")
        splitView.setAccessibilityIdentifier("workspace.split")
        NotificationCenter.default.addObserver(self, selector: #selector(resized),
                                               name: NSSplitView.didResizeSubviewsNotification, object: splitView)
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        appeared = true
        restoreWidth()
        nameDivider()
    }

    override func viewDidDisappear() {
        appeared = false
        super.viewDidDisappear()
    }

    private var detail: NSView { splitViewItems[1].viewController.view }

    private func restoreWidth() {
        restoring = true
        defer { restoring = false }
        view.layoutSubtreeIfNeeded()
        let stored = UserDefaults.standard.double(forKey: Self.widthKey)
        let wanted = stored.isFinite && stored > 0 ? CGFloat(stored) : Self.detailWidth
        let widest = splitView.bounds.width - Self.listWidth - splitView.dividerThickness
        let width = min(max(wanted, Self.detailWidth), widest)
        guard width >= Self.detailWidth else { return }
        splitView.setPosition(splitView.bounds.width - width - splitView.dividerThickness, ofDividerAt: 0)
        view.layoutSubtreeIfNeeded()
    }

    @objc private func resized(_ notification: Notification) {
        guard appeared, !restoring, view.window != nil else { return }
        let width = detail.frame.width
        if width.isFinite, width >= Self.detailWidth {
            UserDefaults.standard.set(width, forKey: Self.widthKey)
        }
    }

    private func nameDivider() {
        for case let element as any NSAccessibilityProtocol in splitView.accessibilityChildren() ?? [] {
            if element.accessibilityRole() == .splitter {
                element.setAccessibilityLabel("List width")
                element.setAccessibilityIdentifier("list.width")
            }
        }
    }
}
