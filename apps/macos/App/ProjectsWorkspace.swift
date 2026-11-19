import SwiftUI
import SpielOrdersKit

/// The Package's Projects, each with its Order Groups beneath it and the number of Orders
/// using each, as the library lists them, beside the selected Project's or group's detail.
struct ProjectsWorkspace: View {
    @Bindable var model: DesktopModel
    @State private var sortOrder = [KeyPathComparator(\ValueRow.name)]
    @State private var collapsed: Set<ValueItem> = []

    var body: some View {
        let rows = rows
        WorkspaceSplit(content: ProjectGrid(model: model, rows: rows, sortOrder: $sortOrder, collapsed: $collapsed),
                       detail: ValueDetail(model: model, kind: .projects, rows: rows) { item in
                           // A group selected from its Project's detail shows beneath it.
                           if let project = item.value?.scope.project {
                               collapsed.remove(project.isEmpty ? .noProject : .value(.projects, project))
                           }
                           model.valueSelection = [item]
                       },
                       layout: .values)
    }

    /// The Projects as the list shows them. Projects sort by the chosen column, and each
    /// Project's groups sort the same way beneath it. The groups of Orders with no Project come
    /// last, under No Project.
    private var rows: [ValueRow] {
        let named = model.projects.filter { $0.project != nil }.map(row).sorted(using: sortOrder)
        let unassigned = model.projects.filter { $0.project == nil }.map(row)
        let adding = model.naming == .new(.projects)
            ? [ValueRow(id: .new(.projects), name: "New Project", orderCount: 0)] : []
        return named + adding + unassigned
    }

    /// A Project's row, its groups beneath it in the table's order.
    private func row(_ listed: ProjectGroups) -> ValueRow {
        let project = listed.project?.name ?? ""
        let scope = ValueScope.groups(of: project)
        let groups = listed.groups.map { group in
            ValueRow(id: .value(scope, group.name), name: group.name, orderCount: group.orderCount)
        }.sorted(using: sortOrder)
        guard let named = listed.project else {
            return ValueRow(id: .noProject, name: "No Project", orderCount: nil, children: groups)
        }
        return ValueRow(id: .value(.projects, named.name), name: named.name, orderCount: named.orderCount,
                        children: groups)
    }
}

/// The Projects list. + adds a Project; a Project's detail adds its Order Groups.
private struct ProjectGrid: View {
    @Bindable var model: DesktopModel
    let rows: [ValueRow]
    @Binding var sortOrder: [KeyPathComparator<ValueRow>]
    @Binding var collapsed: Set<ValueItem>

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ListCount(total: model.projects.count { $0.project != nil }, noun: "Project")
                .padding(12)
            Table(of: ValueRow.self, selection: $model.valueSelection, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { row in
                    ValueName(model: model, row: row, identifier: "projects.row.\(key(row.id)).name")
                }
                .width(min: 80)
                TableColumn("Orders", value: \.usage) { row in
                    if let count = row.orderCount {
                        Text(count, format: .number)
                            .monospacedDigit()
                            .accessibleValue("Orders", value: String(count),
                                             identifier: "projects.row.\(key(row.id)).orders")
                    }
                }
                .width(72)
                .alignment(.numeric)
            } rows: {
                ForEach(rows) { project in
                    if project.children.isEmpty {
                        TableRow(project)
                    } else {
                        DisclosureTableRow(project, isExpanded: expansion(of: project.id)) {
                            ForEach(project.children) { group in
                                TableRow(group)
                            }
                        }
                    }
                }
            }
            .tableStyle(.inset(alternatesRowBackgrounds: !rows.isEmpty))
            .disabled(!model.listsAvailable)
            .contextMenu(forSelectionType: ValueItem.self) { items in
                ValueMenu(model: model, items: items, scope: .projects, prefix: "projects")
            } primaryAction: { items in
                if items.count == 1, let item = items.first { model.beginNaming(item) }
            }
            .onDeleteCommand(perform: model.valueDeletion(model.valueSelection))
            .overlay {
                if rows.isEmpty && !model.refreshingLookup {
                    ContentUnavailableView("No Projects", systemImage: "tray",
                                           description: Text("Add one with + or ⌘N, or from an Order’s Project field."))
                }
            }
            .accessibilityLabel("Projects")
            .accessibilityIdentifier("projects.table")
            CreationBar(title: "New Project", identifier: "projects.new") { model.beginNaming(.new(.projects)) }
                .disabled(!model.canStartNew)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Projects start expanded; the table remembers each one the user collapses.
    private func expansion(of id: ValueItem) -> Binding<Bool> {
        Binding {
            !collapsed.contains(id)
        } set: { expanded in
            if expanded {
                collapsed.remove(id)
            } else {
                collapsed.insert(id)
            }
        }
    }

    /// Identifies a row for accessibility: the Project, then the group after a slash.
    private func key(_ item: ValueItem) -> String {
        switch item {
        case let .value(scope, name):
            guard let project = scope.project else { return name }
            return "\(project.isEmpty ? "No Project" : project)/\(name)"
        case .noProject:
            return "No Project"
        case .new:
            return "New Project"
        }
    }
}
