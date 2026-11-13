import SwiftUI

/// The Tasks Context: the Active list and the Backlog, as Todoist-style rows.
struct TasksView: View {
    @Environment(TasksModel.self) private var model
    @State private var isAdding = false
    @State private var subtaskParent: ListedTask?

    var body: some View {
        @Bindable var model = model
        NavigationStack {
            Group {
                if model.tasks.isEmpty {
                    ContentUnavailableView {
                        AppContext.tasks.label
                    } description: {
                        Text(model.planning == .active ? "Nothing active." : "The Backlog is empty.")
                    }
                } else {
                    List {
                        ForEach(model.rows) { task in
                            TaskRow(
                                task: task, isCollapsed: model.isCollapsed(task),
                                onCircle: { model.toggleDone(task) }, onDisclose: { model.toggleCollapsed(task) }
                            )
                            .contentShape(Rectangle())
                            .draggable(task.id)
                            .dropDestination(for: String.self) { ids, _ in
                                guard !task.closed, let moved = model.tasks.first(where: { $0.id == ids.first }) else {
                                    return false
                                }
                                model.move(moved, under: task)
                                return true
                            }
                            .onTapGesture { if !task.closed { model.editing = task } }
                            .contextMenu {
                                if !task.closed && !task.done {  // a pending Done is Reverted at its circle
                                    Button("Add Subtask", systemImage: "plus.circle") { subtaskParent = task }
                                    if task.depth == 0 {  // a Subtask follows its top-level Task
                                        if task.planning == .backlog {
                                            Button("Make Active", systemImage: "arrow.up.circle") {
                                                model.plan(task, as: .active)
                                            }
                                        } else {
                                            Button("Move to Backlog", systemImage: "tray") {
                                                model.plan(task, as: .backlog)
                                            }
                                        }
                                    }
                                    Button("Reject", systemImage: "xmark.circle", role: .destructive) {
                                        model.close(task, as: .rejected)
                                    }
                                }
                            }
                        }
                        .onMove { model.move(from: $0, to: $1) }
                    }
                    .listStyle(.plain)
                }
            }
            .navigationTitle(Text(AppContext.tasks.title))
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Picker("List", selection: $model.planning) {
                        Text("Active").tag(Planning.active)
                        Text("Backlog").tag(Planning.backlog)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .accessibilityIdentifier("tasks-planning")
                }
                #if os(iOS)
                    ToolbarItem(placement: .topBarLeading) { EditButton() }  // reordering
                    ToolbarItem(placement: .topBarTrailing) { SettingsButton() }
                #endif
            }
            .safeAreaInset(edge: .bottom) {
                if let pendingClose = model.pendingClose { ClosedToast(pendingClose: pendingClose) }
            }
            .animation(.default, value: model.pendingClose)
            .addButton("New Task", identifier: "add-task") { isAdding = true }
            .sheet(isPresented: $isAdding) { NewTaskSheet(planning: model.planning) }
            .sheet(item: $subtaskParent) { NewTaskSheet(planning: $0.planning, parent: $0) }
            .sheet(item: $model.editing) { EditTaskSheet(task: $0) }
        }
    }
}

/// Names the Task just Closed and offers to Revert it.
private struct ClosedToast: View {
    @Environment(TasksModel.self) private var model
    let pendingClose: TasksModel.PendingClose

    var body: some View {
        HStack {
            Group {
                switch pendingClose.closing {
                case .done: Text("\(pendingClose.title) done")
                case .rejected: Text("\(pendingClose.title) rejected")
                }
            }
            .lineLimit(1)
            Spacer()
            Button("Revert") { model.revert() }
                .accessibilityIdentifier("revert-close")
        }
        .padding()
        .background(.regularMaterial, in: .rect(cornerRadius: 12))
        .padding(.horizontal)
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}

/// One Task: a circle, coloured by Priority and filled while Done, and beside
/// it the Title, the Description faded on one line, and the Tags fainter still.
/// A Subtask is indented, a Parent has a chevron to collapse it, a Closed Subtask
/// is struck through, a Blocked Task has a lock at the end. `onCircle` is a tap on the circle.
private struct TaskRow: View {
    let task: ListedTask
    let isCollapsed: Bool
    let onCircle: () -> Void
    let onDisclose: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Button(action: onDisclose) {
                Image(systemName: "chevron.right")
                    .rotationEffect(.degrees(isCollapsed ? 0 : 90))
                    .foregroundStyle(.secondary)
                    .frame(width: 16)
            }
            .buttonStyle(.plain)
            .opacity(task.hasSubtasks ? 1 : 0)
            .disabled(!task.hasSubtasks)
            .accessibilityLabel(isCollapsed ? "Expand" : "Collapse")
            Button(action: onCircle) {
                Image(systemName: task.done ? "circle.inset.filled" : "circle")
                    .font(.title2)
                    .foregroundStyle(task.priorityColor)
            }
            .buttonStyle(.plain)
            .disabled(task.closed)
            .accessibilityLabel(task.done ? "Revert Done" : "Done")
            VStack(alignment: .leading, spacing: 2) {
                Text(task.title)
                    .strikethrough(task.closed)
                    .foregroundStyle(task.closed ? .secondary : .primary)
                if !task.description.isEmpty {
                    Text(task.description)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                if !task.tags.isEmpty {
                    Text(task.tags.joined(separator: " "))
                        .font(.subheadline)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
            }
            if task.blocked {
                Spacer()
                Image(systemName: "lock.fill")
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Blocked")
            }
        }
        .padding(.vertical, 4)
        .padding(.leading, CGFloat(task.depth) * 24)
    }
}

extension ListedTask {
    fileprivate var priorityColor: Color {
        switch priority {
        case 1: .red
        case 2: .orange
        case 3: .blue
        default: .gray
        }
    }
}

/// Asks for a Title and Active or Backlog (Active unless the Backlog is shown),
/// or only a Title for a Subtask of `parent`.
private struct NewTaskSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var planning: Planning
    @FocusState private var isTitleFocused: Bool
    private let parent: ListedTask?

    init(planning: Planning, parent: ListedTask? = nil) {
        _planning = State(initialValue: planning)
        self.parent = parent
    }

    private var canAdd: Bool { !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        NavigationStack {
            Form {
                TextField("Title", text: $title)
                    .focused($isTitleFocused)
                    .onSubmit(add)
                    .accessibilityIdentifier("task-title")
                if parent == nil {
                    Picker("List", selection: $planning) {
                        Text("Active").tag(Planning.active)
                        Text("Backlog").tag(Planning.backlog)
                    }
                    .pickerStyle(.segmented)
                }
            }
            .formStyle(.grouped)
            .navigationTitle(parent == nil ? "New Task" : "New Subtask")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add", action: add).disabled(!canAdd)
                }
            }
        }
        .onAppear { isTitleFocused = true }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium])
    }

    private func add() {
        let added =
            if let parent { model.addSubtask(title: title, under: parent) } else {
                model.add(title: title, planning: planning)
            }
        if added { dismiss() }
    }
}

/// Edits a Task's Title, Description, Priority and Tags; Save is off while the Title is empty.
private struct EditTaskSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let task: ListedTask
    @State private var title: String
    @State private var description: String
    @State private var priority: UInt8
    @State private var tags: [String]
    @State private var newTag = ""
    @State private var refusedTagText = false
    @State private var blockedBy: [Blocker]
    @State private var refusedBlockedBy = false

    init(task: ListedTask) {
        self.task = task
        _title = State(initialValue: task.title)
        _description = State(initialValue: task.description)
        _priority = State(initialValue: task.priority)
        _tags = State(initialValue: task.tags)
        _blockedBy = State(initialValue: task.blockedBy)
    }

    /// Existing Tags matching what is typed, not yet on the Task.
    private var suggestions: [String] {
        guard !newTag.isEmpty else { return [] }
        return model.knownTags(prefix: newTag).filter { !tags.contains($0) }
    }

    private func addTag(_ tag: String) {
        guard isValidTag(tag: tag) else { return }
        if !tags.contains(tag) { tags.append(tag) }
        newTag = ""
        refusedTagText = false
    }

    /// Open Tasks it could be Blocked by, not yet chosen here.
    private var blockerCandidates: [ListedTask] {
        let chosen = Set(blockedBy.map(\.id))
        return model.blockerCandidates(for: task).filter { !chosen.contains($0.id) }
    }

    private var canSave: Bool { !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        NavigationStack {
            Form {
                TextField("Title", text: $title)
                    .onSubmit(save)
                    .accessibilityIdentifier("task-title")
                TextField("Description", text: $description, axis: .vertical)
                    .lineLimit(3...8)
                    .accessibilityIdentifier("task-description")
                Picker("Priority", selection: $priority) {
                    ForEach(UInt8(1)...4, id: \.self) { Text(verbatim: "P\($0)").tag($0) }
                }
                .pickerStyle(.segmented)
                Section("Tags") {
                    ForEach(tags, id: \.self) { tag in
                        HStack {
                            Text(tag)
                            Spacer()
                            Button("Remove \(tag)", systemImage: "minus.circle") { tags.removeAll { $0 == tag } }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.borderless)
                        }
                    }
                    TextField("Add Tag", text: $newTag)
                        .onSubmit { addTag(newTag) }
                        .onChange(of: newTag) { _, text in
                            let allowed = TasksModel.allowedTagText(text)
                            refusedTagText = allowed != text
                            if refusedTagText { newTag = allowed }
                        }
                        #if os(iOS)
                            .textInputAutocapitalization(.never)
                        #endif
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("task-tag")
                    if refusedTagText {
                        Text("Tags use a–z, 0–9, - and / only.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                    ForEach(suggestions, id: \.self) { tag in
                        Button(tag) { addTag(tag) }
                    }
                }
                Section("Blocked by") {
                    ForEach(blockedBy, id: \.id) { blocker in
                        HStack {
                            Text(blocker.title)
                                .strikethrough(blocker.closed)
                                .foregroundStyle(blocker.closed ? .secondary : .primary)
                            Spacer()
                            Button("Remove \(blocker.title)", systemImage: "minus.circle") {
                                blockedBy.removeAll { $0.id == blocker.id }
                            }
                            .labelStyle(.iconOnly)
                            .buttonStyle(.borderless)
                        }
                    }
                    if !blockerCandidates.isEmpty {
                        Menu("Add Blocked by") {
                            ForEach(blockerCandidates) { candidate in
                                Button(candidate.title) {
                                    blockedBy.append(Blocker(id: candidate.id, title: candidate.title, closed: false))
                                    refusedBlockedBy = false
                                }
                            }
                        }
                        .accessibilityIdentifier("task-add-blocker")
                    }
                    if refusedBlockedBy {
                        Text("That would make Tasks wait on each other.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Task")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", action: save).disabled(!canSave)
                }
            }
        }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium])
    }

    private func save() {
        var saved = tags
        if isValidTag(tag: newTag), !saved.contains(newTag) { saved.append(newTag) }
        guard model.setBlockedBy(task, to: blockedBy.map(\.id)) else {
            refusedBlockedBy = true
            return
        }
        if model.edit(task, title: title, description: description, priority: priority, tags: saved) { dismiss() }
    }
}
