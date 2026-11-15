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
                            .onTapGesture { if !task.closed { model.startEditing(task) } }
                            .swipeActions(edge: .trailing) {
                                if !task.closed && !task.done {
                                    Button("Reject", role: .destructive) { model.close(task, as: .rejected) }
                                }
                            }
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
                    .contentMargins(.bottom, 96, for: .scrollContent)  // scroll the last Task clear of the "+"
                }
            }
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)  // no large-title space above the list
            #endif
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
                    ToolbarItem(placement: .topBarTrailing) { SettingsButton() }
                #endif
            }
            .safeAreaInset(edge: .bottom) {
                if let pendingClose = model.pendingClose { ClosedToast(closing: pendingClose) }
            }
            .animation(.default, value: model.pendingClose)
            .addButton("New Task", identifier: "add-task") { isAdding = true }
            .sheet(isPresented: $isAdding) { NewTaskSheet() }
            .sheet(item: $subtaskParent) { NewTaskSheet(parent: $0) }
            .sheet(isPresented: Binding(get: { !model.editing.isEmpty }, set: { if !$0 { model.closeEditing() } })) {
                EditTaskSheet()
            }
        }
    }
}

/// Tells that a Task was just Closed and offers to Revert it.
private struct ClosedToast: View {
    @Environment(TasksModel.self) private var model
    let closing: Closing

    var body: some View {
        HStack {
            switch closing {
            case .done: Text("Task Done")
            case .rejected: Text("Task Rejected")
            }
            Spacer()
            Button("Revert") { model.revert() }
                .accessibilityIdentifier("revert-close")
        }
        .padding(.horizontal)
        .frame(minHeight: AddButtonMetrics.size)
        .background(.regularMaterial, in: .rect(cornerRadius: 12))
        #if os(iOS)  // left of the "+", level with it
            .padding(.leading, AddButtonMetrics.margin)
            .padding(.trailing, AddButtonMetrics.size + 2 * AddButtonMetrics.margin)
            .padding(.bottom, AddButtonMetrics.margin)
        #else
            .padding(.horizontal)
        #endif
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}

/// One Task: a circle, coloured by Priority and filled while Done, and beside
/// it the Title, the Description faded on one line, and the Tags fainter still.
/// A Subtask is indented, a Parent has a chevron to collapse it, a Closed Subtask
/// is struck through, a Blocked Task has a lock at the end. `onCircle` is a tap on the circle.
/// A Tag's name in its Tag Color, Default being the tertiary grey.
private func tagText(_ tag: TagEntry) -> Text {
    Text(tag.name).foregroundStyle(tag.style())
}

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
                if let first = task.tags.first {
                    // One Text, so that too many Tags are cut off at the end, not each.
                    task.tags.dropFirst().reduce(tagText(first)) { Text("\($0) \(tagText($1))") }
                        .font(.subheadline)
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
        .frame(maxWidth: .infinity, alignment: .leading)  // the whole row opens the Task, not just its text
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

/// Asks for a Title and a Description; the Task is added Active. Also used for a Subtask of `parent`.
private struct NewTaskSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var quickAdd = TaskQuickAdd()
    /// Quick Add's Title, kept up to date by the field.
    @State private var title = ""
    @State private var description = ""
    private let parent: ListedTask?

    init(parent: ListedTask? = nil) {
        self.parent = parent
    }

    var body: some View {
        NavigationStack {
            Form {
                QuickAddField(quickAdd: quickAdd, title: $title, onSubmit: add)
                    .accessibilityIdentifier("task-title")
                TextField("Description", text: $description, axis: .vertical)
                    .lineLimit(3...8)
                    .accessibilityIdentifier("task-description")
            }
            .formStyle(.grouped)
            .navigationTitle(parent == nil ? "New Task" : "New Subtask")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                #if os(macOS)
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                #endif
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add", action: add).disabled(title.isEmpty)
                }
            }
        }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium])
    }

    private func add() {
        let priority = quickAdd.priority() ?? 4
        let tags = quickAdd.tags()
        let added =
            if let parent {
                model.addSubtask(title: title, description: description, priority: priority, tags: tags, under: parent)
            } else {
                model.add(title: title, description: description, priority: priority, tags: tags)
            }
        if added { dismiss() }
    }
}

/// The sheet over the list: a stack of Tasks following the model's, the Task
/// opened first at the bottom and each Blocker opened from one pushed on it.
/// While the Title on top is empty the sheet can't be left.
private struct EditTaskSheet: View {
    @Environment(TasksModel.self) private var model
    /// Dismisses the sheet, not a pushed form; its binding then empties the model's stack.
    @Environment(\.dismiss) private var dismiss

    /// The pushed Tasks; popping (Back, edge swipe) goes through the model.
    private var pushed: Binding<[String]> {
        Binding(
            get: { Array(model.editing.dropFirst()) },
            set: { path in
                if path.count > model.editing.count - 1, let id = path.last {
                    model.openBlocker(id: id)
                } else {
                    while model.editing.count > path.count + 1, model.back() {}
                }
            }
        )
    }

    @ViewBuilder private func form(_ id: String?) -> some View {
        // Its own identity per Task: a link may swap the bottom Task while the sheet stays.
        if let task = id.flatMap(model.task(id:)) { EditTaskForm(task: task, done: { dismiss() }).id(task.id) }
    }

    var body: some View {
        NavigationStack(path: pushed) {
            form(model.editing.first)
                .navigationDestination(for: String.self) { form($0) }
        }
        .interactiveDismissDisabled(model.isLeavingBlocked)
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium])
    }
}

/// Edits a Task, saving as it goes: Title and Description after a pause in typing
/// and when their field loses focus, everything else at once, and a Tag typed but
/// not submitted when the view is left. The Title is Quick Add: the pause saves it without
/// pending Shortcuts, and Return, losing focus and leaving the view apply them. While the Title is empty the rest still
/// saves (the last saved Title stays), a hint shows, and the view can't be left.
/// On macOS `done` closes the whole sheet. An Open Blocker is a row that opens its
/// own form on top; Back then shows this one as last saved.
private struct EditTaskForm: View {
    @Environment(TasksModel.self) private var model
    let task: ListedTask
    let done: () -> Void
    @State private var title: String
    /// The Title field's text with its Shortcuts; `title` is what it leaves of it.
    @State private var quickAdd: TaskQuickAdd
    @State private var quickAddRevision = 0
    @State private var description: String
    @State private var priority: UInt8
    @State private var tags: [String]
    @State private var newTag = ""
    @State private var refusedTagText = false
    @State private var blockedBy: [Blocker]
    @FocusState private var focus: Field?

    private enum Field { case description }

    /// How long typing pauses before text is saved.
    private static let pause = Duration.milliseconds(500)

    init(task: ListedTask, done: @escaping () -> Void) {
        self.task = task
        self.done = done
        _title = State(initialValue: task.title)
        _quickAdd = State(initialValue: .fromTitle(title: task.title))
        _description = State(initialValue: task.description)
        _priority = State(initialValue: task.priority)
        _tags = State(initialValue: task.tags.map(\.name))
        _blockedBy = State(initialValue: task.blockedBy)
    }

    /// Known Tags matching what is typed, not yet on the Task, and whether to offer Create.
    private var suggestions: TagSuggestions { model.suggestTags(newTag, leaveOut: tags) }

    private func addTag(_ tag: String) {
        guard isValidTag(tag: tag) else { return }
        if !tags.contains(tag) { tags.append(tag) }
        newTag = ""
        refusedTagText = false
        save()
    }

    /// Open Tasks it could be Blocked by, not yet chosen here.
    private var blockerCandidates: [ListedTask] {
        let chosen = Set(blockedBy.map(\.id))
        return model.blockerCandidates(for: task).filter { !chosen.contains($0.id) }
    }

    private var hasTitle: Bool { !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// What the typing pause waits on.
    private var text: [String] { [title, description] }

    var body: some View {
        Form {
            QuickAddField(
                quickAdd: quickAdd, title: $title, revision: quickAddRevision, taskTags: tags, autofocus: false,
                onSubmit: commitQuickAdd, onBlur: commitQuickAdd
            )
            .accessibilityIdentifier("task-title")
            if !hasTitle {
                Text("A Task needs a Title.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            TextField("Description", text: $description, axis: .vertical)
                .lineLimit(3...8)
                .focused($focus, equals: .description)
                .accessibilityIdentifier("task-description")
            Picker("Priority", selection: $priority) {
                ForEach(UInt8(1)...4, id: \.self) { Text(verbatim: "P\($0)").tag($0) }
            }
            .pickerStyle(.segmented)
            .onChange(of: priority) {
                quickAdd.ignorePriority()  // the last action wins
                quickAddRevision += 1
                save()
            }
            Section("Tags") {
                ForEach(tags, id: \.self) { tag in
                    Text(tag).foregroundStyle(model.tagEntry(tag).style(default: .primary))
                        .swipeActions(edge: .trailing) {
                            Button("Remove", role: .destructive) {
                                tags.removeAll { $0 == tag }
                                save()
                            }
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
                let suggested = suggestions
                ForEach(suggested.matches, id: \.self) { tag in
                    Button(tag) { addTag(tag) }
                }
                if suggested.create {
                    Button("Create Tag '\(newTag)'") { addTag(newTag) }
                }
            }
            Section("Blocked by") {
                ForEach(blockedBy, id: \.id) { blocker in
                    Group {
                        if blocker.closed {
                            Text(blocker.title).strikethrough().foregroundStyle(.secondary)
                        } else {
                            NavigationLink(blocker.title, value: blocker.id).disabled(!hasTitle)
                        }
                    }
                    .swipeActions(edge: .trailing) {
                        Button("Remove", role: .destructive) {
                            blockedBy.removeAll { $0.id == blocker.id }
                            saveBlockedBy()
                        }
                    }
                }
                if !blockerCandidates.isEmpty {
                    Menu("Add Blocked by") {
                        ForEach(blockerCandidates) { candidate in
                            Button(candidate.title) {
                                blockedBy.append(Blocker(id: candidate.id, title: candidate.title, closed: false))
                                saveBlockedBy()
                            }
                        }
                    }
                    .accessibilityIdentifier("task-add-blocker")
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(task.title)  // as last saved; also the back button's label on the Blocker's form
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            #if os(macOS)
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: done).disabled(!hasTitle)
                }
            #endif
        }
        .onChange(of: focus) { save() }
        .onChange(of: hasTitle) { save() }  // keeps the model's Back and Blocker rules current
        .task(id: text) {
            guard (try? await Task.sleep(for: Self.pause)) != nil else { return }
            save()
        }
        .onDisappear {
            commitQuickAdd()
            save(includingTypedTag: true)
        }
        .navigationBarBackButtonHidden(!hasTitle)
        .onAppear(perform: refresh)
    }

    /// Shows what is saved now: this Task may have been edited from a Blocker's form on top.
    private func refresh() {
        guard let saved = model.task(id: task.id) else { return }
        (title, description, priority, tags, blockedBy) =
            (saved.title, saved.description, saved.priority, saved.tags.map(\.name), saved.blockedBy)
        startQuickAdd(from: saved.title)
    }

    private func startQuickAdd(from title: String) {
        quickAdd = .fromTitle(title: title)
        quickAddRevision += 1
    }

    /// Applies the Shortcuts in the Title field: they leave the field, the Priority picker and the
    /// Tag list show the result. With none pending it just saves.
    private func commitQuickAdd() {
        guard quickAdd.priority() != nil || !quickAdd.tags().isEmpty,
            let saved = model.commit(quickAdd, to: task, description: description, priority: priority, tags: tags)
        else { return save() }
        (title, priority, tags) = (saved.title, saved.priority, saved.tags.map(\.name))
        startQuickAdd(from: saved.title)
    }

    /// Saves what is edited, if the Tags are valid; `includingTypedTag` adds a valid
    /// Tag typed but not submitted, as when the view is left.
    private func save(includingTypedTag: Bool = false) {
        var saved = tags
        if includingTypedTag, isValidTag(tag: newTag), !saved.contains(newTag) { saved.append(newTag) }
        model.edit(task, title: title, description: description, priority: priority, tags: saved)
    }

    private func saveBlockedBy() {
        model.setBlockedBy(task, to: blockedBy.map(\.id))
    }
}
