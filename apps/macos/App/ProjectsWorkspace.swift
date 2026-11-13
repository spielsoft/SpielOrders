import SwiftUI
import SpielOrdersKit

/// The Package's Projects, each with its Order Groups beneath it and the number of Orders
/// using each, as the library lists them.
///
/// Like the other value destinations, the table is display-only and takes the content's
/// width. Projects sort by the chosen column, and each Project's groups sort the same way
/// beneath it. The groups of Orders with no Project come last, under No Project.
struct ProjectsWorkspace: View {
    @Bindable var model: DesktopModel
    @State private var sortOrder = [KeyPathComparator(\ProjectRow.name)]
    @State private var collapsed: Set<ProjectRow.ID> = []

    private var rows: [ProjectRow] {
        let named = model.projects.filter { $0.project != nil }
        let unassigned = model.projects.filter { $0.project == nil }
        let sorted = named.map(row).sorted(using: sortOrder)
        return sorted + unassigned.map(row)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ListCount(total: model.projects.count { $0.project != nil }, noun: "Project")
                .padding(12)
            Table(of: ProjectRow.self, sortOrder: $sortOrder) {
                TableColumn("Name", value: \.name) { row in
                    Text(row.name)
                        .foregroundStyle(row.orderCount == nil ? HierarchicalShapeStyle.secondary : .primary)
                        .accessibleValue("Name", value: row.name, identifier: "projects.row.\(row.key).name")
                }
                .width(min: 160, ideal: 320, max: 480)
                TableColumn("Orders", value: \.usage) { row in
                    if let count = row.orderCount {
                        Text(count, format: .number)
                            .monospacedDigit()
                            .accessibleValue("Orders", value: String(count),
                                             identifier: "projects.row.\(row.key).orders")
                    }
                }
                .width(min: 60, ideal: 80, max: 120)
                .alignment(.numeric)
            } rows: {
                ForEach(rows) { project in
                    if project.groups.isEmpty {
                        TableRow(project)
                    } else {
                        DisclosureTableRow(project, isExpanded: expansion(of: project.id)) {
                            ForEach(project.groups) { group in
                                TableRow(group)
                            }
                        }
                    }
                }
            }
            .tableStyle(.inset)
            .font(.callout)
            .overlay {
                if model.projects.isEmpty && !model.refreshingLookup {
                    ContentUnavailableView("No Projects", systemImage: "tray",
                                           description: Text("Projects appear here once an Order uses one."))
                }
            }
            .accessibilityLabel("Projects")
            .accessibilityIdentifier("projects.table")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// A Project's row, its groups beneath it in the table's order.
    private func row(_ listed: ProjectGroups) -> ProjectRow {
        let project = listed.project?.name
        let groups = listed.groups.map { group in
            ProjectRow(id: ProjectRow.ID(project: project, group: group.name), name: group.name,
                       orderCount: group.orderCount, groups: [])
        }
        return ProjectRow(id: ProjectRow.ID(project: project, group: nil), name: project ?? "No Project",
                          orderCount: listed.project?.orderCount, groups: groups.sorted(using: sortOrder))
    }

    /// Projects start expanded; the table remembers each one the user collapses.
    private func expansion(of id: ProjectRow.ID) -> Binding<Bool> {
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
}

/// A row of the Projects table: a Project, No Project, or an Order Group beneath one.
struct ProjectRow: Identifiable {
    struct ID: Hashable {
        /// The Project, or nil for the Orders with no Project.
        let project: String?
        /// The Order Group, or nil for the Project's own row.
        let group: String?
    }

    let id: ID
    let name: String
    /// How many Orders use it; nil for No Project, which the library does not count.
    let orderCount: Int?
    let groups: [ProjectRow]

    /// The Orders column's sort key.
    var usage: Int { orderCount ?? 0 }

    /// Identifies the row for accessibility: the Project, then the group after a slash.
    var key: String {
        let project = id.project ?? "No Project"
        guard let group = id.group else { return project }
        return "\(project)/\(group)"
    }
}
