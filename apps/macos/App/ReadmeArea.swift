import SpielOrdersKit
import SwiftUI

/// A README's always-editable text, marked at its corner when a save wrote nothing, warned,
/// or left it to wait for Reload.
struct ReadmeArea: View {
    @Bindable var model: DesktopModel
    let readme: ReadmeSession
    let label: String
    let identifier: String
    /// Bounds an area that grows with its text; without it, the area fills its space.
    let height: ClosedRange<CGFloat>?

    var body: some View {
        if readme.readable {
            ReadmeEditor(text: Binding(get: { readme.draft.text }, set: readme.edit),
                         isEditable: model.canType(readme), label: label, identifier: identifier,
                         height: height, onEnd: model.saveReadme)
                .id(ObjectIdentifier(readme))
                .overlay(alignment: .topTrailing) {
                    ReadmeBadge(model: model, readme: readme)
                        .padding(.top, 6)
                        .padding(.trailing, 20)
                }
        } else {
            Text("README is unreadable. Reload the Package to refresh.")
                .foregroundStyle(.secondary)
                .accessibilityIdentifier(identifier + ".note")
        }
    }
}

/// The README's state: Not Saved, saved with warnings, or waiting for Reload.
private struct ReadmeBadge: View {
    @Bindable var model: DesktopModel
    let readme: ReadmeSession

    var body: some View {
        HStack(spacing: 6) {
            if let failure = readme.failure {
                ProblemBadge(level: .problem, title: "Not Saved", lines: [failure.summary], actions: repairs(failure),
                             identifier: "readme.notSaved")
            } else if !readme.writable || !model.canBrowseRecords {
                ReloadBadge(model: model, explanation: "Reload the Package before editing this README.",
                            identifier: "readme.reloadRequired", notice: readme.writable ? nil : readme.warning)
            }
            if let warning = readme.warning, readme.writable {
                ProblemBadge(level: .warning, title: "Saved with Warnings", lines: [warning.summary],
                             actions: model.reloadActions(prefix: "readme.warnings", also: warning),
                             identifier: "readme.savedWarnings")
            }
        }
    }

    private func repairs(_ failure: Notice) -> [NoticeAction] {
        var actions = model.reloadActions(prefix: "readme", also: failure)
        actions.append(NoticeAction("Revert…", identifier: "readme.revert", perform: model.revertReadme))
        if model.canBrowseRecords, readme.writable {
            actions.append(NoticeAction("Retry", identifier: "readme.retry", perform: model.saveReadme))
        }
        return actions
    }
}
