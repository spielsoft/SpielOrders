import Foundation
import Testing
@testable import SpielOrdersKit

@Test(arguments: [false, true]) func guidanceRefresh(damagedMetadata: Bool) async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathExtension("splord")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: root) }
    let metadata = root.appendingPathComponent(".spielorders")
    if damagedMetadata {
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: false)
        try Data("not metadata".utf8).write(to: metadata.appendingPathComponent("config.toml"))
    }
    let instructions = root.appendingPathComponent("AGENTS.md")
    try "Owner instructions\n".write(to: instructions, atomically: true, encoding: .utf8)
    _ = try await AgentGuidance.provision(in: root, cancellation: LibraryCancellation())
    #expect(try String(contentsOf: instructions, encoding: .utf8) == "Owner instructions\n")
    let skill = root.appendingPathComponent(".agents/skills/spielorders/SKILL.md")
    let shipped = try Data(contentsOf: skill)
    #expect(!shipped.isEmpty)
    try Data("Earlier guidance".utf8).write(to: skill)
    _ = try await AgentGuidance.provision(in: root, cancellation: LibraryCancellation())
    #expect(try Data(contentsOf: skill) == shipped)
    if damagedMetadata {
        #expect(try Data(contentsOf: metadata.appendingPathComponent("config.toml")) == Data("not metadata".utf8))
    } else {
        #expect(!FileManager.default.fileExists(atPath: metadata.path))
    }
}

@Test func cancelledGuidanceWritesNothing() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: root) }
    let cancellation = LibraryCancellation()
    cancellation.request()
    await #expect(throws: CancellationError.self) {
        _ = try await AgentGuidance.provision(in: root, cancellation: cancellation)
    }
    #expect(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
}
