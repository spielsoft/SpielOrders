import AppKit
import Observation
import SwiftUI

/// The window's columns: the destination sidebar beside what the open Package shows.
///
/// The sidebar appears only while a Package is open, and hiding it leaves the content as it is.
@MainActor final class PackageSplit: NSSplitViewController {
    private let model: DesktopModel
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

    private var shown: Bool { model.root != nil && model.sidebarVisible }

    private func observeModel() {
        withObservationTracking { show(shown) } onChange: { [weak self] in
            Task { @MainActor in self?.observeModel() }
        }
    }

    private func show(_ shown: Bool) {
        guard destinations.isCollapsed == shown else { return }
        let item = view.window?.isVisible == true ? destinations.animator() : destinations
        item.isCollapsed = !shown
    }

    /// A divider drag can collapse the sidebar too; the welcome view's collapse is not a choice.
    private func remember(collapsed: Bool) {
        guard model.root != nil, model.sidebarVisible == collapsed else { return }
        model.sidebarVisible = !collapsed
    }
}
