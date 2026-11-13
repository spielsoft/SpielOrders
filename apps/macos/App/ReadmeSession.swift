import Observation
import SpielOrdersKit

/// The README the window shows: the Package's, or the selected Order's. Swift keeps its
/// text beside the text last saved, and asks for a save once typing pauses.
@MainActor @Observable final class ReadmeSession {
    /// How long typing pauses before the text saves.
    private static let pause = Duration.milliseconds(1500)

    /// The Order whose README this is; nil for the Package README.
    let order: OrderRow?
    /// Whether the library could read the README; an unreadable one shows no text.
    let readable: Bool
    private(set) var draft: ReadmeDraft
    /// Whether the library can write the README as it was last read or saved.
    private(set) var writable: Bool
    /// Why the last save wrote nothing; its text stays, Not Saved.
    private(set) var failure: Notice?
    /// What the last save warned about, though it wrote the text.
    private(set) var warning: Notice?
    /// Asked once typing has paused.
    @ObservationIgnored var onPause: (() -> Void)?
    /// A record field that ended while this README's write owned the save gate.
    @ObservationIgnored var afterSave: (() -> Void)?
    @ObservationIgnored private var timer: Task<Void, Never>?

    init(order: OrderRow?, readme: ReadmeText) {
        self.order = order
        readable = readme.readable
        writable = readme.readable
        draft = ReadmeDraft(readme.text)
    }

    var refused: Bool { failure != nil }

    func edit(_ text: String) {
        guard text != draft.text else { return }
        draft.text = text
        waitForPause()
    }

    /// Asks for a save after the next pause, replacing any earlier wait.
    func waitForPause() {
        timer?.cancel()
        timer = Task { [weak self] in
            try? await Task.sleep(for: Self.pause)
            guard !Task.isCancelled else { return }
            self?.onPause?()
        }
    }

    /// The text to write, or nil when nothing is unsaved or a save is in flight.
    func submit() -> String? {
        timer?.cancel()
        return draft.submit()
    }

    func saved(_ result: ReadmeSaveResult) {
        draft.save(result.refreshedText)
        writable = result.canEdit
        failure = nil
        if !result.canEdit {
            warning = Notice("The README was saved, but it couldn’t be read again.", details: result.message)
        } else {
            warning = result.hasWarnings ? Notice("The README was saved with warnings.", details: result.message) : nil
        }
    }

    func refuse(_ failure: Notice) {
        timer?.cancel()
        draft.refuse()
        self.failure = failure
    }
}
