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
    }

    @ViewBuilder private var content: some View {
        if model.starting {
            ProgressView("Opening Package…")
                .accessibilityIdentifier("package.starting")
        } else if model.root == nil {
            Welcome(model: model)
        } else {
            package
                .frame(minWidth: WorkspaceController.minimumWidth, minHeight: 600)
        }
    }

    @ViewBuilder private var package: some View {
        if model.diagnosticWorkspace {
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

/// What the window shows without a Package: its actions beside the recently opened Packages.
private struct Welcome: View {
    @Bindable var model: DesktopModel

    var body: some View {
        if model.busy && !model.dialogs.isPresenting {
            ProgressView(model.activity)
                .accessibilityIdentifier("welcome.working")
        } else {
            HStack(spacing: 0) {
                actions
                    .frame(width: 300)
                    .padding(24)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                Divider()
                RecentPackageList(model: model)
                    .frame(width: 280)
            }
        }
    }

    private var actions: some View {
        VStack(spacing: 16) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 96, height: 96)
                .accessibilityHidden(true)
            VStack(spacing: 4) {
                Text("SpielOrders")
                    .font(.title.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
                Text("Open a Package or create a new one to manage Orders and Vendors.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            VStack(spacing: 8) {
                Button(action: model.choosePackage) {
                    Text("Open Package…").frame(maxWidth: .infinity)
                }
                .accessibilityIdentifier("welcome.open")
                Button(action: model.chooseInitialization) {
                    Text("New Package…").frame(maxWidth: .infinity)
                }
                .accessibilityIdentifier("welcome.initialize")
                Button(action: model.makeAgentReady) {
                    Text("Set Up Agent Guidance…").frame(maxWidth: .infinity)
                }
                .disabled(model.busy)
                .accessibilityIdentifier("welcome.agentReady")
            }
            .controlSize(.large)
            .frame(width: 240)
            .padding(.top, 8)
        }
    }
}

/// The recently opened Packages, most recent first; one opens on click.
private struct RecentPackageList: View {
    @Bindable var model: DesktopModel

    var body: some View {
        List {
            ForEach(model.recents.entries) { recent in
                Button {
                    model.openRecent(recent)
                } label: {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(recent.name)
                            Text(URL(fileURLWithPath: recent.id).deletingLastPathComponent().path)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .truncationMode(.middle)
                        }
                        .lineLimit(1)
                    } icon: {
                        Image(systemName: "shippingbox")
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .padding(.vertical, 4)
                .help(recent.id)
                .contextMenu {
                    Button("Remove from List") { model.recents.remove(recent) }
                }
                .accessibilityIdentifier("welcome.recent")
            }
        }
        .listStyle(.sidebar)
        .overlay {
            if model.recents.entries.isEmpty {
                ContentUnavailableView("No Recent Packages", systemImage: "clock")
            }
        }
        .safeAreaInset(edge: .bottom) {
            // The list's counterpart of File → Open Recent → Clear Menu.
            Button("Clear Recent Packages", action: model.recents.clear)
                .buttonStyle(.borderless)
                .controlSize(.small)
                .disabled(model.recents.entries.isEmpty)
                .padding(8)
                .frame(maxWidth: .infinity)
                .accessibilityIdentifier("welcome.recents.clear")
        }
        .disabled(model.busy)
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
