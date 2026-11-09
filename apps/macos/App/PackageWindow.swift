import SwiftUI
import SpielOrdersKit

struct PackageWindow: View {
    @Bindable var model: DesktopModel
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Button("Open Package…") { model.choosePackage() }.disabled(model.busy)
                if let root = model.root {
                    Text(root.lastPathComponent).font(.headline)
                    Spacer()
                    Button("Reveal in Finder") { model.reveal() }
                    Button("Move Order to Trash…") { model.removeOrderToTrash() }
                        .disabled(model.busy || model.dirty)
                    Button("Save") { model.save() }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(!model.dirty || !model.writable || model.busy)
                }
                if model.busy { ProgressView().controlSize(.small) }
            }
            if model.root != nil {
                HStack {
                    TextField("Search Supporting Material with Spotlight", text: $model.query)
                        .onSubmit { model.searchMaterial() }
                        .accessibilityLabel("Spotlight query")
                    Button("Search") { model.searchMaterial() }.disabled(model.busy)
                    if model.searching { Button("Stop") { model.stopSearch() } }
                }
                if let answer = model.searchAnswer {
                    List(answer.matches) { match in
                        HStack {
                            Text(match.orderDirectory.lastPathComponent)
                            Text(match.relativePath)
                            Spacer()
                            Button("Reveal") { model.reveal(match.url) }
                                .accessibilityLabel("Reveal \(match.filename) in Finder")
                        }
                    }
                    .frame(minHeight: 100, maxHeight: 220)
                }
                Text("Package README").font(.title2)
                ReadmeEditor(text: $model.text, isEditable: !model.busy && model.writable)
            } else {
                ContentUnavailableView("Open a SpielOrder Package", systemImage: "folder",
                                       description: Text("Choose a Package to read and edit its README."))
            }
            if !model.message.isEmpty { Text(model.message).textSelection(.enabled).accessibilityIdentifier("operationMessage") }
        }
        .padding(20)
        .frame(minWidth: 700, minHeight: 500)
    }
}
