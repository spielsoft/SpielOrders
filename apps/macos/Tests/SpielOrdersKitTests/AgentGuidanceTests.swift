import Foundation
import Testing
@testable import SpielOrdersKit

@Test func provisioningGuidanceThroughTheSessionPreservesTheOwnersInstructions() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let instructions = root.appendingPathComponent("AGENTS.md")
    try "Owner instructions\n".write(to: instructions, atomically: true, encoding: .utf8)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let setup = try await session.provisionAgentGuidance(cancellation: LibraryCancellation())
    #expect(setup.message.contains("created"))
    #expect(setup.prompt.contains(".agents/skills/spielorders/SKILL.md"))
    #expect(try String(contentsOf: instructions, encoding: .utf8) == "Owner instructions\n")
    let repeated = try await session.provisionAgentGuidance(cancellation: LibraryCancellation())
    #expect(repeated.message.contains("already"))
}
