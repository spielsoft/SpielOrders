import SwiftUI
import SpielOrdersKit

/// The Package's Projects, each with its Order Groups beneath it and the number of Orders
/// using each, as the library lists them.
///
/// Like the other value destinations, the table takes the content's width. Projects sort by
/// the chosen column, and each Project's groups sort the same way beneath it. The groups of
/// Orders with no Project come last, under No Project. + adds a group to the selected
/// Project, or a Project when none is selected.
struct ProjectsWorkspace: View {
    @Bindable var model: DesktopModel
    @State private var sortOrder = [KeyPathComparator(\ValueRow.name)]
    @State private var collapsed: Set<ValueItem> = []

    private var rows: [ValueRow] {
        let named = model.projects.filter { $0.project != nil }.map(row).sorted(using: sortOrder)
        let unassigned = model.projects.filter { $0.project == nil }.map(row)
        let adding = model.naming == .new(.projects)
            ? [ValueRow(id: .new(.projects), name: "New Project", orderCount: 0)] : []
        return named + adding + unassigned
    }

    /// Where + adds: to the one selected Project, or the Project of the one selected group.
    private var addition: ValueScope {
        guard model.valueSelection.count == 1, let selected = model.valueSelection.first else { return .projects }
        switch selected {
        case let .value(scope, name) where scope == .projects: return .groups(of: name)
        case let .value(scope, _): return scope.project.map(ValueScope.groups) ?? .projects
        case .noProject: return .groups(of: "")
        case .new: return .projects
        }
    }

    private var additionTitle: String {
        guard let project = addition.project else { return "New Project" }
        return project.isEmpty ? "New Order Group with No Project" : "New Order Group in “\(project)”"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ListCount(total: model.projects.count { $0.project != nil }, noun: "Project")
                .padding(12)
            Table(of: ValueRow.self, selection: $model.valueSelection, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { row in
                    ValueName(model: model, row: row, identifier: "projects.row.\(key(row.id)).name")
                }
                .width(min: 160, ideal: 320, max: 480)
                TableColumn("Orders", value: \.usage) { row in
                    if let count = row.orderCount {
                        Text(count, format: .number)
                            .monospacedDigit()
                            .accessibleValue("Orders", value: String(count),
                                             identifier: "projects.row.\(key(row.id)).orders")
                    }
                }
                .width(min: 60, ideal: 80, max: 120)
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
            .tableStyle(.inset)
            .font(.callout)
            .disabled(!model.listsAvailable)
            .contextMenu(forSelectionType: ValueItem.self) { items in
                ValueMenu(model: model, items: items, prefix: "projects")
            } primaryAction: { items in
                if items.count == 1, let item = items.first { model.beginNaming(item) }
            }
            .overlay {
                if rows.isEmpty && !model.refreshingLookup {
                    ContentUnavailableView("No Projects", systemImage: "tray",
                                           description: Text("Add one with +, or from an Order’s Project field."))
                }
            }
            .accessibilityLabel("Projects")
            .accessibilityIdentifier("projects.table")
            CreationBar(title: additionTitle, identifier: "projects.new") { add() }
                .disabled(!model.canStartNew)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// A group is named beneath its Project, which opens to show it.
    private func add() {
        let scope = addition
        if let project = scope.project {
            collapsed.remove(project.isEmpty ? .noProject : .value(.projects, project))
        }
        model.beginNaming(.new(scope))
    }

    /// A Project's row, its groups beneath it in the table's order, then a group being named.
    private func row(_ listed: ProjectGroups) -> ValueRow {
        let project = listed.project?.name ?? ""
        let scope = ValueScope.groups(of: project)
        var groups = listed.groups.map { group in
            ValueRow(id: .value(scope, group.name), name: group.name, orderCount: group.orderCount)
        }.sorted(using: sortOrder)
        if model.naming == .new(scope) {
            groups.append(ValueRow(id: .new(scope), name: "New Order Group", orderCount: 0))
        }
        guard let named = listed.project else {
            return ValueRow(id: .noProject, name: "No Project", orderCount: nil, children: groups)
        }
        return ValueRow(id: .value(.projects, named.name), name: named.name, orderCount: named.orderCount,
                        children: groups)
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
        case let .new(scope):
            guard let project = scope.project else { return "New Project" }
            return "\(project.isEmpty ? "No Project" : project)/New Order Group"
        }
    }
}
