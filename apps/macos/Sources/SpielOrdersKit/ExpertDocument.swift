import CSpielOrders
import Darwin
import Foundation

public enum ManagedJSON: String, Sendable {
    case version = "version.json", vendors = "vendors.json", order = "order.json"
}

/// The desktop decision's expert exception. Text is literal, not a repair proposal.
public actor ExpertDocument {
    public nonisolated let text: String
    public nonisolated let location: URL
    private let root: FileSystemPath
    private let components: [[UInt8]]
    private let filename: String
    private let parent: Int32
    private var original: Data

    init(root: FileSystemPath, directory: FileSystemPath?, artifact: ManagedJSON) throws {
        self.root = root
        filename = artifact.rawValue
        var components: [[UInt8]] = []
        if artifact == .order {
            guard let directory else { throw ExpertDocumentError.unsafeLocation }
            let rootBytes = root.withUnsafeFileSystemRepresentation { Array(UnsafeBufferPointer(start: $0, count: strlen($0))).map { UInt8(bitPattern: $0) } }
            let directoryBytes = directory.withUnsafeFileSystemRepresentation { Array(UnsafeBufferPointer(start: $0, count: strlen($0))).map { UInt8(bitPattern: $0) } }
            let prefix = rootBytes + [47]
            guard directoryBytes.starts(with: prefix) else { throw ExpertDocumentError.unsafeLocation }
            components = directoryBytes.dropFirst(prefix.count).split(separator: 47).map(Array.init)
            guard !components.isEmpty, components.allSatisfy({ $0 != [46] && $0 != [46, 46] }) else {
                throw ExpertDocumentError.unsafeLocation
            }
        } else if directory != nil { throw ExpertDocumentError.unsafeLocation }
        components.append(Array(".spielorders".utf8))
        self.components = components
        let parent = try Self.openParent(root: root, components: components)
        do {
            let bytes = try Self.read(parent: parent, filename: artifact.rawValue).0
            guard let text = String(data: bytes, encoding: .utf8) else { throw CocoaError(.fileReadInapplicableStringEncoding) }
            self.text = text
            original = bytes
            self.parent = parent
            location = (directory?.url ?? root.url).appendingPathComponent(".spielorders").appendingPathComponent(artifact.rawValue)
        } catch { close(parent); throw error }
    }

    deinit { close(parent) }

    /// Writes atomically only if the document and its physical parent still match.
    /// After replacement, durability failures are warnings, never a pretend rejection.
    public func write(_ text: String) throws -> [String] {
        try Task.checkCancellation()
        try checkParent()
        let (currentBytes, mode) = try Self.read(parent: parent, filename: filename)
        guard currentBytes == original else { throw ExpertDocumentError.changed }
        let stage = ".\(filename).expert-\(UUID().uuidString)"
        let descriptor = openat(parent, stage, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, mode_t(0o600))
        guard descriptor >= 0 else { throw Self.posixError() }
        defer { close(descriptor) }
        let data = Data(text.utf8)
        do {
            try data.withUnsafeBytes { bytes in
                var offset = 0
                while offset < bytes.count {
                    let count = Darwin.write(descriptor, bytes.baseAddress!.advanced(by: offset), bytes.count - offset)
                    if count < 0, errno == EINTR { continue }
                    guard count > 0 else { throw Self.posixError() }
                    offset += count
                }
            }
            guard fchmod(descriptor, mode & 0o777) == 0, fcntl(descriptor, F_FULLFSYNC) == 0 else { throw Self.posixError() }
            try Task.checkCancellation()
            try checkParent()
            guard try Self.read(parent: parent, filename: filename).0 == original else { throw ExpertDocumentError.changed }
            guard renameat(parent, stage, parent, filename) == 0 else { throw Self.posixError() }
        } catch {
            if unlinkat(parent, stage, 0) != 0, errno != ENOENT {
                let retained = location.deletingLastPathComponent().appendingPathComponent(stage)
                throw ExpertDocumentError.cleanup(error.localizedDescription, retained.path, Self.posixError().localizedDescription)
            }
            throw error
        }
        original = data
        return fsync(parent) == 0 ? [] : ["JSON was written, but directory durability could not be confirmed: \(Self.posixError().localizedDescription)"]
    }

    private func checkParent() throws {
        let currentParent = try Self.openParent(root: root, components: components)
        defer { close(currentParent) }
        var before = stat()
        var current = stat()
        guard fstat(parent, &before) == 0, fstat(currentParent, &current) == 0,
              before.st_dev == current.st_dev, before.st_ino == current.st_ino else {
            throw ExpertDocumentError.changed
        }
    }

    private static func openParent(root: FileSystemPath, components: [[UInt8]]) throws -> Int32 {
        var descriptor = root.withUnsafeFileSystemRepresentation { Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW) }
        guard descriptor >= 0 else { throw posixError() }
        do {
            for component in components {
                let next = (component + [0]).withUnsafeBytes {
                    openat(descriptor, $0.baseAddress!.assumingMemoryBound(to: CChar.self), O_RDONLY | O_DIRECTORY | O_NOFOLLOW)
                }
                guard next >= 0 else { throw posixError() }
                close(descriptor)
                descriptor = next
            }
            return descriptor
        } catch { close(descriptor); throw error }
    }

    private static func read(parent: Int32, filename: String) throws -> (Data, mode_t) {
        let descriptor = openat(parent, filename, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)
        guard descriptor >= 0 else { throw posixError() }
        defer { close(descriptor) }
        var info = stat()
        guard fstat(descriptor, &info) == 0 else { throw posixError() }
        guard info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG) else { throw ExpertDocumentError.unsafeLocation }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 65536)
        while true {
            let count = Darwin.read(descriptor, &buffer, buffer.count)
            if count < 0, errno == EINTR { continue }
            guard count >= 0 else { throw posixError() }
            if count == 0 { return (data, info.st_mode) }
            data.append(contentsOf: buffer.prefix(count))
        }
    }

    private static func posixError() -> NSError { NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
}

private enum ExpertDocumentError: LocalizedError {
    case changed, unsafeLocation
    case cleanup(String, String, String)
    var errorDescription: String? {
        switch self {
        case .changed: "The raw document changed on disk. Your draft is retained; reopen it before writing."
        case .unsafeLocation: "Raw correction requires an existing regular managed JSON file, with no linked metadata directories."
        case let .cleanup(failure, path, reason): "\(failure)\nThe uncommitted staging file remains at \(path). Cleanup failed: \(reason)"
        }
    }
}

extension PackageSession {
    public func expertDocument(directory: URL?, artifact: ManagedJSON) throws -> ExpertDocument {
        guard let package else { throw SessionAccessError.reopenRequired }
        let root = FileSystemPath(fileSystemRepresentation: spielorders_package_root(package.pointer))
        return try ExpertDocument(root: root, directory: directory.map(FileSystemPath.init(url:)), artifact: artifact)
    }

    public func revalidateAfterExpertWrite(directory: URL?) throws -> PackageDiagnosis {
        guard let package else { throw SessionAccessError.reopenRequired }
        let notices = refreshPackageAfterCommit(package)
        guard self.package != nil else { throw LibraryFailure(result: SPIELORDERS_RESULT_REFUSED, entries: notices) }
        return try diagnose(directory: directory, cancellation: LibraryCancellation())
    }
}
