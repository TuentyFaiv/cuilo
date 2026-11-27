import SwiftUI
import Testing
import ExampleApp

@MainActor
struct ExampleOrganismTests {
    @Test func showsItsHeadingContentAndActions() {
        let organism = ExampleOrganism(heading: "Team") {
            Text("Three members")
        } actions: {
            Text("Invite")
        }

        #expect(organism.heading == "Team")
        #expect(organism.content == Text("Three members"))
        #expect(organism.actions == Text("Invite"))
    }

    @Test func namesTheRegionAfterItsHeadingWithoutActions() {
        let organism = ExampleOrganism(heading: "Team") { Text("Three members") }

        #expect(organism.regionLabel == Text("Team"))
        #expect(organism.headingTraits.contains(.isHeader))
    }
}
