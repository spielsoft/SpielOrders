import AppKit
import SwiftUI
import SpielOrdersKit

/// The window's content beside the destination sidebar. Each pane pads its own content, so
/// tables and dividers reach the window's edges.
struct PackageWindow: View {
    @Bindable var model: DesktopModel

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .frame(minWidth: 1024, minHeight: 600)
    }

    @ViewBuilder private var content: some View {
        if model.starting {
            ProgressView("Opening Package…")
                .accessibilityIdentifier("package.starting")
        } else if model.root == nil {
            Welcome(model: model)
        } else if model.diagnosticWorkspace {
            RecoveryWorkspace(model: model)
                .padding(12)
        } else {
            switch model.destination {
            case .orders:
                OrdersWorkspace(model: model)
            case .vendors:
                VendorsWorkspace(model: model)
            case let .values(list):
                ValuesWorkspace(model: model, list: list)
            case .projects:
                ProjectsWorkspace(model: model)
            case .packageReadme:
                PackageReadme(model: model)
            }
        }
    }
}

private struct Welcome: View {
    @Bindable var model: DesktopModel

    var body: some View {
        if model.busy && !model.dialogs.isPresenting {
            ProgressView(model.activity)
                .accessibilityIdentifier("welcome.working")
        } else {
            VStack(spacing: 24) {
                ContentUnavailableView {
                    Label("SpielOrders", systemImage: "folder")
                } description: {
                    Text("Open a Package or create a new one to manage Orders and Vendors.")
                } actions: {
                    Button("Open Package…", action: model.choosePackage)
                        .accessibilityIdentifier("welcome.open")
                    Button("New Package…", action: model.chooseInitialization)
                        .accessibilityIdentifier("welcome.initialize")
                    Button("Set Up Agent Guidance…", action: model.makeAgentReady)
                        .disabled(model.busy)
                        .accessibilityIdentifier("welcome.agentReady")
                }
                .fixedSize(horizontal: false, vertical: true)
                if !model.recents.entries.isEmpty {
                    RecentPackageList(model: model)
                }
            }
        }
    }
}

/// The recently opened Packages, most recent first; one opens on click.
private struct RecentPackageList: View {
    @Bindable var model: DesktopModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Recent Packages")
                .font(.headline)
                .foregroundStyle(.secondary)
                .accessibilityAddTraits(.isHeader)
            ForEach(model.recents.entries) { recent in
                Button {
                    model.openRecent(recent)
                } label: {
                    Label(recent.name, systemImage: "shippingbox")
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .help(recent.id)
                .contextMenu {
                    Button("Remove from List") { model.recents.remove(recent) }
                }
                .accessibilityIdentifier("welcome.recent")
            }
        }
        .frame(width: 320, alignment: .leading)
        .disabled(model.busy)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Recent Packages")
        .accessibilityIdentifier("welcome.recents")
    }
}

private struct PackageReadme: View {
    @Bindable var model: DesktopModel

    var body: some View {
        if let readme = model.readme {
            ReadmeArea(model: model, readme: readme, label: "Package README", identifier: "package.readme.editor",
                       height: nil)
                .padding(12)
        }
    }
}
