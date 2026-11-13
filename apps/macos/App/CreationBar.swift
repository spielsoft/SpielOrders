import SwiftUI

/// The bar attached beneath a record table, holding the plus button that creates a record.
struct CreationBar: View {
    let title: String
    let identifier: String
    let action: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            HStack {
                Button(title, systemImage: "plus", action: action)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help(title)
                    .accessibilityIdentifier(identifier)
                Spacer()
            }
            .padding(.horizontal, 10)
            .frame(height: 28)
        }
    }
}
