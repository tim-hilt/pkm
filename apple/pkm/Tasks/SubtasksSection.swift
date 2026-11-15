import SwiftUI

/// A Task's Subtasks in its edit form: a header with the "Done/total" counter and a
/// fold chevron, then the direct Subtasks as list rows. A tap opens a Subtask's form on top
/// (the sheet's stack, as for a Blocker), unless `canOpen` is false (the Title on top is
/// empty). The circle marks a Subtask Done or Reverts it, a swipe Rejects it. Ends with an
/// "Add Subtask" row. Starts unfolded; the fold is not saved.
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
                        .swipeActions(edge: .trailing) {  // no context menu: Reject is all it would offer
                            if !subtask.done {
                                Button("Reject", role: .destructive) { model.close(subtask, as: .rejected) }
                            }
                        }
                }
                AddSubtaskRow(parent: task)
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

/// The last row of the Subtasks section: "Add Subtask", which a tap turns into a focused Quick
/// Add field. Return adds the Subtask at the end of `parent`'s and empties the field, which stays
/// focused for the next one; Return on an empty field, or losing focus, turns it back into the row.
private struct AddSubtaskRow: View {
    @Environment(TasksModel.self) private var model
    let parent: ListedTask
    @State private var isAdding = false
    @State private var quickAdd = TaskQuickAdd()
    /// Quick Add's Title, kept up to date by the field.
    @State private var title = ""
    @State private var revision = 0

    var body: some View {
        if isAdding {
            QuickAddField(quickAdd: quickAdd, title: $title, revision: revision, onSubmit: submit, onBlur: stop)
                .accessibilityIdentifier("subtask-title")
        } else {
            Button("Add Subtask", systemImage: "plus") { isAdding = true }
                .accessibilityIdentifier("add-subtask")
        }
    }

    private func submit() {
        guard !title.isEmpty else { return stop() }
        guard
            model.addSubtask(
                title: title, priority: quickAdd.priority() ?? 4, tags: quickAdd.tags(), under: parent)
        else { return }
        quickAdd.replace(start: 0, end: UInt32(quickAdd.text().utf16.count), with: "")
        revision += 1
    }

    private func stop() {
        isAdding = false
        quickAdd = TaskQuickAdd()
        title = ""
    }
}
