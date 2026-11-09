import Foundation

/// Filesystem bytes for C calls; the URL is only for native presentation and services.
struct FileSystemPath: Sendable {
    private let storage: [CChar]

    init(bytes: [UInt8]) throws {
        guard !bytes.isEmpty, !bytes.contains(0) else {
            throw CocoaError(.fileReadInvalidFileName)
        }
        storage = bytes.map { CChar(bitPattern: $0) } + [0]
    }

    init(fileSystemRepresentation: UnsafePointer<CChar>) {
        storage = Array(UnsafeBufferPointer(start: fileSystemRepresentation,
                                           count: strlen(fileSystemRepresentation) + 1))
    }

    init(url: URL) throws {
        guard url.isFileURL else { throw CocoaError(.fileReadUnsupportedScheme) }
        self = try url.withUnsafeFileSystemRepresentation { pointer in
            guard let pointer else { throw CocoaError(.fileReadInvalidFileName) }
            return FileSystemPath(fileSystemRepresentation: pointer)
        }
    }

    var url: URL {
        withUnsafeFileSystemRepresentation {
            URL(fileURLWithFileSystemRepresentation: $0, isDirectory: false, relativeTo: nil)
        }
    }

    func withUnsafeFileSystemRepresentation<T>(_ body: (UnsafePointer<CChar>) throws -> T) rethrows -> T {
        try storage.withUnsafeBufferPointer { try body($0.baseAddress!) }
    }
}
