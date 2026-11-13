import AppKit
import SpielOrdersKit
import SwiftUI

/// Date Ordered in the Order form: a native date field whose calendar opens from any of its
/// parts. An Order without a date, or with one the calendar cannot show, keeps its text as
/// stored until a date is chosen. The library validates whatever the field writes.
struct DateOrderedField: View {
    @Binding var text: String
    let identifier: String
    /// What the field says inside itself when a refused save left it blank.
    let problem: String?

    var body: some View {
        if let date = DateOrdered.date(from: text) {
            HStack(spacing: 4) {
                DatePickerField(date: date, label: "Date Ordered", identifier: identifier) {
                    text = DateOrdered.text(for: $0)
                }
                .fixedSize()
                clear
            }
        } else {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    stored
                    ChooseDateButton(identifier: "\(identifier).choose") {
                        text = DateOrdered.text(for: $0)
                    }
                    if !text.isEmpty {
                        clear
                    }
                }
                if !text.isEmpty {
                    Text("The calendar can’t show this date. Choose a date to replace it.")
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
            .foregroundStyle(text.isEmpty ? (problem == nil ? .secondary : Color.red) : .primary)
            .textSelection(.enabled)
            .accessibleValue("Date Ordered", value: text, identifier: identifier)
    }

    private var clear: some View {
        Button("Clear Date Ordered", systemImage: "xmark.circle.fill") { text = "" }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .help("Clear Date Ordered")
            .accessibilityIdentifier("\(identifier).clear")
    }
}

/// Chooses a date from a calendar in a popover, for a field that holds no date it can show.
private struct ChooseDateButton: View {
    let identifier: String
    let choose: (Date) -> Void
    @State private var presented = false
    @State private var date = Date()

    var body: some View {
        Button("Choose Date…", systemImage: "calendar") {
            date = Date()
            presented = true
        }
        .controlSize(.small)
        .accessibilityIdentifier(identifier)
        .popover(isPresented: $presented, arrowEdge: .bottom) {
            DatePicker("Date Ordered", selection: $date, displayedComponents: .date)
                .datePickerStyle(.graphical)
                .labelsHidden()
                .padding(8)
                .onChange(of: date) { _, chosen in
                    presented = false
                    choose(chosen)
                }
        }
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
