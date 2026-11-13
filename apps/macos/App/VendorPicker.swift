import AppKit
import SwiftUI

/// Entity selection is distinct from free-entry classification. The search text is
/// local to the chooser and never becomes a Vendor fact.
struct VendorPicker: View {
    @Binding var text: String
    let choices: [String]
    let identifier: String
    let newVendor: () -> Void
    @State private var presented = false
    @State private var query = ""
    @State private var selection: String?
    @State private var createAfterDismissal = false

    private var matches: [String] {
        query.isEmpty ? choices : choices.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        Button {
            query = ""
            selection = choices.contains(text) ? text : nil
            presented = true
        } label: {
            HStack {
                // Its own color keeps the choice legible while the picker is disabled.
                Text(text.isEmpty ? "Choose Vendor…" : text)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer()
                Image(systemName: "chevron.up.chevron.down").font(.caption)
            }
        }
        .accessibilityLabel("Vendor")
        .accessibilityValue(text.isEmpty ? "None" : text)
        .accessibilityIdentifier(identifier)
        .popover(isPresented: $presented, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                SearchField(placeholder: "Search Vendors", text: $query, identifier: "vendor.chooser.search",
                            submit: chooseSelection, moveSelection: moveSelection, cancel: { presented = false })
                Button("New Vendor…") {
                    createAfterDismissal = true
                    presented = false
                }
                .accessibilityIdentifier("vendor.chooser.new")
                Divider()
                ScrollViewReader { scroll in
                    List(selection: $selection) {
                        ForEach(matches, id: \.self) { name in
                            Button(name) { choose(name) }
                                .buttonStyle(.plain)
                                .tag(name)
                                .id(name)
                        }
                    }
                    .listStyle(.inset)
                    .accessibilityLabel("Vendors")
                    .accessibilityIdentifier("vendor.chooser.choices")
                    .onKeyPress(.return) { chooseSelection(); return .handled }
                    .onChange(of: selection) { _, selected in
                        if let selected { scroll.scrollTo(selected) }
                    }
                }
                Button("Clear Vendor") { choose("") }
                    .disabled(text.isEmpty)
                    .accessibilityIdentifier("vendor.chooser.clear")
            }
            .padding(12)
            .frame(width: 360, height: 340)
            .onExitCommand { presented = false }
            .onChange(of: query) { _, _ in selection = matches.first }
            .onDisappear {
                if createAfterDismissal {
                    createAfterDismissal = false
                    newVendor()
                }
            }
        }
    }

    private func choose(_ name: String) { text = name; presented = false }

    private func moveSelection(_ offset: Int) {
        let names = matches
        guard !names.isEmpty else { return }
        if let selection, let index = names.firstIndex(of: selection) {
            self.selection = names[min(max(index + offset, 0), names.count - 1)]
        } else {
            selection = offset > 0 ? names.first : names.last
        }
    }

    private func chooseSelection() {
        if let selection, matches.contains(selection) { choose(selection) }
        else if matches.count == 1, let match = matches.first { choose(match) }
        else { NSSound.beep() }
    }
}
