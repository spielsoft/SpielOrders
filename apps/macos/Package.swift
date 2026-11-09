// swift-tools-version: 6.4
import PackageDescription

let package = Package(
    name: "SpielOrdersNative",
    platforms: [.macOS("26.0")],
    products: [.library(name: "SpielOrdersKit", targets: ["SpielOrdersKit"])],
    targets: [
        .systemLibrary(name: "CSpielOrders"),
        .target(
            name: "SpielOrdersKit", dependencies: ["CSpielOrders"],
            linkerSettings: [.linkedLibrary("iconv")]
        ),
        .testTarget(name: "SpielOrdersKitTests", dependencies: ["SpielOrdersKit", "CSpielOrders"]),
    ]
)
