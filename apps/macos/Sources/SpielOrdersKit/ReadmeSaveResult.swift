import CSpielOrders
import Foundation

/// A README as the library read it for editing.
public struct ReadmeText: Sendable {
    public let text: String
    /// Whether the library could read it; an unreadable README cannot be written.
    public let readable: Bool

    init(_ readme: UnsafePointer<SpielordersReadme>) {
        readable = spielorders_readme_state(readme) != SPIELORDERS_README_STATE_UNREADABLE
        text = String(cString: spielorders_readme_text(readme))
    }
}

public struct ReadmeSaveResult: Sendable {
    public let refreshedText: String?
    private let warnings: [String]
    public var canEdit: Bool { refreshedText != nil }
    public var hasWarnings: Bool { !warnings.isEmpty }

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
        let status = !canEdit ? "Saved. Reload the Package before editing again."
            : warnings.isEmpty ? "Saved." : "Saved with warnings."
        return ([status] + warnings).joined(separator: "\n")
    }
}

enum SessionAccessError: Error, LocalizedError {
    case reopenRequired
    case selectionChanged
    var errorDescription: String? {
        switch self {
        case .reopenRequired: "Open or reload the Package before continuing."
        case .selectionChanged: "The selected Order changed. Reload the Package before selecting it again."
        }
    }
}
