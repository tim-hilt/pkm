import SwiftUI

/// A Task's Subtasks in its edit form: a header with the Open Subtasks counter and a
/// fold chevron, then the direct Subtasks as list rows. A tap opens a Subtask's form on top
/// (the sheet's stack, as for a Blocker), unless `canOpen` is false (the Title on top is
/// empty). The circle marks a Subtask Done or Reverts it, a swipe Rejects it, a drag reorders it
/// among its siblings. Ends with an "Add Subtask" row. Starts unfolded; the fold is not saved.
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
                        .swipeActions(edge: .trailing) {
                            if !subtask.done {
                                Button("Reject", role: .destructive) { model.close(subtask, as: .rejected) }
                            }
                        }
                        .taskQuickActions(subtask)
                }
                .onMove { model.moveSubtask(of: task.id, from: $0, to: $1) }
                AddSubtaskRow(parent: task)
            }
        } header: {
            HStack {
                Text("Subtasks")
                Spacer()
                if saved.openSubtasks > 0 {
                    Text("\(saved.openSubtasks)")
                        .accessibilityLabel("\(saved.openSubtasks) Open Subtasks")
                }
                Button(isFolded ? "Unfold Subtasks" : "Fold Subtasks", systemImage: "chevron.down") {
                    withAnimation { isFolded.toggle() }
                }
                .labelStyle(.iconOnly)
                .rotationEffect(.degrees(isFolded ? -90 : 0))
                .buttonStyle(.plain)
            }
        }
    }
}

/// The last row of the Subtasks section: "Add Subtask", which a tap turns into a focused Quick
/// Add field. Return adds the Subtask at the end of `parent`'s and empties the field, which stays
/// focused for the next one; Return on an empty field, or losing focus, turns it back into the row.
private struct AddSubtaskRow: View {
    @Environment(TasksModel.self) private var model
    let parent: ListedTask
    @State private var quickAdd: TaskQuickAdd?
    /// Quick Add's Title, kept up to date by the field.
    @State private var title = ""
    @State private var revision = 0

    var body: some View {
        if let quickAdd {
            // Indented to line up with the Subtasks' Titles, past their circles.
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "circle").font(.title2).hidden()
                QuickAddField(quickAdd: quickAdd, title: $title, revision: revision, onSubmit: submit, onBlur: stop)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("subtask-title")
        } else {
            Button("Add Subtask", systemImage: "plus") { quickAdd = model.newQuickAdd() }
                .accessibilityIdentifier("add-subtask")
        }
    }

    private func submit() {
        guard let quickAdd, !title.isEmpty else { return stop() }
        guard
            model.addSubtask(
                title: title, priority: quickAdd.priority() ?? 4, tags: quickAdd.tags(), under: parent)
        else { return }
        self.quickAdd = model.newQuickAdd()
        revision += 1
    }

    private func stop() {
        quickAdd = nil
        title = ""
    }
}
