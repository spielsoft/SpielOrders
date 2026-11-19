import SwiftUI

/// A borderless icon button's label: the symbol alone, its title kept for help and assistive
/// technology, inside a click target larger than the symbol.
struct IconLabel: View {
    let title: String
    let systemImage: String

    init(_ title: String, systemImage: String) {
        self.title = title
        self.systemImage = systemImage
    }

    var body: some View {
        Label(title, systemImage: systemImage)
            .labelStyle(.iconOnly)
            .iconTarget()
    }
}

extension View {
    /// The click target the macOS guidelines ask of a small control, around a smaller symbol.
    func iconTarget() -> some View {
        frame(minWidth: 22, minHeight: 22).contentShape(Rectangle())
    }
}
