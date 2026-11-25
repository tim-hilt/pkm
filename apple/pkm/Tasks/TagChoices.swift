import SwiftUI

/// A Tag offered while an `@` word is typed: an existing one, or a new one to create.
struct TagChoice: Identifiable, Equatable {
    let tag: String
    let isNew: Bool
    var id: String { tag }
}

/// What a Quick Add field offers right now and how to pick it, published by the focused field
/// as the scene's focused value so that a bar above the keyboard can show it (`TagChoiceBar`).
/// Equal when the offer is: a new closure on every update must not count as a change.
struct TagChoices: Equatable {
    /// Matching Tags in Relevance order, then Create last.
    let items: [TagChoice]
    /// The index the arrow keys are on.
    let selected: Int
    let pick: (TagChoice) -> Void

    static func == (a: Self, b: Self) -> Bool { a.items == b.items && a.selected == b.selected }
}

extension FocusedValues {
    /// The focused Quick Add field's choices; nil while it offers none.
    var tagChoices: TagChoices? {
        get { self[TagChoicesKey.self] }
        set { self[TagChoicesKey.self] = newValue }
    }
}

private struct TagChoicesKey: FocusedValueKey {
    typealias Value = TagChoices
}

/// The chips of the focused field's `TagChoices`; nothing while it offers none. Put it in a bottom
/// inset, which rides above the keyboard.
struct TagChoiceBar: View {
    @Environment(TasksModel.self) private var model
    @FocusedValue(\.tagChoices) private var choices

    var body: some View {
        if let choices {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(Array(choices.items.enumerated()), id: \.element.id) { index, choice in
                        Button {
                            choices.pick(choice)
                        } label: {
                            Label(choice.isNew ? "Create Tag '\(choice.tag)'" : choice.tag, systemImage: "tag")
                                .foregroundStyle(model.tagEntry(choice.tag).style(default: .primary))
                                .padding(.horizontal, 12)
                                .padding(.vertical, 6)
                                .background(
                                    index == choices.selected
                                        ? Color.accentColor.opacity(0.25) : Color.secondary.opacity(0.15),
                                    in: .capsule)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("tag-choice")
                    }
                }
                .padding(.horizontal)
            }
            .padding(.vertical, 6)
            .background(.bar)
        }
    }
}
