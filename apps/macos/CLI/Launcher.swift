import AppKit
import Darwin
import Foundation
import OSLog

/// Restore this executable's own folder grants, then exec the portable Rust CLI.
/// The GUI alone invokes the separate authorization mode; ordinary commands never prompt.
@main @MainActor enum AgentLauncher {
    private static let bookmarkPrefix = "authorizedPackage."
    private static var reply: URL?
    private static var cancellation: DispatchSourceFileSystemObject?

    static func main() {
        do {
            try enterThroughBundle()
            // Both entry paths resolve to this same signed executable. The public CLI
            // alias never enters AppKit, even with no arguments; argv is kept intact.
            if URL(fileURLWithPath: CommandLine.arguments[0]).lastPathComponent == AgentAuthorizationContract.applicationExecutable {
                runAuthorization()
            }
            let grants = restoreGrants()
            let backend = Bundle.main.bundleURL.appendingPathComponent(AgentAuthorizationContract.relativeBackend)
            // exec retains this process's sandbox and active scopes, not the GUI's.
            let result = withExtendedLifetime(grants) {
                backend.withUnsafeFileSystemRepresentation { path in
                    path.map { execv($0, CommandLine.unsafeArgv) } ?? -1
                }
            }
            throw NSError(domain: NSPOSIXErrorDomain, code: result == -1 ? Int(errno) : Int(EIO))
        } catch { finish(error) }
    }

    /// The bundle, and with it the saved grants and the backend, is found from the path
    /// this process was executed by. A link placed on PATH hides it, so run again from
    /// the real executable before anything reads the bundle. argv is kept intact.
    private static func enterThroughBundle() throws {
        var size: UInt32 = 0
        _ = _NSGetExecutablePath(nil, &size)
        var executed = [CChar](repeating: 0, count: Int(size))
        guard _NSGetExecutablePath(&executed, &size) == 0, let real = realpath(executed, nil) else { return }
        defer { free(real) }
        let directory = { (path: UnsafePointer<CChar>) in
            URL(fileURLWithFileSystemRepresentation: path, isDirectory: false, relativeTo: nil)
                .deletingLastPathComponent().path
        }
        guard directory(executed) != directory(real) else { return }
        execv(real, CommandLine.unsafeArgv)
        throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
    }

    private static func restoreGrants() -> [URL] {
        UserDefaults.standard.dictionaryRepresentation().compactMap { key, value in
            guard key.hasPrefix(bookmarkPrefix), let data = value as? Data else { return nil }
            var stale = false
            guard let url = try? URL(resolvingBookmarkData: data, options: [.withSecurityScope, .withoutUI],
                                     bookmarkDataIsStale: &stale), !stale,
                  url.startAccessingSecurityScopedResource() else { return nil }
            return url
        }
    }

    private static func sameDirectory(_ left: URL, _ right: URL) -> Bool {
        var relationship: FileManager.URLRelationship = .other
        return (try? FileManager.default.getRelationship(&relationship, ofDirectoryAt: left, toItemAt: right)) != nil
            && relationship == .same
    }

    private static var authorizationDelegate: AuthorizationDelegate?

    private static func runAuthorization() -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let delegate = AuthorizationDelegate()
        authorizationDelegate = delegate
        app.delegate = delegate
        app.run()
        finish(AccessError.interrupted)
    }

    fileprivate static func openAuthorization(_ urls: [URL]) {
        do {
            guard reply == nil, urls.count == 1, let document = urls.first else { throw AccessError.invalidRequest }
            let request = try PropertyListDecoder().decode(AgentAuthorizationRequest.self, from: Data(contentsOf: document))
            var stale = false
            reply = try URL(resolvingBookmarkData: request.replyBookmark, options: [.withoutUI], bookmarkDataIsStale: &stale)
            // Cross-container resolution can mark this transient bookmark stale even
            // though it grants the correct reply file. It is used once, never saved.
            try observeCancellation(of: document)
            let expected = try URL(resolvingBookmarkData: request.packageBookmark, options: [.withoutUI], bookmarkDataIsStale: &stale)
            guard !stale else { throw AccessError.renew }
            let grants = restoreGrants()
            if grants.contains(where: { sameDirectory($0, expected) }) { complete(.authorized) }
            authorize(expected)
        } catch { finish(error) }
    }

    private static func observeCancellation(of document: URL) throws {
        let descriptor = document.withUnsafeFileSystemRepresentation { path in
            path.map { Darwin.open($0, O_EVTONLY) } ?? -1
        }
        guard descriptor >= 0 else { throw AccessError.interrupted }
        let source = DispatchSource.makeFileSystemObjectSource(fileDescriptor: descriptor,
                                                               eventMask: .delete, queue: .main)
        source.setEventHandler { Task { @MainActor in complete(.cancelled) } }
        source.setCancelHandler { close(descriptor) }
        cancellation = source
        source.resume()
        // Deletion may race with registering the watcher. An already-unlinked
        // descriptor has no future delete event to deliver.
        var information = stat()
        guard fstat(descriptor, &information) == 0 else { throw AccessError.interrupted }
        if information.st_nlink == 0 { complete(.cancelled) }
    }

    private static func authorize(_ expected: URL) {
        let panel = NSOpenPanel()
        panel.title = "Authorize SpielOrders CLI"
        panel.message = "Choose this Package folder to let agents use the bundled CLI while SpielOrders is closed."
        panel.prompt = "Authorize CLI"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.directoryURL = expected
        panel.setAccessibilityLabel("Authorize SpielOrders CLI")
        panel.setAccessibilityIdentifier("agent.authorization.panel")
        // Modeless presentation keeps the Dock and other applications usable.
        panel.begin { response in
            if response == .cancel { complete(.cancelled) }
            guard response == .OK, let selected = panel.url else { finish(AccessError.interrupted) }
            do {
                guard sameDirectory(selected, expected) else { throw AccessError.wrongFolder }
                let data = try selected.bookmarkData(options: [.withSecurityScope], includingResourceValuesForKeys: nil, relativeTo: nil)
                guard let name = selected.withUnsafeFileSystemRepresentation({ bytes in
                    bytes.map { Data(bytes: $0, count: strlen($0)).base64EncodedString() }
                }) else { throw AccessError.storeFailed }
                UserDefaults.standard.set(data, forKey: bookmarkPrefix + name)
                guard UserDefaults.standard.synchronize() else { throw AccessError.storeFailed }
                complete(.authorized)
            } catch { finish(error) }
        }
        NSApplication.shared.activate()
    }

    private static func complete(_ result: AgentAuthorizationResult) -> Never {
        do {
            guard let reply else { throw AccessError.invalidRequest }
            // The GUI reads only after termination. Writing directly needs access to
            // this reply file alone, not its containing directory or the GUI container.
            try PropertyListEncoder().encode(result).write(to: reply)
        } catch {
            FileHandle.standardError.write(Data(("Authorization reply failed: " + error.localizedDescription + "\n").utf8))
            exit(1)
        }
        switch result {
        case .authorized: exit(0)
        case .cancelled: exit(2)
        case .failed: exit(1)
        }
    }

    private static func finish(_ error: Error) -> Never {
        Logger(subsystem: Bundle.main.bundleIdentifier ?? "SpielOrders.CLI", category: "authorization").error("Authorization error: \(error.localizedDescription, privacy: .public)")
        if reply != nil { complete(.failed(error.localizedDescription)) }
        FileHandle.standardError.write(Data(("spielorders: " + error.localizedDescription + "\n").utf8))
        exit(1)
    }
}

private enum AccessError: LocalizedError {
    case invalidRequest, renew, wrongFolder, storeFailed, interrupted
    var errorDescription: String? {
        switch self {
        case .invalidRequest: "The CLI authorization request is invalid."
        case .renew: "Renew CLI access with Agent Ready."
        case .wrongFolder: "Choose the requested Package folder; no Package operation was run."
        case .storeFailed: "The CLI folder grant could not be saved; no Package operation was run."
        case .interrupted: "CLI authorization ended without a folder grant."
        }
    }
}

@MainActor private final class AuthorizationDelegate: NSObject, NSApplicationDelegate {
    func application(_ application: NSApplication, open urls: [URL]) {
        AgentLauncher.openAuthorization(urls)
    }
}
