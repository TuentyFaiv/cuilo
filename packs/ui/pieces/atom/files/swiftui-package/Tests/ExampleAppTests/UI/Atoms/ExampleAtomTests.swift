import SwiftUI
import Testing
import ExampleApp

@MainActor
struct ExampleAtomTests {
    @Test func showsItsContentInTheNeutralToneByDefault() {
        let atom = ExampleAtom { Text("Draft") }

        #expect(atom.tone == .neutral)
        #expect(atom.content == Text("Draft"))
    }

    @Test func keepsTheToneItIsGiven() {
        let atom = ExampleAtom(tone: .accent) { Text("New") }

        #expect(atom.tone == .accent)
        #expect(atom.content == Text("New"))
    }
}
