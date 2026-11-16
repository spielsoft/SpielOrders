import SwiftUI

/// A merge's selection in precedence order: the first is kept, and each later one only fills
/// what the result still lacks. Rows reorder by dragging, with Move Up and Move Down (⌘↑ and
/// ⌘↓) for the selected row, or with each row's accessibility actions.
struct PrecedenceList<Item: Identifiable, Label: View>: View where Item.ID == String {
    @Binding var items: [Item]
    let identifier: String
    @ViewBuilder let label: (Item) -> Label
    @State private var selection: String?

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            List(selection: $selection) {
                ForEach(Array(items.enumerated()), id: \.element.id) { position, item in
                    row(item, at: position)
                }
                .onMove { items.move(fromOffsets: $0, toOffset: $1) }
            }
            .listStyle(.bordered)
            .alternatingRowBackgrounds()
            .accessibilityLabel("Precedence")
            .accessibilityIdentifier(identifier + ".list")
            VStack(spacing: 6) {
                Button("Move Up", systemImage: "chevron.up") { moveSelection(by: -1) }
                    .keyboardShortcut(.upArrow, modifiers: .command)
                    .disabled(!canMoveSelection(by: -1))
                    .help("Move Up (⌘↑)")
                    .accessibilityIdentifier(identifier + ".moveUp")
                Button("Move Down", systemImage: "chevron.down") { moveSelection(by: 1) }
                    .keyboardShortcut(.downArrow, modifiers: .command)
                    .disabled(!canMoveSelection(by: 1))
                    .help("Move Down (⌘↓)")
                    .accessibilityIdentifier(identifier + ".moveDown")
            }
            .labelStyle(.iconOnly)
        }
    }

    private func row(_ item: Item, at position: Int) -> some View {
        HStack(spacing: 8) {
            Text("\(position + 1)")
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(minWidth: 16, alignment: .trailing)
            label(item)
            Spacer(minLength: 8)
            if position == 0 {
                KeptTag()
            }
        }
        .padding(.vertical, 2)
        .tag(item.id)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("\(identifier).position.\(position + 1)")
        .accessibilityAction(named: "Move Up") { move(item.id, by: -1) }
        .accessibilityAction(named: "Move Down") { move(item.id, by: 1) }
    }

    private func canMoveSelection(by offset: Int) -> Bool {
        guard let selection, let index = items.firstIndex(where: { $0.id == selection }) else { return false }
        return items.indices.contains(index + offset)
    }

    private func moveSelection(by offset: Int) {
        guard let selection else { return }
        move(selection, by: offset)
    }

    private func move(_ id: String, by offset: Int) {
        guard let index = items.firstIndex(where: { $0.id == id }), items.indices.contains(index + offset) else {
            return
        }
        items.swapAt(index, index + offset)
    }
}

/// Marks the first row of a precedence list, the one a merge keeps.
private struct KeptTag: View {
    var body: some View {
        Text("Kept")
            .font(.caption.weight(.medium))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .overlay(Capsule().strokeBorder(.secondary.opacity(0.6)))
    }
}
