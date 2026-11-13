import SwiftUI

/// The open Package's destinations, listed in the window's sidebar.
struct DestinationList: View {
    @Bindable var model: DesktopModel
    @State private var selection: DesktopModel.Destination?

    /// The destination the content shows; the diagnostic workspace is none of them.
    private var shown: DesktopModel.Destination? {
        model.diagnosticWorkspace ? nil : model.destination
    }

    var body: some View {
        List(selection: $selection) {
            Section {
                ForEach(DesktopModel.Destination.fixed, id: \.self) { destination in
                    DestinationRow(model: model, destination: destination)
                        .tag(destination)
                        .selectionDisabled(!model.canBrowseRecords)
                }
            }
        }
        .listStyle(.sidebar)
        .accessibilityLabel("Destinations")
        .accessibilityIdentifier("navigation.destinations")
        .onChange(of: selection) { _, chosen in
            if let chosen, chosen != shown {
                model.navigate(to: chosen)
            }
            // Navigation that started work settles the highlight when it finishes.
            if !model.busy {
                selection = shown
            }
        }
        .onChange(of: shown, initial: true) { _, shown in
            selection = shown
        }
        .onChange(of: model.busy) { _, busy in
            if !busy {
                selection = shown
            }
        }
    }
}

/// A destination, marked when listing it or reading its selection reported a problem.
private struct DestinationRow: View {
    @Bindable var model: DesktopModel
    let destination: DesktopModel.Destination

    var body: some View {
        HStack {
            Label(destination.title, systemImage: destination.symbol)
                .accessibilityIdentifier("navigation.\(destination.key)")
            if let warnings = model.warnings[destination], !warnings.isEmpty {
                Spacer()
                ProblemBadge(level: .warning, title: "Needs Attention", lines: warnings.map(\.summary),
                             actions: actions(warnings), label: "\(destination.title) needs attention",
                             identifier: "navigation.\(destination.key).warnings")
            }
        }
    }

    private func actions(_ warnings: [Notice]) -> [NoticeAction] {
        let prefix = "navigation.\(destination.key)"
        return [
            NoticeAction("Details…", identifier: prefix + ".details") { [model] in model.showDetails(warnings) },
            NoticeAction("Open Diagnostics", identifier: prefix + ".diagnose") { [model] in
                model.showDiagnosis(directory: nil)
            },
        ]
    }
}

private extension DesktopModel.Destination {
    var symbol: String {
        switch self {
        case .orders: "shippingbox"
        case .vendors: "building.2"
        case .values(.orderers): "person.2"
        case .values(.institutions): "building.columns"
        case .projects: "folder"
        case .packageReadme: "doc.text"
        }
    }
}
