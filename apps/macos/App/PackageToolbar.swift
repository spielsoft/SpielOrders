import AppKit
import Observation

/// AppKit owns toolbar placement and keyboard traversal. The sidebar control sits over the
/// destination sidebar and is hidden with it while no Package is open; progress shows at the
/// trailing end while the Package's work runs.
@MainActor final class PackageToolbar: NSObject, NSToolbarDelegate {
    private let model: DesktopModel
    private let sidebar = NSToolbarItem(itemIdentifier: .sidebar)
    private let progress = NSToolbarItem(itemIdentifier: .progress)
    private let spinner = NSProgressIndicator()

    init(model: DesktopModel, window: NSWindow) {
        self.model = model
        super.init()
        sidebar.target = self
        sidebar.action = #selector(toggleSidebar)
        sidebar.autovalidates = false
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.setAccessibilityLabel("Working")
        progress.view = spinner
        progress.label = "Working"
        let toolbar = NSToolbar(identifier: "Package")
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        toolbar.allowsUserCustomization = false
        window.toolbar = toolbar
        window.toolbarStyle = .unified
        observeChanges()
    }

    private func observeChanges() {
        withObservationTracking { update() } onChange: { [weak self] in
            Task { @MainActor in self?.observeChanges() }
        }
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.sidebar, .sidebarTrackingSeparator, .flexibleSpace, .progress]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier identifier: NSToolbarItem.Identifier,
                 willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        switch identifier {
        case .sidebar: sidebar
        case .progress: progress
        default: nil
        }
    }

    private func update() {
        let label = model.sidebarVisible ? "Hide Sidebar" : "Show Sidebar"
        sidebar.label = label
        sidebar.toolTip = label
        sidebar.image = NSImage(systemSymbolName: "sidebar.left", accessibilityDescription: label)
        sidebar.isHidden = model.root == nil
        progress.isHidden = !working
        spinner.isHidden = !working
        if working {
            spinner.startAnimation(nil)
        } else {
            spinner.stopAnimation(nil)
        }
    }

    /// A selection keeps its detail in place while it is read, search shows progress beside its
    /// field, and opening shows it in the window, so none of them shows progress here.
    private var working: Bool {
        model.busy && model.selectionRead == nil && !model.refreshingLookup && !model.starting
            && model.root != nil
    }

    @objc private func toggleSidebar() {
        model.sidebarVisible.toggle()
    }
}

private extension NSToolbarItem.Identifier {
    static let sidebar = Self("sidebar.toggle")
    static let progress = Self("progress")
}
