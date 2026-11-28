import SwiftUI
import Testing
import ExampleApp

@MainActor
struct ExampleMoleculeTests {
    @Test func labelsTheGroupAndShowsItsContent() {
        let molecule = ExampleMolecule(label: "Notifications", description: "Choose how we reach you.") {
            Text("Email")
        }

        #expect(molecule.groupLabel == Text("Notifications"))
        #expect(molecule.description == "Choose how we reach you.")
        #expect(molecule.content == Text("Email"))
    }

    @Test func leavesOutTheDescriptionWhenThereIsNone() {
        let molecule = ExampleMolecule(label: "Notifications") { Text("Email") }

        #expect(molecule.description == nil)
    }
}
