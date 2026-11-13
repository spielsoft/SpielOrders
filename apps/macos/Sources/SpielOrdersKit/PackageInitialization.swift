import Foundation

public struct PackageInitialization: Sendable {
    public let readmeText: String?
    public let canBrowseRecords: Bool
    public let message: String
    public let notices: [String]
}
