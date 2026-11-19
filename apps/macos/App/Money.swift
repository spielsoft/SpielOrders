import Foundation

/// Money as the app shows it: a library amount rounded for display to two decimals, in the
/// system's currency style when the Order's currency is an ISO code and as a plain number
/// otherwise. Display never changes the amount the library stores or calculates; a cell being
/// edited shows its amount as typed.
enum Money {
    private static let codes = Set(Locale.Currency.isoCurrencies.map(\.identifier))
    private static let precision = NumberFormatStyleConfiguration.Precision.fractionLength(2)
    private static let rounding = FloatingPointRoundingRule.toNearestOrAwayFromZero

    static func display(_ amount: String, currency: String) -> String {
        guard let value = Decimal(string: amount, locale: Locale(identifier: "en_US_POSIX")) else {
            return amount
        }
        let code = currency.trimmingCharacters(in: .whitespaces).uppercased()
        if codes.contains(code) {
            return value.formatted(.currency(code: code).precision(precision).rounded(rule: rounding))
        }
        return value.formatted(.number.precision(precision).rounded(rule: rounding))
    }
}
