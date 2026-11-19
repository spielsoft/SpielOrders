import SpielOrdersKit
import SwiftUI

/// A problem or consequence the app shows where it occurs: one line for its tooltip and
/// popover, and the library's full result for Details.
struct Notice: Hashable {
    let summary: String
    let details: String
    /// The Order Directory the problem is located in, or nil for one located elsewhere.
    var order: URL?

    init(_ summary: String, details: String = "") {
        self.summary = summary
        self.details = details
    }

    /// `problem` as the record it is located in shows it.
    init(_ problem: Problem) {
        self.init(problem.statement, details: problem.details)
        order = problem.order
    }

    /// The library's notices, one per line, without blanks or repeats.
    static func list(_ notices: [String]) -> [Notice] {
        var seen = Set<String>()
        return notices.filter { !$0.isEmpty && seen.insert($0).inserted }.map { Notice($0) }
    }

    /// `problems` as a list for the whole Package shows them, each naming the Order it is
    /// located in, without repeats.
    static func list(_ problems: [Problem]) -> [Notice] {
        var seen = Set<Notice>()
        return problems.map { problem in
            guard let order = problem.order else { return Notice(problem) }
            var notice = Notice("\(order.lastPathComponent): \(problem.statement)", details: problem.details)
            notice.order = order
            return notice
        }.filter { seen.insert($0).inserted }
    }
}

extension DesktopModel {
    func diagnosticText(_ notices: [String]) -> String {
        var seen = Set<String>()
        return notices.filter { !$0.isEmpty && seen.insert($0).inserted }.joined(separator: "\n\n")
    }

    func showDetails(_ notices: [Notice]) {
        Task { await dialogs.showDetails(notices) }
    }

    /// Says that an action the owner started failed, where nothing else in the window can
    /// show it. Show in Finder is offered for a `location` the failure left behind.
    func alert(_ title: String, _ explanation: String = "", details: String = "", identifier: String,
               location: URL? = nil) {
        Task {
            await dialogs.notify(title, explanation, identifier: identifier,
                                 diagnostic: details.isEmpty ? nil : details, location: location)
        }
    }

    /// Shows what listing `destination` reported on its sidebar row. The Orders row also
    /// carries the Package's own findings.
    func showListing(_ notices: [Problem], of destination: Destination, session: PackageSession) async {
        guard destination != .orders else {
            warnings[.orders] = Notice.list(await session.findings + notices)
            return
        }
        let shown = warnings[.orders] ?? []
        warnings[destination] = Notice.list(notices).filter { !shown.contains($0) }
    }

    /// Replaces the Orders and Vendors rows' warnings. A notice both listings report, such as
    /// a damaged Order, marks Orders only.
    func showWarnings(orders: [Notice], vendors: [Notice]) {
        warnings[.orders] = orders
        warnings[.vendors] = vendors.filter { !orders.contains($0) }
    }

    /// Retains committed warnings after refreshing the lists, separately from successes.
    func addWarnings(_ notices: [String], to destination: Destination) {
        for notice in Notice.list(notices) where warnings[destination]?.contains(notice) != true {
            warnings[destination, default: []].append(notice)
        }
    }

    /// Marks `destination`'s sidebar row with a failure to list it or read its selection.
    func warn(_ destination: Destination, _ summary: String, error: Error) {
        let notice = Notice(summary, details: error.localizedDescription)
        guard warnings[destination]?.contains(notice) != true else { return }
        warnings[destination, default: []].append(notice)
    }

    /// Records stay readable but cannot be edited until the Package is reloaded.
    func requireReload(_ reason: Notice) {
        canBrowseRecords = false
        reloadReason = reason
    }
}

/// The sheet Details opens: each notice with the library's full result.
struct FeedbackDetails: View {
    let notices: [Notice]
    let done: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Details").font(.headline).accessibilityAddTraits(.isHeader)
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    ForEach(Array(notices.enumerated()), id: \.offset) { index, notice in
                        entry(notice, index: index)
                    }
                }
            }
            .accessibilityLabel("Diagnostic Details")
            .accessibilityIdentifier("feedback.contents")
            HStack {
                Spacer()
                Button("Done", action: done)
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("feedback.done")
            }
        }
        .padding(20)
        .frame(width: 680, height: 460)
    }

    private func entry(_ notice: Notice, index: Int) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(notice.summary)
                .font(.headline)
            if !notice.details.isEmpty, notice.details != notice.summary {
                Text(notice.details)
                    .font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .accessibilityIdentifier("feedback.detail.\(index)")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct OperationReview: View {
    let title: String
    let explanation: String
    let details: String
    let identifier: String
    let approve: String
    let destructive: Bool
    let complete: (Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title).font(.headline).accessibilityAddTraits(.isHeader)
            Text(explanation)
            ScrollView {
                Text(details).font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier(identifier + ".details")
            }.accessibilityLabel("Proposed Changes")
            HStack {
                Spacer()
                Button("Cancel") { complete(false) }.keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier(identifier + ".cancel")
                Button(approve, role: destructive ? .destructive : nil) { complete(true) }
                    .accessibilityIdentifier(identifier + ".confirm")
            }
        }
        .padding(20)
        .frame(minWidth: 600, minHeight: 360)
    }
}
