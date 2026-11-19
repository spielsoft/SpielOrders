import Observation

/// Text being entered for a name. This is an editor buffer, not a name validator: the
/// Package still decides what the submitted spelling means and whether it is accepted.
@MainActor @Observable public final class NameDraft {
    public var text: String
    private var baseline: String?
    private var submitted: String?
    public private(set) var refused = false

    /// `original` is nil for a new name, whose starting text may be selected for replacement.
    public init(_ text: String, original: String?) {
        self.text = text
        baseline = original
    }

    public var dirty: Bool {
        guard let baseline else { return !text.allSatisfy(\.isWhitespace) }
        return !text.utf8.elementsEqual(baseline.utf8)
    }

    public var saving: Bool { submitted != nil }

    /// Takes the text once; ending focus twice never submits the same write twice.
    public func submit() -> String? {
        guard dirty, submitted == nil else { return nil }
        submitted = text
        refused = false
        return text
    }

    /// The name was committed. Follow the returned spelling, or the submitted text when
    /// readback was unavailable; any later input remains unsaved.
    public func saved(_ name: String?) {
        if let submitted {
            let written = name ?? submitted
            if text.utf8.elementsEqual(submitted.utf8) { text = written }
            baseline = written
        }
        submitted = nil
        refused = false
    }

    /// A refusal keeps the text for correction or another explicit attempt.
    public func refuse() {
        submitted = nil
        refused = true
    }

    /// Escape or a declined confirmation restores the original spelling.
    public func cancel() {
        text = baseline ?? ""
        submitted = nil
        refused = false
    }

    /// Follows the refreshed name only while there is no input left to protect.
    public func read(_ name: String) {
        guard !dirty, !saving else { return }
        baseline = name
        text = name
    }
}
