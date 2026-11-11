import Foundation

/// Native permission handoff only. Domain operations still call linked Rust directly.
enum AgentAuthorizationContract {
    static let applicationExecutable = "SpielOrdersCLIAuthorization"
    static let relativeBundle = "Contents/Helpers/SpielOrdersCLI.app"
    static let relativeBackend = "Contents/Helpers/spielorders-backend"
}

/// Written to the GUI's disposable reply file before the independently launched app exits.
enum AgentAuthorizationResult: Codable {
    case authorized
    case cancelled
    case failed(String)
}

struct AgentAuthorizationRequest: Codable {
    let packageBookmark: Data
    let replyBookmark: Data
}
