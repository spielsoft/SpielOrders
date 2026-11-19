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
                    if model.orders.isEmpty {
                        EmptyRecordList(model: model, total: model.orderCount, noun: "Order",
                                        hint: "Add one with + or ⌘N.")
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
        if !model.multipleSelection.isEmpty {
            SelectedOrders(model: model)
        } else if model.orderEditor != nil || model.currentOrder != nil {
            SelectedOrder(model: model)
        } else if !model.orders.isEmpty {
            ContentUnavailableView { Text("No Order Selected") }
                .accessibilityIdentifier("order.detail.placeholder")
        }
    }
}

/// The detail for several selected Orders: how many, and Merge.
private struct SelectedOrders: View {
    @Bindable var model: DesktopModel

    var body: some View {
        ContentUnavailableView {
            Label("\(model.multipleSelection.count) Orders Selected", systemImage: "doc.on.doc")
        } description: {
            Text("Merging keeps the Order you selected first. You can change that before merging.")
        } actions: {
            Button(model.mergeTitle) { model.mergeOrders() }
                .disabled(!model.canMergeOrders)
                .accessibilityIdentifier("order.selection.merge")
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Order detail")
        .accessibilityIdentifier("order.selection")
    }
}

/// The detail for one selected Order: its editor, or what can be shown without one.
private struct SelectedOrder: View {
    /// The narrowest the Order lays out, its facts in one column; a narrower detail scrolls.
    private static let narrowest: CGFloat = 420
    @Bindable var model: DesktopModel

    var body: some View {
        // The README is last, so it takes the height the form and Items leave.
        DetailScroll(narrowest: Self.narrowest) {
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
        .accessibilityLabel("Order detail")
        .accessibilityIdentifier("order.detail")
    }
}

/// The selected Order's files that Folder Contents matched, most relevant first.
private struct MatchedFiles: View {
    @Bindable var model: DesktopModel
    let row: OrderRow

    var body: some View {
        DetailSection("Matched Files") {
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

/// The detail without an Order's editor: a damaged Order's diagnosis, or an Order that cannot
/// be edited until the Package is reloaded.
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
                        ReloadBadge(model: model, explanation: "Reload the Package to keep editing.",
                                    identifier: "order.inspector.reloadRequired")
                    }
                    OrderFolder(model: model, order: row, prefix: "order.inspector")
                    Divider()
                    OrderAttachments(model: model, row: row, details: details)
                }
            case let .damaged(problems):
                DamagedOrder(model: model, row: row, notices: problems.map(Notice.init))
            case nil:
                if model.busy {
                    ProgressView("Reading Order…")
                }
                HStack(alignment: .firstTextBaseline) {
                    Text(row.directoryName)
                        .font(.title2)
                        .textSelection(.enabled)
                    if !model.busy {
                        ReloadBadge(model: model, explanation: "The Order’s details couldn’t be read. Reload the Package.",
                                    identifier: "order.inspector.unavailable")
                    }
                    ShowOrderInFinder(model: model, row: row)
                }
            }
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
    let notices: [Notice]

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(row.directoryName)
                .font(.title2)
                .textSelection(.enabled)
                .accessibleValue("Directory Name", value: row.directoryName, identifier: "order.inspector.directory")
            ProblemBadge(level: .problem, title: "Damaged Order", lines: notices.map(\.summary),
                         actions: [model.detailsAction(notices, prefix: "order.inspector")],
                         identifier: "order.inspector.diagnosis")
            ShowOrderInFinder(model: model, row: row)
        }
        Button("Diagnose Order") { model.showDiagnosis(directory: row.directory) }
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
        DetailSection("README") {
            if let readme = model.readme, readme.order?.id == row.id {
                ReadmeArea(model: model, readme: readme, label: "Order README", identifier: "order.readme.editor",
                           height: 88...400)
                    .frame(maxHeight: .infinity)
            } else if details.readmeState == SPIELORDERS_README_STATE_UNREADABLE {
                Text("The README couldn’t be read. Reload the Package.")
                    .foregroundStyle(.secondary)
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
        Button(action: action) { IconLabel("Show in Finder", systemImage: "folder") }
            .buttonStyle(.borderless)
            .help("Show in Finder")
    }
}

/// Opens the Order's folder in Finder to show its contents, placed beside its directory name.
struct ShowOrderInFinder: View {
    @Bindable var model: DesktopModel
    let row: OrderRow

    var body: some View {
        Button { model.revealOrder(row.id, contents: true) } label: {
            IconLabel("Open Folder in Finder", systemImage: "folder")
        }
        .buttonStyle(.borderless)
        .help("Open Folder in Finder")
        // A click while a field saves waits for the save, so the button doesn't dim for it.
        .disabled(model.busy && !model.committing)
        .accessibilityIdentifier("order.inspector.reveal")
    }
}
