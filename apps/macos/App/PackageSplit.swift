import AppKit
import Observation
import SwiftUI

/// The window's columns: the destination sidebar beside what the open Package shows.
///
/// The sidebar appears only while a Package is open, and hiding it leaves the content as it is.
/// Without a Package the window is a compact welcome, as in Xcode; once a Package opens, the
/// window returns to the size it had.
@MainActor final class PackageSplit: NSSplitViewController {
    private static let welcomeSize = NSSize(width: 720, height: 420)
    private let model: DesktopModel
    /// The window's frame before the welcome view compacted it.
    private var packageFrame: NSRect?
    /// The Package the sidebar was last shown or hidden for.
    private var shownRoot: URL?
    private let destinations: NSSplitViewItem
    private var collapsing: NSKeyValueObservation?

    init(model: DesktopModel) {
        self.model = model
        let list = NSHostingController(rootView: DestinationList(model: model))
        destinations = NSSplitViewItem(sidebarWithViewController: list)
        super.init(nibName: nil, bundle: nil)
        destinations.minimumThickness = 180
        destinations.maximumThickness = 300
        let content = NSHostingController(rootView: PackageWindow(model: model))
        content.sizingOptions = [.minSize]
        addSplitViewItem(destinations)
        addSplitViewItem(NSSplitViewItem(viewController: content))
    }

    required init?(coder: NSCoder) { nil }

    override func viewDidLoad() {
        super.viewDidLoad()
        observeModel()
        collapsing = destinations.observe(\.isCollapsed, options: [.new]) { [weak self] _, change in
            guard let collapsed = change.newValue else { return }
            MainActor.assumeIsolated { self?.remember(collapsed: collapsed) }
        }
    }

    override func viewWillAppear() {
        super.viewWillAppear()
        fitWindow(welcoming: welcoming)
    }

    private var shown: Bool { model.root != nil && model.sidebarVisible }
    private var welcoming: Bool { model.root == nil && !model.starting }

    private func observeModel() {
        withObservationTracking {
            show(shown)
            fitWindow(welcoming: welcoming)
        } onChange: { [weak self] in
            Task { @MainActor in self?.observeModel() }
        }
    }

    private func show(_ shown: Bool) {
        defer { shownRoot = model.root }
        guard destinations.isCollapsed == shown else { return }
        // Only the owner's choice animates; a Package opening or the welcome view's resize sets
        // the sidebar at once.
        let animated = view.window?.isVisible == true && model.root != nil && model.root == shownRoot
            && packageFrame == nil
        let item = animated ? destinations.animator() : destinations
        item.isCollapsed = !shown
    }

    /// The welcome window is compact and keeps its size; full screen keeps the screen's.
    private func fitWindow(welcoming: Bool) {
        guard let window = view.window, !window.styleMask.contains(.fullScreen),
              welcoming == (packageFrame == nil) else { return }
        let animate = window.isVisible
        if welcoming {
            packageFrame = window.frame
            window.styleMask.remove(.resizable)
            let size = window.frameRect(forContentRect: NSRect(origin: .zero, size: Self.welcomeSize)).size
            let frame = NSRect(x: window.frame.midX - size.width / 2, y: window.frame.midY - size.height / 2,
                               width: size.width, height: size.height)
            window.setFrame(frame, display: true, animate: animate)
        } else if let frame = packageFrame {
            packageFrame = nil
            window.styleMask.insert(.resizable)
            window.setFrame(frame, display: true, animate: animate)
        }
    }

    /// A divider drag can collapse the sidebar too; the welcome view's collapse is not a choice.
    private func remember(collapsed: Bool) {
        guard model.root != nil, model.sidebarVisible == collapsed else { return }
        model.sidebarVisible = !collapsed
    }
}
