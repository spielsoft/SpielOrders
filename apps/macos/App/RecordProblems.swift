import SwiftUI

/// A record's state beside its heading: Not Saved, Reload Required, or warnings about how it
/// was saved or read. A saved record without warnings shows nothing.
struct RecordStateBadge: View {
    @Bindable var model: DesktopModel
    let editor: RecordEditor
    /// The library's notices about the record as it was read.
    var notices: [String] = []

    var body: some View {
        if let failure = editor.failure {
            ProblemBadge(level: .problem, title: "Not Saved", lines: [failure.summary] + problems,
                         actions: model.repairs(for: editor), identifier: "editor.notSaved")
        } else if !model.canBrowseRecords {
            ProblemBadge(level: .warning, title: "Reload Required", lines: [reloadReason],
                         actions: model.repairs(for: editor), identifier: "editor.reloadRequired")
        }
        if !editor.warnings.isEmpty {
            let warnings = editor.warnings + Notice.list(notices)
            ProblemBadge(level: .warning, title: "Saved with Warnings", lines: warnings.map(\.summary),
                         actions: [model.detailsAction(warnings, prefix: "editor.warnings")],
                         identifier: "editor.savedWarnings")
        } else if !notices.isEmpty {
            ProblemBadge(level: .warning, title: "Notices", lines: notices,
                         actions: [model.detailsAction(notices, prefix: "editor")], identifier: "editor.notices")
        }
    }

    private var reloadReason: String {
        model.reloadReason?.summary ?? "Reload the Package before editing."
    }

    /// Every problem the refused save located, with where it is; each field and cell is also
    /// marked, but a hidden column or an unshown fact is not.
    private var problems: [String] {
        let row = editor.kind == .order ? "Item" : "Contact"
        return editor.problems.map { problem in
            let field = RecordFieldLabel.of(problem.field)
            if problem.row >= 0 {
                return "\(row) \(problem.row + 1), \(field): \(problem.message)"
            }
            return field.isEmpty ? problem.message : "\(field): \(problem.message)"
        }
        .filter { $0 != editor.failure?.summary }
    }
}

/// The detail of a record that cannot be edited until the Package is reloaded.
struct ReloadBadge: View {
    @Bindable var model: DesktopModel
    let explanation: String
    let identifier: String
    var notice: Notice?

    var body: some View {
        ProblemBadge(level: .warning, title: "Reload Required", lines: reasons.isEmpty ? [explanation] : reasons.map(\.summary),
                     actions: actions, identifier: identifier)
    }

    private var reasons: [Notice] {
        [model.reloadReason, notice].compactMap { $0 }
    }

    private var actions: [NoticeAction] {
        var actions: [NoticeAction] = []
        if reasons.contains(where: { !$0.details.isEmpty }) {
            actions.append(NoticeAction("Details…", identifier: "\(identifier).details") { [model] in
                model.showDetails(reasons)
            })
        }
        actions.append(NoticeAction("Reload Package", identifier: "\(identifier).reload",
                                    perform: model.reloadPackage))
        return actions
    }
}

/// A list's place while the Package must be reloaded: why, with Reload Package.
struct ReloadRequired: View {
    @Bindable var model: DesktopModel
    let reason: Notice

    var body: some View {
        ContentUnavailableView {
            Label("Reload Required", systemImage: "arrow.clockwise")
        } description: {
            Text(reason.summary)
        } actions: {
            Button("Reload Package", action: model.reloadPackage)
                .accessibilityIdentifier("package.reload")
            if !reason.details.isEmpty {
                Button("Details…") { model.showDetails([reason]) }
                    .accessibilityIdentifier("package.reload.details")
            }
        }
        .accessibilityIdentifier("package.reloadRequired")
    }
}

extension DesktopModel {
    /// What fixes `editor`'s refused save, for its heading and each of its marked fields.
    func repairs(for editor: RecordEditor) -> [NoticeAction] {
        var actions = reloadActions(prefix: "editor", also: editor.failure)
        guard editor.refused, canBrowseRecords else { return actions }
        if editor === vendorSheet {
            actions.append(NoticeAction("Retry", identifier: "editor.retry", perform: saveVendorSheet))
            return actions
        }
        if !editor.isNew, editor.dirty {
            actions.append(NoticeAction("Revert…", identifier: "editor.revert", perform: revertRecord))
        }
        actions.append(NoticeAction("Retry", identifier: "editor.retry", perform: retryRecord))
        return actions
    }

    /// Details of `notice` and of why Reload is required, then Reload Package while it is.
    func reloadActions(prefix: String, also notice: Notice? = nil) -> [NoticeAction] {
        var actions: [NoticeAction] = []
        let details = [notice, canBrowseRecords ? nil : reloadReason].compactMap { $0 }
        if details.contains(where: { !$0.details.isEmpty }) {
            actions.append(NoticeAction("Details…", identifier: "\(prefix).details") { [self] in
                showDetails(details)
            })
        }
        if !canBrowseRecords, root != nil {
            actions.append(NoticeAction("Reload Package", identifier: "\(prefix).reload", perform: reloadPackage))
        }
        return actions
    }

    /// Details of the library's own lines.
    func detailsAction(_ lines: [String], prefix: String) -> NoticeAction {
        detailsAction(Notice.list(lines), prefix: prefix)
    }

    func detailsAction(_ notices: [Notice], prefix: String) -> NoticeAction {
        NoticeAction("Details…", identifier: "\(prefix).details") { [self] in
            showDetails(notices)
        }
    }
}
