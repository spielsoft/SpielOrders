import SpielOrdersKit
import SwiftUI

/// Merge N Orders…: the selected Orders in precedence order, the files the library's plan
/// moves into the kept Order, and the files it leaves behind because the kept Order already
/// has them. Merge commits the plan shown, once it is current and any discards are confirmed.
struct OrderMergeSheet: View {
    let plan: @MainActor ([OrderRow]) async throws -> OrderMerge
    let complete: (OrderMerge?) -> Void
    @State private var orders: [OrderRow]
    @State private var planned: OrderMerge
    @State private var failure: Notice?
    /// The discards the owner confirmed; Merge waits until they are the current plan's.
    @State private var confirmed: [MergedEntry] = []

    init(merge: OrderMerge, plan: @escaping @MainActor ([OrderRow]) async throws -> OrderMerge,
         complete: @escaping (OrderMerge?) -> Void) {
        self.plan = plan
        self.complete = complete
        _orders = State(initialValue: merge.orders)
        _planned = State(initialValue: merge)
    }

    /// The plan for the order shown, once the library has made it.
    private var current: OrderMerge? {
        planned.orders.map(\.id) == orders.map(\.id) ? planned : nil
    }

    private var ready: Bool {
        guard let current else { return false }
        return current.discarded.isEmpty || confirmed == current.discarded
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Merge \(orders.count) Orders")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text("The first Order is kept. Each Order after it only fills what the kept Order still lacks: "
                 + "missing facts, Items, and files. Drag the Orders to change their order.")
                .fixedSize(horizontal: false, vertical: true)
            PrecedenceList(items: $orders, identifier: "order.merge") { order in
                OrderMergeLabel(order: order)
            }
            .frame(height: 124)
            if let current {
                MergedFiles(merge: current)
                MergeConsequences(merge: current, confirmed: $confirmed)
            } else if let failure {
                Spacer()
                PlanningFailure(failure: failure, identifier: "order.merge")
                Spacer()
            } else {
                Spacer()
                ProgressView("Checking files…")
                    .frame(maxWidth: .infinity)
                    .accessibilityIdentifier("order.merge.checking")
                Spacer()
            }
            HStack {
                Spacer()
                Button("Cancel") { complete(nil) }
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("order.merge.cancel")
                Button("Merge") { complete(current) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ready)
                    .accessibilityIdentifier("order.merge.confirm")
            }
        }
        .padding(20)
        .frame(width: 600, height: 620)
        .task(id: orders.map(\.id)) {
            guard current == nil else { return }
            failure = nil
            do {
                let merge = try await plan(orders)
                try Task.checkCancellation()
                planned = merge
            } catch {
                guard !Task.isCancelled else { return }
                failure = Notice((error as? OrderMergeRefusal)?.summary ?? "The merge couldn’t be planned.",
                                 details: error.localizedDescription)
            }
        }
    }
}

/// An Order in the precedence list: its name, with its status, date, and Vendor beneath.
private struct OrderMergeLabel: View {
    let order: OrderRow

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(order.name)
                .lineLimit(1)
            Text(summary)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }

    private var summary: String {
        let line = OrderSummaryLine.of(status: order.values[OrderColumn.status.field] ?? "",
                                       date: order.values[OrderColumn.date.field] ?? "",
                                       vendor: order.values[OrderColumn.vendor.field] ?? "")
        return line.isEmpty ? order.directoryName : line
    }
}

/// The files the plan moves into the kept Order, then those it leaves behind, each under the
/// Order whose folder holds it.
private struct MergedFiles: View {
    let merge: OrderMerge

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                section(moving: true)
                if !merge.discarded.isEmpty {
                    section(moving: false)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(.background, in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.separator))
        .accessibilityLabel("Files")
        .accessibilityIdentifier("order.merge.files")
    }

    private func section(moving: Bool) -> some View {
        let entries = moving ? merge.moved : merge.discarded
        let kept = merge.orders[0].name
        return VStack(alignment: .leading, spacing: 6) {
            if moving {
                Text("Files moving into “\(kept)”")
                    .font(.subheadline.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
            } else {
                Label("Files “\(kept)” already has", systemImage: "exclamationmark.triangle.fill")
                    .font(.subheadline.weight(.semibold))
                    .symbolRenderingMode(.multicolor)
                    .accessibilityAddTraits(.isHeader)
                Text("“\(kept)” keeps its own file at each of these paths. These copies are not merged; "
                     + "they go with their Orders to Removed Orders.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if entries.isEmpty {
                Text("No files move.")
                    .foregroundStyle(.secondary)
            }
            ForEach(merge.orders.dropFirst()) { order in
                let held = entries.filter { $0.order == order.id }
                if !held.isEmpty {
                    EntryGroup(order: order, entries: held,
                               identifier: "order.merge." + (moving ? "moved" : "discarded"))
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("order.merge." + (moving ? "moved" : "discarded"))
    }
}

/// One Order's entries in the merge, by their paths within its folder.
private struct EntryGroup: View {
    let order: OrderRow
    let entries: [MergedEntry]
    let identifier: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("From “\(order.name)”")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(entries) { entry in
                Label(entry.path, systemImage: entry.isFolder ? "folder" : "doc")
                    .font(.callout)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("\(identifier).\(entry.path)")
            }
            .padding(.leading, 8)
        }
    }
}

/// What the merge does besides moving files, and the confirmation its discards need.
private struct MergeConsequences: View {
    let merge: OrderMerge
    @Binding var confirmed: [MergedEntry]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(record)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("order.merge.record")
            Text(removal)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("order.merge.recoverable")
            if !merge.discarded.isEmpty {
                Toggle(discardTitle, isOn: discarding)
                    .toggleStyle(.checkbox)
                    .accessibilityIdentifier("order.merge.confirmDiscard")
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }

    private var record: String {
        let kept = merge.orders[0].name
        return merge.fillsKept ? "“\(kept)” gains the facts and Items it lacks from the others."
            : "“\(kept)” keeps its facts and Items as they are."
    }

    private var removal: String {
        let count = merge.orders.count - 1
        let subject = count == 1 ? "The other Order moves" : "The other \(count) Orders move"
        return subject + " to Removed Orders, where merged Orders stay recoverable."
    }

    private var discardTitle: String {
        let count = merge.discarded.count
        return "Discard \(count) \(count == 1 ? "file" : "files") the kept Order already has"
    }

    private var discarding: Binding<Bool> {
        Binding {
            confirmed == merge.discarded
        } set: { approved in
            confirmed = approved ? merge.discarded : []
        }
    }
}

/// Why a reordered merge couldn't be planned, with the library's details.
struct PlanningFailure: View {
    let failure: Notice
    let identifier: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(failure.summary)
                .foregroundStyle(.problemText)
                .accessibilityIdentifier(identifier + ".problem")
            DisclosureGroup("Details") {
                ScrollView {
                    Text(failure.details)
                        .textSelection(.enabled)
                }
                .frame(maxHeight: 160)
            }
            .accessibilityIdentifier(identifier + ".details")
        }
    }
}

extension WindowDialogs {
    /// Asks the owner to order and confirm a merge of Orders, starting from `merge`; the plan
    /// they approved, or nil when cancelled.
    func reviewMerge(of merge: OrderMerge,
                     plan: @escaping @MainActor ([OrderRow]) async throws -> OrderMerge) async -> OrderMerge? {
        await ask("Merge \(merge.orders.count) Orders", identifier: "order.merge") { complete in
            OrderMergeSheet(merge: merge, plan: plan, complete: complete)
        }
    }
}
