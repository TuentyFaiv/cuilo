import SwiftUI

/// The smallest UI building block: a label-sized piece of content.
public struct ExampleAtom<Content: View>: View {
    /// Visual emphasis.
    public enum Tone: Sendable {
        case neutral, accent
    }

    public let tone: Tone
    public let content: Content

    public init(tone: Tone = .neutral, @ViewBuilder content: () -> Content) {
        self.tone = tone
        self.content = content()
    }

    public var body: some View {
        content
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .foregroundStyle(tone == .accent ? AnyShapeStyle(.white) : AnyShapeStyle(.secondary))
            .background(tone == .accent ? AnyShapeStyle(.tint) : AnyShapeStyle(.quaternary), in: .capsule)
    }
}

#Preview {
    HStack {
        ExampleAtom { Text("Draft") }
        ExampleAtom(tone: .accent) { Text("New") }
    }
    .padding()
}
