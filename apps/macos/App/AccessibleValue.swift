import SwiftUI

extension View {
    // The container is deliberate. Exposed as bare static text, adjacent facts are merged
    // by the system into one element and stop being individually addressable.
    func accessibleValue(_ name: String, value: String, identifier: String) -> some View {
        accessibilityElement(children: .contain)
            .accessibilityLabel(Text(name))
            .accessibilityValue(Text(value))
            .accessibilityIdentifier(identifier)
    }
}
