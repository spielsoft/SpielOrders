import SpielOrdersKit
import Testing

// Decision source: Prototype the SpielOrders desktop experience, Order and Vendor editing.
@Test func readmeDraftSave() {
    var draft = ReadmeDraft("Saved")
    #expect(!draft.unsaved)
    #expect(draft.submit() == nil)
    draft.text = "Saved and typed"
    #expect(draft.unsaved)
    #expect(draft.submit() == "Saved and typed")
    #expect(draft.submit() == nil)
    draft.save("Saved and typed")
    #expect(!draft.unsaved)
    #expect(!draft.saving)
    #expect(draft.text == "Saved and typed")
}

@Test func laterInputSurvivesSave() {
    var draft = ReadmeDraft("")
    draft.text = "First"
    let written = draft.submit()
    draft.text = "First and more"
    draft.save(written)
    #expect(draft.text == "First and more")
    #expect(draft.unsaved)
    #expect(draft.submit() == "First and more")
}

@Test func libraryTextReplacesSubmittedText() {
    var draft = ReadmeDraft("")
    draft.text = "Notes"
    _ = draft.submit()
    draft.save("Notes\n")
    #expect(draft.text == "Notes\n")
    #expect(!draft.unsaved)
}

@Test func unreadableAfterSave() {
    var draft = ReadmeDraft("")
    draft.text = "Written"
    _ = draft.submit()
    draft.save(nil)
    #expect(draft.text == "Written")
    #expect(!draft.unsaved)
}

@Test func refusedSaveKeepsInput() {
    var draft = ReadmeDraft("Saved")
    draft.text = "Refused"
    _ = draft.submit()
    draft.text = "Refused, then more"
    draft.refuse()
    #expect(draft.text == "Refused, then more")
    #expect(draft.unsaved)
    #expect(!draft.saving)
    draft.text = "Saved"
    #expect(!draft.unsaved)
}
