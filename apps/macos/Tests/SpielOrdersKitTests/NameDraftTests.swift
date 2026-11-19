import SpielOrdersKit
import Testing

// Decision source: Prototype the SpielOrders desktop experience, value names and guarded navigation.

@Test @MainActor func anEndedNameIsSubmittedOnceAndKeepsLaterInput() {
    let draft = NameDraft("Ada", original: "Ada")
    draft.text = "Grace"
    #expect(draft.dirty)
    #expect(draft.submit() == "Grace")
    #expect(draft.submit() == nil)
    draft.text = "Katherine"
    draft.saved("Grace")
    #expect(draft.dirty)
    #expect(draft.submit() == "Katherine")
    draft.saved("Katherine")
    #expect(!draft.dirty)
}

@Test @MainActor func aRefusedNameKeepsItsInputUntilCorrectedOrCancelled() {
    let draft = NameDraft("Ada", original: "Ada")
    draft.text = "  "
    #expect(draft.submit() == "  ")
    draft.refuse()
    #expect(draft.dirty)
    #expect(draft.refused)
    #expect(draft.text == "  ")
    draft.text = "Grace"
    #expect(draft.submit() == "Grace")
    #expect(!draft.refused)
    draft.refuse()
    draft.read("A refreshed name")
    #expect(draft.text == "Grace")
    draft.cancel()
    #expect(!draft.dirty)
    #expect(draft.text == "Ada")
    draft.read("A refreshed name")
    #expect(draft.text == "A refreshed name")
    #expect(!draft.dirty)
}

@Test @MainActor func aNewNameIsPendingUntilCommittedOrCancelled() {
    let draft = NameDraft("New Project", original: nil)
    #expect(draft.dirty)
    #expect(draft.submit() == "New Project")
    draft.saved("New Project")
    #expect(!draft.dirty)
    let empty = NameDraft("  ", original: nil)
    #expect(empty.submit() == nil)
    let cancelled = NameDraft("Optics", original: nil)
    cancelled.cancel()
    #expect(!cancelled.dirty)
}

@Test @MainActor func aSavedNameFollowsTheLibrarySpellingWithoutLosingLaterInput() {
    let draft = NameDraft("Ada", original: "Ada")
    draft.text = "  Mira  "
    #expect(draft.submit() == "  Mira  ")
    draft.saved("Mira")
    #expect(draft.text == "Mira")
    #expect(!draft.dirty)
    draft.text = "  Jo  "
    #expect(draft.submit() == "  Jo  ")
    draft.text = "Next name"
    draft.saved("Jo")
    #expect(draft.text == "Next name")
    #expect(draft.dirty)
}
