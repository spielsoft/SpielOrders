import SwiftUI

/// The line above a table that counts its rows, such as “3 of 12 Orders” or “4 Projects”.
struct ListCount: View {
    /// The rows shown, where search can hide some of them.
    var shown: Int?
    let total: Int
    /// What one row is, such as Order.
    let noun: String

    var body: some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(.secondary)
    }

    private var text: String {
        let counted = "\(total) \(total == 1 ? noun : noun + "s")"
        guard let shown else { return counted }
        return "\(shown) of \(counted)"
    }
}

/// What a record table shows while it lists nothing: Reload Required while the Package must be
/// reloaded, that there are none yet, or No Results for a search that matches nothing.
struct EmptyRecordList: View {
    @Bindable var model: DesktopModel
    /// The records before search.
    let total: Int
    /// What one row is, such as Order.
    let noun: String
    /// How to add one, such as “Add one with + or ⌘N.”
    let hint: String

    var body: some View {
        if let reason = model.reloadReason {
            ReloadRequired(model: model, reason: reason)
        } else if total == 0 {
            ContentUnavailableView("No \(noun)s", systemImage: "tray", description: Text(hint))
        } else if !model.search.text.isEmpty, !model.contents.gathering {
            ContentUnavailableView {
                Label("No Results for “\(model.search.text)”", systemImage: "magnifyingglass")
            } description: {
                Text("Searched in \(model.search.extent)")
            }
            .accessibilityIdentifier("search.noResults")
        }
    }
}
