import SwiftUI

/// A distinct, labelled region of the screen, built from molecules.
public struct ExampleOrganism<Content: View, Actions: View>: View {
    public let heading: LocalizedStringKey
    public let content: Content
    public let actions: Actions

    public init(
        heading: LocalizedStringKey,
        @ViewBuilder content: () -> Content,
        @ViewBuilder actions: () -> Actions
    ) {
        self.heading = heading
        self.content = content()
        self.actions = actions()
    }

    /// What VoiceOver calls the region.
    public var regionLabel: Text {
        Text(heading)
    }

    /// The heading's traits, which let VoiceOver's headings rotor find the region.
    public var headingTraits: AccessibilityTraits {
        .isHeader
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .firstTextBaseline) {
                Text(heading)
                    .font(.title3.weight(.semibold))
                    .accessibilityAddTraits(headingTraits)
                Spacer()
                actions
            }
            content
        }
        .padding(24)
        .overlay {
            RoundedRectangle(cornerRadius: 12).stroke(.quaternary)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(regionLabel)
    }
}

extension ExampleOrganism where Actions == EmptyView {
    public init(heading: LocalizedStringKey, @ViewBuilder content: () -> Content) {
        self.init(heading: heading, content: content, actions: { EmptyView() })
    }
}

#Preview {
    ExampleOrganism(heading: "Team") {
        Text("Three members")
    } actions: {
        Button("Invite") {}
    }
    .padding()
}
