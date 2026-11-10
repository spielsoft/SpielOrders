import AppKit
import SwiftUI

struct AboutView: View {
    @Environment(\.colorScheme) private var colorScheme
    static let windowID = "about"
    static let sourceCodeURL = URL(string: "https://github.com/spielsoft/SpielOrders")!

    private var versionText: String {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String ?? "?"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "Version \(version) (\(build))"
    }

    private var secondaryText: Color {
        colorScheme == .dark ? .white.opacity(0.70) : .black.opacity(0.62)
    }

    var body: some View {
        VStack(spacing: 4) {
            Text("A SpielSoft product")
                .font(.callout)
                .foregroundStyle(secondaryText)
                .padding(.bottom, 12)
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 96, height: 96)
                .accessibilityHidden(true)
            Text("SpielOrders")
                .font(.title.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
                .padding(.top, 4)
            Text(versionText)
                .font(.callout)
                .foregroundStyle(secondaryText)
                .textSelection(.enabled)

            Text("© 2026 SpielSoft")
                .font(.footnote)
                .foregroundStyle(secondaryText)
                .padding(.top, 16)
            Link("Source code", destination: Self.sourceCodeURL)
                .font(.footnote)
                .accessibilityIdentifier("about.sourceCode")
        }
        .padding(.horizontal, 40)
        .padding(.vertical, 28)
        .frame(width: 320)
        .fixedSize()
    }
}
