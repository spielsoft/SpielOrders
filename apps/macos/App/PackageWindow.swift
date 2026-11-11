import SwiftUI
import SpielOrdersKit

struct PackageWindow: View {
    @Bindable var model: DesktopModel
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if model.root != nil, model.canBrowseRecords, !model.diagnosticWorkspace, !model.editingReadme, model.recordEditor == nil {
                HStack {
                    SearchField(placeholder: model.spotlightMode && model.destination == .orders ? "Search Supporting Material" : "Search records",
                                text: Binding(get: { model.query }, set: model.updateQuery),
                                identifier: "search.query", submit: { model.refreshLookup() })
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
            if model.diagnosticWorkspace {
                RecoveryWorkspace(model: model)
            } else if model.root != nil, model.destination == .orders {
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
            if model.busy { ProgressView().controlSize(.small).accessibilityLabel("Working") }
            if !model.message.isEmpty { Text(model.message).textSelection(.enabled).accessibilityIdentifier("operationMessage") }
            if let directory = model.recoverableDirectory {
                Button("Reveal Removed Order in Finder") { model.reveal(directory) }
                    .accessibilityIdentifier("order.removed.reveal")
            }
        }
        .padding(20)
        .frame(minWidth: 1000, maxWidth: .infinity, minHeight: 600, maxHeight: .infinity)
    }
}
