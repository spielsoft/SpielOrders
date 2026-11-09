import CSpielOrders
import Foundation

public struct ReadmeSaveResult: Sendable {
    public let refreshedText: String?
    private let warnings: [String]
    public var canEdit: Bool { refreshedText != nil }

    init(refreshedText: String?, warnings: [String]) {
        self.refreshedText = refreshedText
        self.warnings = warnings
    }

    init(refreshedReadme: UnsafePointer<SpielordersReadme>, warnings: [String]) {
        let text = spielorders_readme_state(refreshedReadme) == SPIELORDERS_README_STATE_UNREADABLE
            ? nil : String(cString: spielorders_readme_text(refreshedReadme))
        self.init(refreshedText: text, warnings: warnings)
    }

    public var message: String {
        let status = !canEdit ? "Saved. Reopen the Package before editing again."
            : warnings.isEmpty ? "Saved." : "Saved with warnings."
        return ([status] + warnings).joined(separator: "\n")
    }
}

enum SessionAccessError: Error, LocalizedError {
    case reopenRequired
    var errorDescription: String? { "Open or reopen the Package before continuing." }
}
