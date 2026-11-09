import SwiftUI
import SpielOrdersKit

struct PackageWindow: View {
    @Bindable var model: DesktopModel
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Menu {
                    Button("Open Package…") { model.choosePackage() }.disabled(model.busy)
                    Button("Initialize Package…") { model.chooseInitialization() }.disabled(model.busy)
                    Button("Reopen Package") { model.reopenPackage() }
                        .disabled(model.busy || model.root == nil)
                    Divider()
                    Button("Reveal Package in Finder") { model.reveal() }.disabled(model.root == nil)
                } label: {
                    Label(model.root?.lastPathComponent ?? "Package", systemImage: "shippingbox")
                }
                .fixedSize()
                .accessibilityLabel("Package")
                .accessibilityValue(model.root?.lastPathComponent ?? "No Package")
                if model.root != nil {
                    DestinationPicker(model: model)
                    .fixedSize()
                    Spacer()
                    Button("Move Order to Trash…") { model.removeOrderToTrash() }
                        .disabled(model.busy || model.dirty)
                    Button("Save") { model.save() }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(!model.dirty || !model.writable || model.busy || !model.editingReadme)
                    if !model.editingReadme {
                        Button("Inspector", systemImage: "sidebar.trailing") { model.inspectorVisible.toggle() }
                            .labelStyle(.iconOnly).help("Show or hide the inspector")
                    }
                }
                if model.busy { ProgressView().controlSize(.small) }
            }
            if model.root != nil, !model.editingReadme {
                HStack {
                    TextField(model.spotlightMode && model.destination == .orders ? "Search Supporting Material" : "Search records",
                              text: Binding(get: { model.query }, set: model.updateQuery))
                        .onKeyPress(.return) { model.refreshLookup(); return .handled }
                        .accessibilityLabel("Search query")
                    Toggle("Aa", isOn: Binding(get: { model.caseSensitive }, set: model.setCaseSensitive))
                        .toggleStyle(.button).accessibilityLabel("Case sensitive")
                        .disabled(model.spotlightMode && model.destination == .orders)
                    if model.destination == .orders {
                        Toggle("Spotlight", isOn: Binding(get: { model.spotlightMode }, set: model.setSpotlightMode))
                            .toggleStyle(.button)
                        if model.spotlightMode {
                            Button("Search") { model.refreshLookup() }
                            if model.searching { Button("Stop") { model.stopSearch() } }
                        }
                    }
                }
                .disabled(model.busy && !model.refreshingLookup)
            }
            if model.root != nil, model.destination == .orders {
                if let order = model.editingOrder {
                    HStack {
                        Button("Back to Orders", systemImage: "chevron.left") { model.backToOrders() }.disabled(model.busy)
                        Text(order.values[OrderColumn.title.field] ?? "Order README").font(.title2)
                    }
                    ReadmeEditor(text: $model.text, isEditable: !model.busy && model.writable, label: "Order README")
                        .id(order.id)
                } else if model.spotlightMode {
                    if let answer = model.searchAnswer {
                        List(answer.matches) { match in
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(match.orderDirectory.lastPathComponent).font(.caption).foregroundStyle(.secondary)
                                    Text(match.relativePath)
                                }
                                Spacer()
                                Button("Open") { model.openFile(match.url) }
                                    .accessibilityLabel("Open \(match.filename)")
                                Button("Reveal") { model.reveal(match.url) }
                                    .accessibilityLabel("Reveal \(match.filename) in Finder")
                            }
                        }
                    } else {
                        ContentUnavailableView("Search Supporting Material", systemImage: "magnifyingglass")
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                } else {
                    OrdersWorkspace(model: model)
                }
            } else if model.root != nil, model.destination == .vendors {
                VendorsWorkspace(model: model)
            } else if model.root != nil {
                Text("Package README").font(.title2)
                ReadmeEditor(text: $model.text, isEditable: !model.busy && model.writable, label: "Package README")
            } else {
                ContentUnavailableView("Open a SpielOrder Package", systemImage: "folder",
                                       description: Text("Choose a Package to read and edit its README."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            if !model.message.isEmpty { Text(model.message).textSelection(.enabled).accessibilityIdentifier("operationMessage") }
        }
        .padding(20)
        .frame(minWidth: 1000, maxWidth: .infinity, minHeight: 600, maxHeight: .infinity)
    }
}
