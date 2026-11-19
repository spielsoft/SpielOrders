import SpielOrdersKit
import SwiftUI

/// How many Orders a change rewrites, as each sheet states it.
enum OrderChangeCount {
    static func text(_ count: Int) -> String {
        switch count {
        case 0: "No Orders will change."
        case 1: "1 Order will change."
        default: "\(count) Orders will change."
        }
    }
}

/// One spelling a merge can keep, with how many Orders keeping it changes.
struct MergeChoice {
    let name: String
    let orderCount: Int
}

/// Merge N …: which spelling of the values to keep, each with the Orders choosing it changes.
/// A value has no facts for later ones to fill, so the only choice is the spelling kept.
struct MergeSheet: View {
    let title: String
    let explanation: String
    let choices: [MergeChoice]
    let identifier: String
    let complete: (Int?) -> Void
    @State private var chosen: Int

    init(title: String, explanation: String, choices: [MergeChoice], identifier: String,
         complete: @escaping (Int?) -> Void) {
        self.title = title
        self.explanation = explanation
        self.choices = choices
        self.identifier = identifier
        self.complete = complete
        _chosen = State(initialValue: 0)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text(explanation)
                .fixedSize(horizontal: false, vertical: true)
            Picker("Keep", selection: $chosen) {
                ForEach(choices.indices, id: \.self) { index in
                    Text(choices[index].name).tag(index)
                }
            }
            .pickerStyle(.radioGroup)
            .accessibilityIdentifier(identifier + ".keep")
            Text(OrderChangeCount.text(choices[chosen].orderCount))
                .foregroundStyle(.secondary)
                .accessibilityIdentifier(identifier + ".count")
            HStack {
                Spacer()
                Button("Cancel") { complete(nil) }
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier(identifier + ".cancel")
                Button("Merge") { complete(chosen) }
                    .keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier(identifier + ".confirm")
            }
        }
        .padding(20)
        .frame(width: 440)
    }
}

/// Vendors in precedence order with the library's plan for merging them in that order.
struct VendorMerge {
    let vendors: [VendorRow]
    let change: ValueChange
}

/// Merge N Vendors…: the Vendors in precedence order, which the owner sets by dragging, with
/// how many Orders the plan for that order changes.
struct VendorMergeSheet: View {
    let title: String
    let explanation: String
    let plan: @MainActor ([VendorRow]) async throws -> VendorMerge
    let complete: (VendorMerge?) -> Void
    @State private var vendors: [VendorRow]
    @State private var planned: VendorMerge
    @State private var failure: Notice?

    init(title: String, explanation: String, merge: VendorMerge,
         plan: @escaping @MainActor ([VendorRow]) async throws -> VendorMerge,
         complete: @escaping (VendorMerge?) -> Void) {
        self.title = title
        self.explanation = explanation
        self.plan = plan
        self.complete = complete
        _vendors = State(initialValue: merge.vendors)
        _planned = State(initialValue: merge)
    }

    /// The plan for the order shown, once the library has made it.
    private var current: VendorMerge? {
        planned.vendors.map(\.id) == vendors.map(\.id) ? planned : nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text(explanation)
                .fixedSize(horizontal: false, vertical: true)
            PrecedenceList(items: $vendors, identifier: "vendor.merge") { vendor in
                Text(vendor.name)
                    .lineLimit(1)
            }
            .frame(height: 120)
            if let current {
                Text(OrderChangeCount.text(current.change.orderCount))
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("vendor.merge.count")
            } else if let failure {
                PlanningFailure(failure: failure, identifier: "vendor.merge")
            } else {
                ProgressView("Checking affected Orders…")
                    .controlSize(.small)
                    .accessibilityIdentifier("vendor.merge.checking")
            }
            HStack {
                Spacer()
                Button("Cancel") { complete(nil) }
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("vendor.merge.cancel")
                Button("Merge") { complete(current) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(current == nil)
                    .accessibilityIdentifier("vendor.merge.confirm")
            }
        }
        .padding(20)
        .frame(width: 460)
        .task(id: vendors.map(\.id)) {
            guard current == nil else { return }
            failure = nil
            do {
                let merge = try await plan(vendors)
                try Task.checkCancellation()
                planned = merge
            } catch {
                guard !Task.isCancelled else { return }
                failure = Notice((error as? ValueRefusal)?.summary ?? "The merge couldn’t be planned.",
                                 details: error.localizedDescription)
            }
        }
    }
}

/// One thing a deleted value's Orders can use instead.
struct ReplacementChoice {
    let title: String
    let replacement: Replacement
}

/// Delete… for a value Orders use: what they use instead, and how many change.
struct ReplacementSheet: View {
    let title: String
    let explanation: String
    let choices: [ReplacementChoice]
    let identifier: String
    let plan: @MainActor (Replacement) async throws -> ValueChange
    let complete: (ValueChange?) -> Void
    @State private var chosen: Replacement?
    @State private var prepared: (choice: Replacement, change: ValueChange)?
    @State private var failure: Notice?

    init(title: String, explanation: String, choices: [ReplacementChoice], initial: Replacement,
         change: ValueChange, identifier: String,
         plan: @escaping @MainActor (Replacement) async throws -> ValueChange,
         complete: @escaping (ValueChange?) -> Void) {
        self.title = title
        self.explanation = explanation
        self.choices = choices
        self.identifier = identifier
        self.plan = plan
        self.complete = complete
        _prepared = State(initialValue: (initial, change))
    }

    private var selected: ValueChange? {
        guard let chosen, prepared?.choice == chosen else { return nil }
        return prepared?.change
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text(explanation)
                .fixedSize(horizontal: false, vertical: true)
            Picker("Replace with", selection: $chosen) {
                Text("Choose a replacement…").tag(Replacement?.none)
                ForEach(choices, id: \.replacement) { choice in
                    Text(choice.title).tag(Optional(choice.replacement))
                }
            }
            .pickerStyle(.menu)
            .fixedSize()
            .accessibilityIdentifier(identifier + ".replacement")
            if let selected {
                Text(OrderChangeCount.text(selected.orderCount))
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier(identifier + ".count")
            } else if chosen == nil {
                Text("Choose a replacement to continue.")
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier(identifier + ".choiceRequired")
            } else if let failure {
                Text(failure.summary).foregroundStyle(.problemText)
                    .accessibilityIdentifier(identifier + ".problem")
                DisclosureGroup("Details") {
                    ScrollView { Text(failure.details).textSelection(.enabled) }
                        .frame(maxHeight: 160)
                }
                .accessibilityIdentifier(identifier + ".details")
            } else {
                ProgressView("Checking affected Orders…")
                    .accessibilityIdentifier(identifier + ".checking")
            }
            HStack {
                Spacer()
                Button("Cancel") { complete(nil) }
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier(identifier + ".cancel")
                Button("Delete", role: .destructive) { complete(selected) }
                    .disabled(selected == nil)
                    .accessibilityIdentifier(identifier + ".confirm")
            }
        }
        .padding(20)
        .frame(width: 440)
        .task(id: chosen) {
            guard let replacement = chosen, selected == nil else { return }
            failure = nil
            do {
                let change = try await plan(replacement)
                try Task.checkCancellation()
                prepared = (replacement, change)
            } catch {
                guard !Task.isCancelled else { return }
                failure = Notice((error as? ValueRefusal)?.summary ?? "The change couldn’t be planned.",
                                 details: error.localizedDescription)
            }
        }
    }
}

extension WindowDialogs {
    /// Asks which of `choices` a merge keeps; nil when cancelled.
    func chooseKept(_ title: String, explanation: String, choices: [MergeChoice],
                    identifier: String) async -> Int? {
        await ask(title, identifier: identifier) { complete in
            MergeSheet(title: title, explanation: explanation, choices: choices, identifier: identifier,
                       complete: complete)
        }
    }

    /// Asks the owner to order a merge of Vendors, starting from `merge`; the plan they
    /// approved, or nil when cancelled.
    func reviewMerge(of merge: VendorMerge, title: String, explanation: String,
                     plan: @escaping @MainActor ([VendorRow]) async throws -> VendorMerge) async -> VendorMerge? {
        await ask(title, identifier: "vendor.merge") { complete in
            VendorMergeSheet(title: title, explanation: explanation, merge: merge, plan: plan, complete: complete)
        }
    }

    /// Asks what replaces a deleted value in the Orders using it; nil when cancelled.
    func chooseReplacement(_ title: String, explanation: String, choices: [ReplacementChoice], initial: Replacement,
                           change: ValueChange, identifier: String,
                           plan: @escaping @MainActor (Replacement) async throws -> ValueChange) async -> ValueChange? {
        await ask(title, identifier: identifier) { complete in
            ReplacementSheet(title: title, explanation: explanation, choices: choices, initial: initial,
                             change: change, identifier: identifier, plan: plan, complete: complete)
        }
    }
}
