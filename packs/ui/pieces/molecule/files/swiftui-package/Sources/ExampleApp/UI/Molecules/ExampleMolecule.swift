import SwiftUI

/// Groups atoms under one accessible label, such as a field with its hint.
public struct ExampleMolecule<Content: View>: View {
    public let label: LocalizedStringKey
    public let description: LocalizedStringKey?
    public let content: Content

    public init(
        label: LocalizedStringKey,
        description: LocalizedStringKey? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.label = label
        self.description = description
        self.content = content()
    }

    /// What VoiceOver calls the group.
    public var groupLabel: Text {
        Text(label)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(label)
                .font(.subheadline.weight(.semibold))
            if let description {
                Text(description)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            HStack(spacing: 8) {
                content
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(groupLabel)
    }
}

#Preview {
    ExampleMolecule(label: "Notifications", description: "Choose how we reach you.") {
        Button("Email") {}
        Button("Push") {}
    }
    .padding()
}
