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
