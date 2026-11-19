// swift-tools-version: 6.4
import PackageDescription

let package = Package(
    name: "SpielOrdersNative",
    platforms: [.macOS("26.0")],
    products: [.library(name: "SpielOrdersKit", targets: ["SpielOrdersKit"])],
    dependencies: [.package(url: "https://github.com/swiftlang/swift-markdown.git", from: "0.9.0")],
    targets: [
        .systemLibrary(name: "CSpielOrders"),
        .target(
            name: "SpielOrdersKit",
            dependencies: ["CSpielOrders", .product(name: "Markdown", package: "swift-markdown")],
            linkerSettings: [.linkedLibrary("iconv")]
        ),
        .testTarget(name: "SpielOrdersKitTests", dependencies: ["SpielOrdersKit", "CSpielOrders"]),
    ]
)
