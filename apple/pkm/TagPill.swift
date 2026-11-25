import SwiftUI

/// A Tag as a capsule in its Tag Color (readable grey if it has none), wherever Tags are listed.
struct TagPill: View {
    @Environment(TasksModel.self) private var model
    let tag: String

    var body: some View {
        let style = model.tagEntry(tag).style(default: .secondary)
        Text(tag)
            .foregroundStyle(style)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(style.opacity(0.18), in: .capsule)
    }
}

/// Tags as one line of pills, cut off at the end when they do not fit; a pill is never wrapped or squeezed.
struct TagPills: View {
    let tags: [String]

    var body: some View {
        HStack(spacing: 4) { ForEach(tags, id: \.self) { TagPill(tag: $0) } }
            .lineLimit(1)
            .fixedSize()
            // Only a minimum lets the frame be narrower than the pills: with only a maximum it takes
            // their full width, overflowing its slot instead of cutting them off.
            .frame(minWidth: 0, alignment: .leading)
            .clipped()
    }
}

/// The line in an Item's sheet that shows its Tags as pills, and opens the picker when tapped.
struct TagsRow: View {
    let tags: [String]
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: "tag").foregroundStyle(.secondary)
                if tags.isEmpty {
                    Text("Add Tags").foregroundStyle(.secondary)
                } else {
                    TagPills(tags: tags)
                }
                Spacer(minLength: 0)
            }
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("task-tags")
    }
}
