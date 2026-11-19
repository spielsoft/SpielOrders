import AppKit
import SpielOrdersKit
import SwiftUI

/// Date Ordered in the Order form: a native date field that takes a typed date or opens its
/// calendar from any of its parts. Every Order requires a date, so the field has no way to
/// empty it. An Order without a date, or with one the calendar cannot show, keeps its text as
/// stored until Enter Date starts the field from today. The library validates whatever the
/// field writes.
struct DateOrderedField: View {
    @Binding var text: String
    let identifier: String
    /// What the field says inside itself when a refused save left it blank.
    let problem: String?

    var body: some View {
        if let date = DateOrdered.date(from: text) {
            DatePickerField(date: date, label: "Date Ordered", identifier: identifier) {
                text = DateOrdered.text(for: $0)
            }
            .fixedSize()
        } else {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    stored
                    Button("Enter Date") { text = DateOrdered.text(for: Date()) }
                        .controlSize(.small)
                        .help("Start Date Ordered from today, then type or choose the date")
                        .accessibilityIdentifier("\(identifier).enter")
                }
                if !text.isEmpty {
                    Text("The calendar can’t show this date. Enter a date to replace it.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 4)
        }
    }

    /// The date as stored, or what an empty field says.
    private var stored: some View {
        let placeholder = problem ?? "No Date"
        return Text(text.isEmpty ? placeholder : text)
            .foregroundStyle(text.isEmpty ? (problem == nil ? .secondary : Color.problemText) : .primary)
            .textSelection(.enabled)
            .accessibleValue("Date Ordered", value: text.isEmpty ? placeholder : text, identifier: identifier)
    }
}

/// AppKit's date field with its stepper, presenting the calendar overlay when a part is
/// clicked. Each change is reported as it is made.
private struct DatePickerField: NSViewRepresentable {
    let date: Date
    let label: String
    let identifier: String
    let choose: (Date) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(choose: choose) }

    func makeNSView(context: Context) -> NSDatePicker {
        let picker = NSDatePicker()
        picker.datePickerStyle = .textFieldAndStepper
        picker.datePickerElements = .yearMonthDay
        picker.presentsCalendarOverlay = true
        picker.calendar = .current
        picker.timeZone = .current
        picker.target = context.coordinator
        picker.action = #selector(Coordinator.changed(_:))
        picker.setAccessibilityLabel(label)
        picker.setAccessibilityIdentifier(identifier)
        return picker
    }

    func updateNSView(_ picker: NSDatePicker, context: Context) {
        context.coordinator.choose = choose
        picker.isEnabled = context.environment.isEnabled
        if picker.dateValue != date {
            picker.dateValue = date
        }
    }

    @MainActor final class Coordinator: NSObject {
        var choose: (Date) -> Void

        init(choose: @escaping (Date) -> Void) {
            self.choose = choose
        }

        @objc func changed(_ sender: NSDatePicker) {
            choose(sender.dateValue)
        }
    }
}

/// Date Ordered as lists and headings show it: the system's short date style, or the text as
/// stored when the calendar cannot show it. The library still sorts by its own spelling.
@MainActor enum ShortDate {
    private static let style: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = .autoupdatingCurrent
        formatter.dateStyle = .short
        formatter.timeStyle = .none
        return formatter
    }()

    static func display(_ text: String) -> String {
        guard let date = DateOrdered.date(from: text) else { return text }
        return style.string(from: date)
    }
}
