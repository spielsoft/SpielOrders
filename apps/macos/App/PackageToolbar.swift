import AppKit
import Observation

/// Standard window chrome; AppKit owns placement, overflow, and keyboard traversal.
@MainActor final class PackageToolbar: NSObject, NSToolbarDelegate, NSMenuItemValidation {
    private let model: DesktopModel
    private let package = NSMenuToolbarItem(itemIdentifier: .package)
    private let destinations = NSSegmentedControl(labels: DesktopModel.Destination.allCases.map(\.rawValue),
                                                  trackingMode: .selectOne, target: nil, action: nil)
    private var items: [NSToolbarItem.Identifier: NSToolbarItem] = [:]

    init(model: DesktopModel, window: NSWindow) {
        self.model = model
        super.init()
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
        [.package, .destinations, .flexibleSpace, .newOrder, .saveRecord, .inspector]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier identifier: NSToolbarItem.Identifier,
                 willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        let item: NSToolbarItem
        switch identifier {
        case .package:
            let menu = NSMenu(title: "Package")
            for (title, action) in [("Open Package…", #selector(openPackage)),
                                    ("Initialize Package…", #selector(initializePackage)),
                                    ("Reopen Package", #selector(reopenPackage)),
                                    ("Diagnose Package", #selector(diagnosePackage)),
                                    ("Agent Ready…", #selector(agentReady)),
                                    ("Reveal Package in Finder", #selector(revealPackage))] {
                let entry = NSMenuItem(title: title, action: action, keyEquivalent: "")
                entry.target = self
                menu.addItem(entry)
            }
            package.menu = menu
            package.image = NSImage(systemSymbolName: "shippingbox", accessibilityDescription: "Package")
            package.autovalidates = false
            item = package
        case .destinations:
            item = NSToolbarItem(itemIdentifier: identifier)
            item.label = "Destination"
            destinations.target = self
            destinations.action = #selector(navigate)
            destinations.setAccessibilityLabel("Destination")
            destinations.setAccessibilityIdentifier("navigation.destinations")
            item.view = destinations
        case .newOrder:
            item = command(identifier, "New Order", "plus", #selector(newOrder))
        case .saveRecord:
            item = command(identifier, "Save", "square.and.arrow.down", #selector(save))
        case .inspector:
            item = command(identifier, "Inspector", "sidebar.trailing", #selector(toggleInspector))
        default: return nil
        }
        items[identifier] = item
        return item
    }

    private func command(_ identifier: NSToolbarItem.Identifier, _ title: String, _ symbol: String,
                         _ action: Selector) -> NSToolbarItem {
        let item = NSToolbarItem(itemIdentifier: identifier)
        item.label = title
        item.toolTip = title
        item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: title)
        item.target = self
        item.action = action
        item.autovalidates = false
        return item
    }

    func update() {
        package.label = model.root?.lastPathComponent ?? "Package"
        package.title = package.label
        package.toolTip = model.root?.path ?? "Open or initialize a Package"
        package.isEnabled = !model.busy && model.vendorSheet == nil
        destinations.isEnabled = !model.busy && model.vendorSheet == nil && model.canBrowseRecords
        destinations.selectedSegment = DesktopModel.Destination.allCases.firstIndex(of: model.destination) ?? -1
        items[.newOrder]?.isEnabled = !model.busy && model.vendorSheet == nil && model.canBrowseRecords
        items[.saveRecord]?.isEnabled = model.vendorSheet == nil ? model.canSave : !model.busy
        items[.inspector]?.isEnabled = model.root != nil && !model.editingReadme && model.recordEditor == nil && !model.diagnosticWorkspace
        items[.inspector]?.toolTip = model.inspectorVisible ? "Hide Inspector" : "Show Inspector"
    }

    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        guard !model.busy, model.vendorSheet == nil else { return false }
        switch menuItem.action {
        case #selector(agentReady): return model.root != nil && model.canBrowseRecords
        case #selector(reopenPackage), #selector(diagnosePackage), #selector(revealPackage): return model.root != nil
        default: return true
        }
    }

    @objc private func openPackage() { model.choosePackage() }
    @objc private func initializePackage() { model.chooseInitialization() }
    @objc private func reopenPackage() { model.reopenPackage() }
    @objc private func diagnosePackage() { model.showDiagnosis(directory: nil) }
    @objc private func agentReady() { model.makeAgentReady() }
    @objc private func revealPackage() { model.reveal() }
    @objc private func newOrder() { model.newOrder() }
    @objc private func save() { if model.vendorSheet != nil { model.saveVendorSheet() } else { model.save() } }
    @objc private func toggleInspector() { model.inspectorVisible.toggle() }
    @objc private func navigate() {
        guard DesktopModel.Destination.allCases.indices.contains(destinations.selectedSegment) else { return }
        model.navigate(to: DesktopModel.Destination.allCases[destinations.selectedSegment])
        update()
    }
}

private extension NSToolbarItem.Identifier {
    static let package = Self("package.menu")
    static let destinations = Self("navigation.destinations")
    static let newOrder = Self("order.new")
    static let saveRecord = Self("toolbar.save")
    static let inspector = Self("inspector.toggle")
}
