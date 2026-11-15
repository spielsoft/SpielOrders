/// A README's text in its editor beside the text the library last read or saved. A save
/// writes the text submitted when it began; input typed while it is written stays unsaved
/// and is never replaced by that save's result.
public struct ReadmeDraft: Sendable {
    public var text: String
    private var saved: String
    /// The text a save in flight is writing.
    private var submitted: String?

    public init(_ saved: String) {
        text = saved
        self.saved = saved
    }

    public var unsaved: Bool { !text.utf8.elementsEqual(saved.utf8) }
    public var saving: Bool { submitted != nil }

    /// Begins saving the current text: the text to write, or nil when nothing is unsaved or
    /// a save is already in flight.
    public mutating func submit() -> String? {
        guard unsaved, !saving else { return nil }
        submitted = text
        return text
    }

    /// Ends the save in flight with the README the library read back after writing it, or
    /// with nil when it could not be read back. Input typed meanwhile is kept, still unsaved.
    public mutating func save(_ refreshed: String?) {
        guard let submitted else { return }
        let written = refreshed ?? submitted
        if text.utf8.elementsEqual(submitted.utf8) { text = written }
        saved = written
        self.submitted = nil
    }

    /// Ends the save in flight without a write; every input stays, unsaved.
    public mutating func refuse() {
        submitted = nil
    }
}
