import AppKit
import SwiftUI

struct DestinationPicker: NSViewRepresentable {
    @Bindable var model: DesktopModel

    func makeCoordinator() -> Coordinator { Coordinator(model: model) }

    func makeNSView(context: Context) -> NSSegmentedControl {
        let control = NSSegmentedControl(labels: DesktopModel.Destination.allCases.map(\.rawValue),
                                         trackingMode: .selectOne, target: context.coordinator,
                                         action: #selector(Coordinator.changed(_:)))
        control.setAccessibilityLabel("Destination")
        control.setAccessibilityIdentifier("navigation.destinations")
        return control
    }

    func updateNSView(_ control: NSSegmentedControl, context: Context) {
        control.isEnabled = !model.busy
        control.selectedSegment = DesktopModel.Destination.allCases.firstIndex(of: model.destination) ?? -1
    }

    @MainActor final class Coordinator: NSObject {
        private let model: DesktopModel
        init(model: DesktopModel) { self.model = model }

        @objc func changed(_ control: NSSegmentedControl) {
            guard DesktopModel.Destination.allCases.indices.contains(control.selectedSegment) else { return }
            model.navigate(to: DesktopModel.Destination.allCases[control.selectedSegment])
            control.selectedSegment = DesktopModel.Destination.allCases.firstIndex(of: model.destination) ?? -1
        }
    }
}
