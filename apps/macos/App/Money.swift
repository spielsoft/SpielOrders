import Foundation

/// Money as the app shows it: a library amount with at least two decimals and every decimal
/// it has, in the system's currency style when the Order's currency is an ISO code. Display
/// never rounds or changes the amount the library stores or calculates. A number the system
/// cannot format exactly keeps its literal spelling, with the currency's native affixes.
enum Money {
    private static let codes = Set(Locale.Currency.isoCurrencies.map(\.identifier))

    static func display(_ amount: String, currency: String) -> String {
        guard let spelling = Spelling(amount) else { return amount }
        let code = currency.trimmingCharacters(in: .whitespaces).uppercased()
        // Decimal's initializer may succeed after silently discarding significant digits.
        // Compare decimal spellings, not floating-point values, before trusting that conversion.
        guard let value = Decimal(string: amount, locale: Locale(identifier: "en_US_POSIX")),
              Spelling(NSDecimalNumber(decimal: value).stringValue) == spelling else {
            return literal(amount, currency: code)
        }
        let decimals = spelling.exponent < 0 ? -spelling.exponent : 0
        let precision = NumberFormatStyleConfiguration.Precision.fractionLength(2...max(2, decimals))
        if codes.contains(code) {
            return value.formatted(.currency(code: code).precision(precision))
        }
        return value.formatted(.number.precision(precision))
    }

    /// Retains all digits without asking a numeric formatter to represent the amount.
    private static func literal(_ amount: String, currency: String) -> String {
        guard codes.contains(currency) else { return amount }
        let style = NumberFormatter()
        style.locale = .autoupdatingCurrent
        style.numberStyle = .currency
        style.currencyCode = currency
        let negative = amount.hasPrefix("-")
        let unsigned = amount.hasPrefix("-") || amount.hasPrefix("+") ? String(amount.dropFirst()) : amount
        return (negative ? style.negativePrefix : style.positivePrefix) + unsigned
            + (negative ? style.negativeSuffix : style.positiveSuffix)
    }

    /// A decimal's significant digits and power of ten. This compares spellings only;
    /// parsing and calculation of an Item's numeric value remain library responsibilities.
    private struct Spelling: Equatable {
        let negative: Bool
        let digits: String
        let exponent: Int

        init?(_ text: String) {
            guard let match = text.wholeMatch(of: /([+-]?)(?:([0-9]+)(?:\.([0-9]*))?|\.([0-9]+))(?:[eE]([+-]?[0-9]+))?/) else {
                return nil
            }
            let fraction = match.output.4 ?? match.output.3 ?? ""
            var digits = String((String(match.output.2 ?? "") + fraction).drop(while: { $0 == "0" }))
            if digits.isEmpty {
                self.negative = false
                self.digits = "0"
                self.exponent = 0
                return
            }
            guard let writtenExponent = Int(match.output.5 ?? "0") else { return nil }
            let (power, overflow) = writtenExponent.subtractingReportingOverflow(fraction.count)
            guard !overflow else { return nil }
            var exponent = power
            while digits.last == "0" {
                digits.removeLast()
                let (next, overflow) = exponent.addingReportingOverflow(1)
                guard !overflow else { return nil }
                exponent = next
            }
            negative = match.output.1 == "-"
            self.digits = digits
            self.exponent = exponent
        }
    }
}
