import CSpielOrders
import SpielOrdersKit
import SwiftUI

// The Order form's layout. The editor supplies each fact's control.

/// The kind of control an Order fact is entered with.
enum OrderFieldControl {
    case text, date, status, vendor, suggesting

    init(_ field: Int32) {
        switch field {
        case SPIELORDERS_ORDER_FIELD_DATE_ORDERED:
            self = .date
        case SPIELORDERS_ORDER_FIELD_STATUS:
            self = .status
        case SPIELORDERS_ORDER_FIELD_VENDOR:
            self = .vendor
        case SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION,
             SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDER_GROUP:
            self = .suggesting
        default:
            self = .text
        }
    }
}

enum OrderSummaryLine {
    /// One line of the facts present: status label · short date · vendor name.
    @MainActor static func of(status: String, date: String, vendor: String) -> String {
        [OrderStatus.label(status), ShortDate.display(date), vendor]
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }
}

/// The Order's folder: its directory name with Show in Finder, and Align Directory Name when
/// the library reports that the name differs from the one the Order's facts project.
struct OrderFolder: View {
    @Bindable var model: DesktopModel
    let order: OrderRow
    let prefix: String

    var body: some View {
        HStack(spacing: 6) {
            Text("Folder")
                .foregroundStyle(.secondary)
            Text(order.directoryName)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .help(order.directoryName)
                .accessibleValue("Directory Name", value: order.directoryName, identifier: "\(prefix).directory")
            ShowOrderInFinder(model: model, row: order)
            if drifted {
                Button("Align Directory Name…") { model.changeOrderDirectory(.align) }
                    .controlSize(.small)
                    .disabled(!model.canChangeDirectory)
                    .accessibilityIdentifier("\(prefix).align")
            }
        }
        .font(.callout)
    }

    private var drifted: Bool {
        guard case let .healthy(details) = model.inspection else { return false }
        return details.drifted
    }
}

/// Where each Order fact sits, each labeled and shown in the control the editor supplies.
struct OrderFacts<Control: View>: View {
    @ViewBuilder let control: (Int32) -> Control

    var body: some View {
        Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 12, verticalSpacing: 8) {
            GridRow {
                label(SPIELORDERS_ORDER_FIELD_TITLE)
                control(SPIELORDERS_ORDER_FIELD_TITLE)
                    .gridCellColumns(3)
            }
            row(SPIELORDERS_ORDER_FIELD_DATE_ORDERED, SPIELORDERS_ORDER_FIELD_STATUS)
            row(SPIELORDERS_ORDER_FIELD_VENDOR, SPIELORDERS_ORDER_FIELD_CURRENCY)
            row(SPIELORDERS_ORDER_FIELD_ORDERER, SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION)
            row(SPIELORDERS_ORDER_FIELD_PROJECT, SPIELORDERS_ORDER_FIELD_ORDER_GROUP)
        }
        .frame(maxWidth: 900, alignment: .leading)
    }

    private func row(_ leading: Int32, _ trailing: Int32) -> some View {
        GridRow {
            label(leading)
            control(leading)
            label(trailing)
            control(trailing)
        }
    }

    private func label(_ field: Int32) -> some View {
        Text(RecordFieldLabel.of(field))
            .gridColumnAlignment(.trailing)
    }
}

/// The Order's Items under their heading, with the row actions and the Order's Total beneath.
struct OrderItems<Rows: View, Actions: View>: View {
    /// The Total as shown.
    let total: String
    let prefix: String
    @ViewBuilder let rows: Rows
    @ViewBuilder let actions: Actions

    var body: some View {
        Text("Items")
            .font(.headline)
            .accessibilityAddTraits(.isHeader)
        rows
        HStack {
            actions
            Spacer()
            Text("Total")
                .font(.headline)
            Text(total)
                .monospacedDigit()
                .accessibleValue("Total", value: total, identifier: "\(prefix).total")
        }
    }
}

/// The Order's Status among the library's statuses.
struct StatusPicker: View {
    @Binding var selection: String
    let identifier: String

    var body: some View {
        Picker("Status", selection: $selection) {
            ForEach(OrderStatus.all, id: \.name) { status in
                // Its own color keeps the choice legible while the picker is disabled.
                Text(status.label)
                    .foregroundStyle(.primary)
                    .tag(status.name)
            }
        }
        .labelsHidden()
        .pickerStyle(.menu)
        .fixedSize()
        .accessibilityLabel("Status")
        .accessibilityIdentifier(identifier)
    }
}

extension RowColumn {
    static var orderItems: [RowColumn] {
        [
            RowColumn(field: SPIELORDERS_ITEM_FIELD_CATALOG_NUMBER, width: 120),
            RowColumn(field: SPIELORDERS_ITEM_FIELD_DESCRIPTION, width: 420, flexible: true),
            RowColumn(field: SPIELORDERS_ITEM_FIELD_QUANTITY, width: 64, numeric: true),
            RowColumn(field: SPIELORDERS_ITEM_FIELD_UNIT_PRICE, width: 90, numeric: true, money: true),
            RowColumn(field: SPIELORDERS_ITEM_FIELD_TOTAL, width: 96, editable: false, numeric: true, money: true),
            RowColumn(field: SPIELORDERS_ITEM_FIELD_NOTES, width: 220, flexible: true, multiline: true,
                      optional: true),
        ]
    }
}
