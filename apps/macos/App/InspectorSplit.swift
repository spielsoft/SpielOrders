import AppKit
import SwiftUI

struct InspectorSplit<Content: View, Detail: View>: NSViewControllerRepresentable {
    @Binding var visible: Bool
    let content: Content
    let detail: Detail

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSViewController(context: Context) -> ReadInspectorController {
        let controller = ReadInspectorController()
        controller.splitView.isVertical = true
        controller.splitView.dividerStyle = .thin
        let main = NSHostingController(rootView: content)
        main.sizingOptions = [.minSize]
        let inspector = NSHostingController(rootView: detail)
        inspector.sizingOptions = [.minSize]
        context.coordinator.main = main
        context.coordinator.inspector = inspector
        let left = NSSplitViewItem(viewController: main)
        left.minimumThickness = ReadInspectorController.minimumContentWidth
        let right = NSSplitViewItem(viewController: inspector)
        right.minimumThickness = ReadInspectorController.minimumWidth
        right.maximumThickness = ReadInspectorController.maximumWidth
        right.canCollapse = true
        controller.addSplitViewItem(left)
        controller.addSplitViewItem(right)
        controller.splitView.autosaveName = "ReadInspector"
        controller.setVisible(visible)
        let visibility = $visible
        context.coordinator.observation = right.observe(\.isCollapsed, options: [.new]) { _, change in
            guard let collapsed = change.newValue else { return }
            MainActor.assumeIsolated {
                if visibility.wrappedValue == collapsed { visibility.wrappedValue = !collapsed }
            }
        }
        return controller
    }

    func updateNSViewController(_ controller: ReadInspectorController, context: Context) {
        context.coordinator.main?.rootView = content
        context.coordinator.inspector?.rootView = detail
        controller.setVisible(visible)
    }

    @MainActor final class Coordinator {
        var main: NSHostingController<Content>?
        var inspector: NSHostingController<Detail>?
        var observation: NSKeyValueObservation?
    }
}

@MainActor final class ReadInspectorController: NSSplitViewController {
    static let minimumContentWidth: CGFloat = 460
    static let minimumWidth: CGFloat = 280
    static let maximumWidth: CGFloat = 480
    private var appeared = false
    private var restoringWidth = false

    override func viewDidLoad() {
        super.viewDidLoad()
        NotificationCenter.default.addObserver(self, selector: #selector(splitResized(_:)),
                                              name: NSSplitView.didResizeSubviewsNotification, object: splitView)
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        appeared = true
        restoreWidth()
    }

    override func viewDidDisappear() {
        appeared = false
        super.viewDidDisappear()
    }

    func setVisible(_ visible: Bool) {
        let inspector = splitViewItems[1]
        guard inspector.isCollapsed == visible else { return }
        if !visible, appeared { rememberWidth() }
        restoringWidth = true
        defer { restoringWidth = false }
        inspector.isCollapsed = !visible
        if visible, appeared { restoreWidth() }
    }

    private func restoreWidth() {
        guard appeared, !splitViewItems[1].isCollapsed else { return }
        let wasRestoring = restoringWidth
        restoringWidth = true
        defer { restoringWidth = wasRestoring }
        view.layoutSubtreeIfNeeded()
        guard splitView.bounds.width > Self.minimumContentWidth + Self.minimumWidth else { return }
        let stored = UserDefaults.standard.object(forKey: "inspectorWidth") as? Double
        let width = min(Self.maximumWidth, max(Self.minimumWidth, stored.flatMap { $0.isFinite ? CGFloat($0) : nil } ?? Self.maximumWidth))
        splitView.setPosition(splitView.bounds.width - width - splitView.dividerThickness, ofDividerAt: 0)
        view.layoutSubtreeIfNeeded()
    }

    @objc private func splitResized(_ notification: Notification) {
        guard appeared, !restoringWidth, view.window != nil,
              splitViewItems.count == 2, !splitViewItems[1].isCollapsed else { return }
        rememberWidth()
    }

    private func rememberWidth() {
        let width = splitViewItems[1].viewController.view.frame.width
        if width.isFinite, width >= Self.minimumWidth, width <= Self.maximumWidth {
            UserDefaults.standard.set(width, forKey: "inspectorWidth")
        }
    }
}
