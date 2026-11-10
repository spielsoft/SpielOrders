import SwiftUI
import SpielOrdersKit

struct PackageWindow: View {
    @Bindable var model: DesktopModel
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Menu {
                    Button("Open Package…") { model.choosePackage() }.disabled(model.busy)
                        .accessibilityIdentifier("package.open")
                    Button("Initialize Package…") { model.chooseInitialization() }.disabled(model.busy)
                        .accessibilityIdentifier("package.initialize")
                    Button("Reopen Package") { model.reopenPackage() }
                        .disabled(model.busy || model.root == nil)
                        .accessibilityIdentifier("package.reopen")
                    Divider()
                    Button("Reveal Package in Finder") { model.reveal() }.disabled(model.root == nil)
                        .accessibilityIdentifier("package.reveal")
                } label: {
                    Label(model.root?.lastPathComponent ?? "Package", systemImage: "shippingbox")
                }
                .fixedSize()
                .accessibilityLabel("Package")
                .accessibilityIdentifier("package.menu")
                .accessibilityValue(model.root?.lastPathComponent ?? "No Package")
                if model.root != nil {
                    DestinationPicker(model: model)
                    .fixedSize()
                    Spacer()
                    Button("New Order", systemImage: "plus") { model.newOrder() }
                        .disabled(model.busy)
                        .accessibilityIdentifier("order.new")
                    Button("Move Order to Trash…") { model.removeOrderToTrash() }
                        .disabled(model.busy || model.dirty || model.recordEditor != nil)
                        .accessibilityIdentifier("order.trash")
                    Button("Save") { model.save() }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(!model.canSave)
                        .accessibilityIdentifier("toolbar.save")
                    if !model.editingReadme, model.recordEditor == nil {
                        Button("Inspector", systemImage: "sidebar.trailing") { model.inspectorVisible.toggle() }
                            .labelStyle(.iconOnly).help("Show or hide the inspector")
                            .accessibilityLabel("Inspector")
                            .accessibilityValue(model.inspectorVisible ? "Shown" : "Hidden")
                            .accessibilityIdentifier("inspector.toggle")
                    }
                }
                if model.busy { ProgressView().controlSize(.small).accessibilityLabel("Working") }
            }
            if model.root != nil, !model.editingReadme, model.recordEditor == nil {
                HStack {
                    TextField(model.spotlightMode && model.destination == .orders ? "Search Supporting Material" : "Search records",
                              text: Binding(get: { model.query }, set: model.updateQuery))
                        .onKeyPress(.return) { model.refreshLookup(); return .handled }
                        .accessibilityLabel("Search query")
                        .accessibilityIdentifier("search.query")
                    Toggle("Aa", isOn: Binding(get: { model.caseSensitive }, set: model.setCaseSensitive))
                        .toggleStyle(.button).accessibilityLabel("Case sensitive")
                        .disabled(model.spotlightMode && model.destination == .orders)
                        .accessibilityIdentifier("search.caseSensitive")
                    if model.destination == .orders {
                        Toggle("Spotlight", isOn: Binding(get: { model.spotlightMode }, set: model.setSpotlightMode))
                            .toggleStyle(.button)
                            .accessibilityIdentifier("search.spotlight")
                        if model.spotlightMode {
                            Button("Search") { model.refreshLookup() }
                                .accessibilityIdentifier("search.start")
                            if model.searching {
                                Button("Stop") { model.stopSearch() }
                                    .accessibilityLabel("Stop search")
                                    .accessibilityIdentifier("search.stop")
                            }
                        }
                    }
                }
                .disabled(model.busy && !model.refreshingLookup)
            }
            if model.root != nil, model.destination == .orders {
                if let editor = model.orderEditor {
                    OrderEditorView(model: model, editor: editor)
                        .id(ObjectIdentifier(editor))
                } else if let order = model.editingOrder {
                    HStack {
                        Button("Back to Orders", systemImage: "chevron.left") { model.backToOrders() }.disabled(model.busy)
                            .accessibilityIdentifier("order.readme.back")
                        Text(order.values[OrderColumn.title.field] ?? "Order README").font(.title2)
                    }
                    ReadmeEditor(text: $model.text, isEditable: !model.busy && model.writable,
                                 label: "Order README", identifier: "order.readme.editor")
                        .id(model.readmeDocumentID)
                } else if model.spotlightMode {
                    if let answer = model.searchAnswer {
                        ScrollView {
                            LazyVStack(alignment: .leading, spacing: 12) {
                                ForEach(answer.matches) { match in
                                    HStack {
                                        VStack(alignment: .leading) {
                                            Text(match.orderDirectory.lastPathComponent).font(.caption).foregroundStyle(.secondary)
                                            Text(match.relativePath)
                                        }
                                        Spacer()
                                        Button("Open") { model.openFile(match.url) }
                                            .accessibilityLabel("Open \(match.filename)")
                                            .accessibilityIdentifier("material.open.\(match.url.absoluteString)")
                                        Button("Reveal") { model.reveal(match.url) }
                                            .accessibilityLabel("Reveal \(match.filename) in Finder")
                                            .accessibilityIdentifier("material.reveal.\(match.url.absoluteString)")
                                    }
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .accessibilityElement(children: .contain)
                                    .accessibilityLabel("\(match.orderDirectory.lastPathComponent): \(match.relativePath)")
                                    .accessibilityIdentifier("material.row.\(match.url.absoluteString)")
                                }
                            }
                        }
                        .accessibilityLabel("Supporting Material results")
                        .accessibilityIdentifier("material.results")
                    } else {
                        ContentUnavailableView("Search Supporting Material", systemImage: "magnifyingglass")
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                } else {
                    OrdersWorkspace(model: model)
                }
            } else if model.root != nil, model.destination == .vendors {
                if let editor = model.vendorEditor {
                    VendorEditorView(model: model, editor: editor)
                        .id(ObjectIdentifier(editor))
                }
                else { VendorsWorkspace(model: model) }
            } else if model.root != nil {
                Text("Package README").font(.title2)
                ReadmeEditor(text: $model.text, isEditable: !model.busy && model.writable,
                             label: "Package README", identifier: "package.readme.editor")
                    .id(model.readmeDocumentID)
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
