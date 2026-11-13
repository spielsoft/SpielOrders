import CSpielOrders
import SwiftUI
import SpielOrdersKit

struct OrdersWorkspace: View {
    @Bindable var model: DesktopModel

    var body: some View {
        WorkspaceSplit(content: OrdersGrid(model: model), detail: OrderDetail(model: model))
    }
}

private struct OrdersGrid: View {
    @Bindable var model: DesktopModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                RecordSearch(model: model)
                ListCount(shown: model.orders.count, total: model.orderCount, noun: "Order")
            }
            .padding(12)
            OrdersTable(model: model)
                .overlay {
                    if model.orders.isEmpty, let reason = model.reloadReason {
                        ReloadRequired(model: model, reason: reason)
                    }
                }
            CreationBar(title: "New Order", identifier: "order.new", action: model.newOrder)
                .disabled(model.busy && !model.committing || !model.canBrowseRecords)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct OrderDetail: View {
    @Bindable var model: DesktopModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let editor = model.orderEditor {
                    OrderEditorView(model: model, editor: editor)
                        .id(ObjectIdentifier(editor))
                    RecordActions(model: model, editor: editor)
                    if let row = model.currentOrder, model.search.searchesFiles, !editor.isNew {
                        Divider()
                        MatchedFiles(model: model, row: row)
                    }
                    if let row = model.currentOrder, case let .healthy(details) = model.inspection {
                        Divider()
                        OrderAttachments(model: model, row: row, details: details)
                    }
                } else {
                    OrderSummary(model: model)
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityLabel("Order detail")
        .accessibilityIdentifier("order.detail")
    }
}

/// The selected Order's files that Folder Contents matched, most relevant first.
private struct MatchedFiles: View {
    @Bindable var model: DesktopModel
    let row: OrderRow

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Matched Files")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            if let answer = model.contents.answer(for: model.search.text) {
                let files = answer.matches(in: row)
                if files.isEmpty {
                    Text("Spotlight found no matching files in this Order.")
                        .foregroundStyle(.secondary)
                }
                ForEach(files) { file in
                    MatchedFile(model: model, file: file)
                }
            } else if model.contents.gathering {
                Text("Spotlight is searching…")
                    .foregroundStyle(.secondary)
            } else {
                Text("Spotlight did not search the files.")
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Matched Files")
        .accessibilityIdentifier("order.files")
    }
}

private struct MatchedFile: View {
    @Bindable var model: DesktopModel
    let file: MaterialMatch

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(file.filename)
                if file.relativePath != file.filename {
                    Text(file.relativePath)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if !facts.isEmpty {
                    Text(facts)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .textSelection(.enabled)
            Spacer()
            Button("Open") { model.openFile(file.url) }
                .accessibilityLabel("Open \(file.filename)")
                .accessibilityIdentifier("order.files.open.\(file.relativePath)")
            ShowInFinderButton { model.reveal(file.url) }
                .accessibilityLabel("Show \(file.filename) in Finder")
                .accessibilityIdentifier("order.files.reveal.\(file.relativePath)")
        }
        .controlSize(.small)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(file.relativePath)
    }

    /// The file's kind and modification date, where Spotlight stated them.
    private var facts: String {
        let modified = file.modified?.formatted(date: .abbreviated, time: .shortened)
        return [file.kind, modified ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// The detail without an Order's editor: a damaged Order's diagnosis, an Order that cannot
/// be edited until the Package is reloaded, or no selection.
private struct OrderSummary: View {
    @Bindable var model: DesktopModel

    var body: some View {
        if let row = model.currentOrder {
            switch model.inspection {
            case let .healthy(details):
                if model.canBrowseRecords {
                    ProgressView("Reading Order…")
                } else {
                    RecordHeading(title: row.values[OrderColumn.title.field] ?? "", summary: summary(row),
                                  prefix: "order.inspector") {
                        ReloadBadge(model: model, explanation: "Reload the Package to edit this Order.",
                                    identifier: "order.inspector.reloadRequired")
                    }
                    OrderFolder(model: model, order: row, prefix: "order.inspector")
                    OrderAttachments(model: model, row: row, details: details)
                }
            case let .damaged(messages):
                DamagedOrder(model: model, row: row, messages: messages)
            case nil:
                if model.busy {
                    ProgressView("Reading Order…")
                }
                HStack {
                    Text(row.directoryName)
                        .textSelection(.enabled)
                    if !model.busy {
                        ReloadBadge(model: model, explanation: "The Order’s details couldn’t be read. Reload the Package.",
                                    identifier: "order.inspector.unavailable")
                    }
                    ShowOrderInFinder(model: model, row: row)
                }
            }
        } else {
            Text("Select an Order")
                .foregroundStyle(.secondary)
        }
    }

    private func summary(_ row: OrderRow) -> String {
        OrderSummaryLine.of(status: row.values[OrderColumn.status.field] ?? "",
                            date: row.values[OrderColumn.date.field] ?? "",
                            vendor: row.values[OrderColumn.vendor.field] ?? "")
    }
}

/// A damaged Order's trusted directory name and diagnosis, with the way to repair it.
private struct DamagedOrder: View {
    @Bindable var model: DesktopModel
    let row: OrderRow
    let messages: [String]

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(row.directoryName)
                .font(.headline)
                .textSelection(.enabled)
                .accessibleValue("Directory Name", value: row.directoryName, identifier: "order.inspector.directory")
            ProblemBadge(level: .problem, title: "Damaged Order", lines: messages,
                         actions: [model.detailsAction(messages, prefix: "order.inspector")],
                         identifier: "order.inspector.diagnosis")
            ShowOrderInFinder(model: model, row: row)
        }
        Button("Open Diagnostics") { model.showDiagnosis(directory: row.directory) }
            .disabled(model.busy)
            .accessibilityIdentifier("order.inspector.diagnose")
    }
}

/// What the Order form does not show: the README, editable beside the Order's editor.
/// Without the editor, the library's text shows.
private struct OrderAttachments: View {
    @Bindable var model: DesktopModel
    let row: OrderRow
    let details: OrderDetails

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("README")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            if let readme = model.readme, readme.order?.id == row.id {
                ReadmeArea(model: model, readme: readme, label: "Order README", identifier: "order.readme.editor",
                           height: 88...400)
            } else if details.readmeState == SPIELORDERS_README_STATE_UNREADABLE {
                Text("README is unreadable. Reload the Package to refresh.")
            } else {
                Text(details.readmeText)
                    .font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .accessibleValue("Order README", value: details.readmeText, identifier: "order.inspector.readme")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A small borderless action that shows a location in Finder.
struct ShowInFinderButton: View {
    let action: () -> Void

    var body: some View {
        Button("Show in Finder", systemImage: "folder", action: action)
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .help("Show in Finder")
    }
}

/// Shows the Order's folder in Finder, placed beside its directory name.
struct ShowOrderInFinder: View {
    @Bindable var model: DesktopModel
    let row: OrderRow

    var body: some View {
        ShowInFinderButton { model.revealOrder(row.id) }
            .disabled(model.busy)
            .accessibilityIdentifier("order.inspector.reveal")
    }
}
