import CSpielOrders
import Darwin
import Foundation

enum SpotlightIndexState {
    case enabled
    case unavailable(Int32, String)

    @concurrent static func read(_ directory: URL) async -> SpotlightIndexState {
        if Task.isCancelled { return .unavailable(SPIELORDERS_PROVIDER_STATE_CANCELLED, "Search stopped.") }
        do {
            guard let volume = try directory.resourceValues(forKeys: [.volumeURLKey]).volume else {
                return .unavailable(SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED, "The volume couldn’t be identified.")
            }
            let process = Process()
            defer {
                if process.isRunning {
                    kill(process.processIdentifier, SIGKILL)
                    process.waitUntilExit()
                }
            }
            process.executableURL = URL(fileURLWithPath: "/usr/bin/mdutil")
            process.arguments = ["-s", volume.path]
            var environment = ProcessInfo.processInfo.environment
            environment["LC_ALL"] = "C"
            process.environment = environment
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = pipe
            defer {
                try? pipe.fileHandleForReading.close()
                try? pipe.fileHandleForWriting.close()
            }
            let descriptor = pipe.fileHandleForReading.fileDescriptor
            let flags = fcntl(descriptor, F_GETFL)
            guard flags >= 0, fcntl(descriptor, F_SETFL, flags | O_NONBLOCK) == 0 else {
                return .unavailable(SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED, "Spotlight’s status couldn’t be read.")
            }
            try process.run()
            try pipe.fileHandleForWriting.close()
            var bytes = Data()
            var buffer = [UInt8](repeating: 0, count: 4096)
            var overflow = false
            func drain() {
                // mdutil reports one volume. Bound unexpected output.
                for _ in 0..<16 {
                    let count = buffer.withUnsafeMutableBytes { Darwin.read(descriptor, $0.baseAddress, $0.count) }
                    if count < 0 && errno == EINTR { continue }
                    guard count > 0 else { break }
                    if bytes.count + count <= 65536 { bytes.append(contentsOf: buffer.prefix(count)) }
                    else { overflow = true }
                }
            }
            while true {
                drain()
                if !process.isRunning { break }
                if Task.isCancelled || overflow {
                    process.terminate()
                    let grace = ContinuousClock.now.advanced(by: .milliseconds(100))
                    while process.isRunning && ContinuousClock.now < grace { usleep(5_000) }
                    if process.isRunning { kill(process.processIdentifier, SIGKILL) }
                    process.waitUntilExit()
                    drain()
                    let state = Task.isCancelled ? SPIELORDERS_PROVIDER_STATE_CANCELLED : SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED
                    return .unavailable(
                        state,
                        Task.isCancelled ? "Search stopped." : "Spotlight’s status couldn’t be read.")
                }
                do { try await Task.sleep(for: .milliseconds(10)) } catch { /* Stop the owned process on the next iteration. */ }
            }
            process.waitUntilExit()
            drain()
            guard !overflow else {
                return .unavailable(SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED, "Spotlight’s status couldn’t be read.")
            }
            let report = String(decoding: bytes, as: UTF8.self)
            let state = report.split(separator: "\n").filter { $0.first?.isWhitespace == true }
                .map { $0.trimmingCharacters(in: .whitespaces) }.joined(separator: " ").lowercased()
            if state.contains("disabled") { return .unavailable(SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED, report) }
            if state.contains("enabled") { return .enabled }
            if state.contains("no index") { return .unavailable(SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED, report) }
            return .unavailable(SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED,
                                report.isEmpty ? "Index state wasn’t reported (exit \(process.terminationStatus))." : report)
        } catch {
            return .unavailable(SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED, error.localizedDescription)
        }
    }
}
