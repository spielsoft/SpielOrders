import Foundation

/// Date Ordered between the library's spelling, such as 2026-08-22, and a calendar date in
/// the user's time zone. The library validates the text; this only says whether the calendar
/// can show it.
public enum DateOrdered {
    /// The calendar date `text` spells, or nil when the calendar cannot show it exactly as stored.
    public static func date(from text: String) -> Date? {
        guard let date = try? spelling.parse(text), spelling.format(date) == text else { return nil }
        return date
    }

    /// The library's spelling of the calendar day `date` falls on.
    public static func text(for date: Date) -> String {
        spelling.format(date)
    }

    private static var spelling: Date.ISO8601FormatStyle {
        Date.ISO8601FormatStyle(timeZone: .current).year().month().day()
    }
}
