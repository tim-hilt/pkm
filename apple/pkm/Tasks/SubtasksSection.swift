import SwiftUI

/// A Task's Subtasks in its edit form: a header with the "Done/total" counter and a
/// fold chevron, then the direct Subtasks as list rows. A tap opens a Subtask's form on top
/// (the sheet's stack, as for a Blocker), unless `canOpen` is false (the Title on top is
/// empty). Starts unfolded; the fold is not saved.
struct SubtasksSection: View {
    @Environment(TasksModel.self) private var model
    let task: ListedTask
    let canOpen: Bool
    @State private var isFolded = false

    var body: some View {
        // The counts as last saved: the Subtasks' own edits change them, not this form's.
        let saved = model.task(id: task.id) ?? task
        let subtasks = model.subtasks(of: task.id)
        Section {
            if !isFolded {
                ForEach(subtasks) { subtask in
                    TaskRow(task: subtask, showsParent: false, onCircle: { model.toggleDone(subtask) })
                        .contentShape(Rectangle())
                        .onTapGesture { if canOpen { model.open(id: subtask.id) } }
                }
            }
        } header: {
            HStack {
                Text("Subtasks")
                Spacer()
                if saved.subtasksTotal > 0 {
                    Text("\(saved.subtasksDone)/\(saved.subtasksTotal)")
                        .accessibilityLabel("\(saved.subtasksDone) of \(saved.subtasksTotal) Subtasks Done")
                }
                Button(isFolded ? "Unfold Subtasks" : "Fold Subtasks", systemImage: "chevron.down") {
                    withAnimation { isFolded.toggle() }
                }
                .labelStyle(.iconOnly)
                .rotationEffect(.degrees(isFolded ? -90 : 0))
                .buttonStyle(.plain)
            }
        }
        .accessibilityIdentifier("task-subtasks")
    }
}
