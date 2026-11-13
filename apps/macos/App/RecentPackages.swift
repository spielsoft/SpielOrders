import Foundation
import Observation

/// A Package opened recently. Its security-scoped bookmark lets the sandboxed app open it
/// again without a folder panel.
struct RecentPackage: Identifiable, Equatable {
    /// The folder's path when it was last opened, which tells two entries apart.
    let id: String
    let name: String
    let bookmark: Data
}

/// The Packages opened most recently, newest first, which File → Open Recent and the welcome
/// view list. They are kept in the app's preferences, apart from any Package.
@MainActor @Observable final class RecentPackages {
    private static let key = "recentPackages"
    private static let limit = 10
    private(set) var entries: [RecentPackage]

    init() {
        let stored = UserDefaults.standard.array(forKey: Self.key) as? [[String: Any]] ?? []
        entries = stored.compactMap { entry in
            guard let path = entry["path"] as? String, let name = entry["name"] as? String,
                  let bookmark = entry["bookmark"] as? Data else { return nil }
            return RecentPackage(id: path, name: name, bookmark: bookmark)
        }
    }

    /// Puts the Package at `url` first, replacing an earlier entry for the same folder.
    func note(_ url: URL, bookmark: Data) {
        let entry = RecentPackage(id: url.path, name: url.packageName, bookmark: bookmark)
        let others = entries.filter { $0.id != entry.id }
        entries = Array(([entry] + others).prefix(Self.limit))
        save()
    }

    func remove(_ entry: RecentPackage) {
        entries.removeAll { $0.id == entry.id }
        save()
    }

    func clear() {
        entries = []
        save()
    }

    private func save() {
        let stored = entries.map { ["path": $0.id, "name": $0.name, "bookmark": $0.bookmark] as [String: Any] }
        UserDefaults.standard.set(stored, forKey: Self.key)
    }
}

extension URL {
    /// A Package folder's name, without its `.splord` extension.
    var packageName: String {
        pathExtension == "splord" ? deletingPathExtension().lastPathComponent : lastPathComponent
    }
}
