import SwiftUI

/// The Tasks Context: the Filters and the Tasks they match, as Todoist-style rows. A Filter
/// opens on top of the Filters list; a Favorite opened from the bar is shown directly.
struct TasksView: View {
    @Environment(TasksModel.self) private var model
    @State private var isAdding = false
    @State private var isAddingFilter = false

    var body: some View {
        // In `ContentView`'s stack, whose path is `model.openFilterPath`.
        Group {
            switch model.barEntry {
            case .filter:
                TaskListView(emptyText: "No Open Tasks match.")
            case .filters:
                FiltersList { isAddingFilter = true }
            }
        }
        #if os(iOS)
            // The bar shares the row of the "+" below, in thumb reach; the toast sits above it.
            .safeAreaInset(edge: .bottom) {
                TasksBar()
                .padding(.leading, AddButtonMetrics.margin)
                .padding(.trailing, AddButtonMetrics.size + 2 * AddButtonMetrics.margin)
                .frame(height: AddButtonMetrics.size)
                .padding(.vertical, AddButtonMetrics.margin)
            }
        #endif
        .addButton(
            model.barEntry == .filters ? "New Filter" : "New Task",
            identifier: model.barEntry == .filters ? "add-filter" : "add-task"
        ) {
            if model.barEntry == .filters { isAddingFilter = true } else { isAdding = true }
        }
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)  // slim top bar: the sidebar button, no title
        #endif
        #if os(macOS)
            .toolbar { ToolbarItem(placement: .principal) { TasksBar() } }
        #endif
        .navigationDestination(for: String.self) { id in
            TaskListView(emptyText: "No Open Tasks match.")
                .addButton("New Task", identifier: "add-task") { isAdding = true }
                .taskSheets(isAdding: $isAdding, isInFront: true)
                .navigationTitle(model.filters.first { $0.id == id }?.title ?? "")
                #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                #endif
        }
        // Outside the switch, not on the content that swaps between empty and list when a Task is added.
        .taskSheets(isAdding: $isAdding, isInFront: model.openFilterPath.isEmpty)
        .sheet(isPresented: $isAddingFilter) { FilterSheet() }
    }
}

extension View {
    /// The New Task and edit sheets, presented only while this view `isInFront`: on the Mac a
    /// sheet of a view under a pushed one waits until it shows again.
    fileprivate func taskSheets(isAdding: Binding<Bool>, isInFront: Bool) -> some View {
        modifier(TaskSheets(isAdding: isAdding, isInFront: isInFront))
    }
}

private struct TaskSheets: ViewModifier {
    @Environment(TasksModel.self) private var model
    @Binding var isAdding: Bool
    let isInFront: Bool

    func body(content: Content) -> some View {
        content
            .sheet(isPresented: Binding(get: { isInFront && isAdding }, set: { isAdding = $0 })) {
                NewTaskSheet(model: model)
            }
            .sheet(
                isPresented: Binding(
                    get: { isInFront && !model.editing.isEmpty }, set: { if !$0 { model.closeEditing() } })
            ) { EditTaskSheet() }
    }
}

/// The Tasks of the list shown (`model.tasks`) as rows with their actions, and the toast of a
/// pending Close. Dragging orders only the Group of the row.
private struct TaskListView: View {
    @Environment(TasksModel.self) private var model
    let emptyText: LocalizedStringKey

    var body: some View {
        Group {
            if model.tasks.isEmpty && !model.isGrouped {  // a Group's heading shows even when empty
                ContentUnavailableView {
                    AppContext.tasks.label
                } description: {
                    Text(emptyText)
                }
            } else {
                List {
                    ForEach(model.groups, id: \.tag) { group in
                        Section {
                            ForEach(group.tasks) { task in
                                TaskRow(task: task, onCircle: { model.toggleDone(task) })
                                    .listRowSeparator(
                                        task.id == group.tasks.first?.id ? .hidden : .automatic, edges: .top
                                    )
                                    .contentShape(Rectangle())
                                    .onTapGesture { model.startEditing(task) }
                                    .swipeActions(edge: .trailing) {
                                        if !task.done {
                                            Button("Reject", role: .destructive) { model.close(task, as: .rejected) }
                                        }
                                    }
                                    .taskContextMenu(task)
                            }
                            // A drop lands within the Section: no target outside the row's Group.
                            .onMove { model.move(in: group.tag, from: $0, to: $1) }
                        } header: {
                            if let tag = group.tag {
                                Text(tag)
                                    .foregroundStyle(model.tagEntry(tag).style())
                                    .accessibilityIdentifier("group-\(tag)")
                            }
                        }
                    }
                }
                .listStyle(.plain)
            }
        }
        .toolbar { ToolbarItem(placement: .primaryAction) { SortMenu() } }
        .safeAreaInset(edge: .bottom) {
            // While the edit sheet is open, its own toast shows instead.
            if let pendingClose = model.pendingClose, model.editing.isEmpty {
                ClosedToast(closing: pendingClose)
            }
        }
        .animation(.default, value: model.pendingClose)
    }
}

/// The Filter's Sort: a Picker of the Sort keys, and a Direction section while the key has one.
private struct SortMenu: View {
    @Environment(TasksModel.self) private var model

    var body: some View {
        if let sort = model.sort {
            Menu {
                Picker(
                    "Sort",
                    selection: Binding(
                        get: { sortKey(sort: sort) }, set: { model.setSort(sortWithKey(sort: sort, key: $0)) })
                ) {
                    ForEach(sortKeys(), id: \.self) { key in
                        switch key {
                        case .manual: Text("Manual").tag(key)
                        case .priority: Text("Priority").tag(key)
                        }
                    }
                }
                if case .priority(let direction) = sort {
                    Section("Direction") {
                        Picker(
                            "Direction",
                            selection: Binding(get: { direction }, set: { model.setSort(.priority($0)) })
                        ) {
                            Text("Ascending").tag(Direction.ascending)
                            Text("Descending").tag(Direction.descending)
                        }
                    }
                }
            } label: {
                Label("Sort", systemImage: "arrow.up.arrow.down")
            }
            .accessibilityIdentifier("sort-menu")
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
        .padding(.horizontal)
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}

/// A Tag's name in its Tag Color, Default being the secondary grey.
private func tagText(_ tag: TagEntry) -> Text {
    Text(tag.name).foregroundStyle(tag.style())
}

/// A symbol and a text, close together, for the line of Tags.
private func marker(_ text: Text, symbol: String) -> some View {
    HStack(spacing: 3) {
        Image(systemName: symbol).imageScale(.small)
        text
    }
}

/// One Task: a circle, coloured by Priority and filled while Done, and beside
/// it the Title, the Description faded on one line, and the Tags fainter still.
/// A Parent shows how many of its Subtasks are Done, a Subtask its Parent's Title, before
/// the Tags. A Blocked Task has a lock at the end. `onCircle` is a tap on the circle.
struct TaskRow: View {
    let task: ListedTask
    /// Whether the Parent's Title is shown as a marker; not where the Parent is already in view.
    var showsParent = true
    let onCircle: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Button(action: onCircle) {
                Image(systemName: task.done ? "circle.inset.filled" : "circle")
                    .font(.title2)
                    .foregroundStyle(task.priorityColor)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(task.done ? "Revert Done" : "Done")
            VStack(alignment: .leading, spacing: 2) {
                Text(AttributedString(title: task.title))
                let preview = rowPreview(text: task.description)
                if !preview.isEmpty {
                    Text(AttributedString(title: preview))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                if task.hasMarkers(showsParent: showsParent) || !task.tags.isEmpty {
                    HStack(spacing: 8) {
                        if task.openSubtasks > 0 {
                            marker(Text(verbatim: "\(task.openSubtasks)"), symbol: "checklist")
                                .accessibilityLabel("\(task.openSubtasks) Open Subtasks")
                        }
                        if showsParent, let parentTitle = task.parentTitle {
                            marker(Text(AttributedString(title: parentTitle)), symbol: "arrow.turn.down.right")
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .opacity(0.7)
                        }
                        if let first = task.tags.first {
                            // One Text, so that too many Tags are cut off at the end, not each.
                            task.tags.dropFirst().reduce(tagText(first)) { Text("\($0) \(tagText($1))") }
                                .lineLimit(1)
                        }
                    }
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
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
    }
}

extension View {
    /// The context menu of a Task row (long press, right click): the Priority palette, a divider,
    /// then Reject. A pending Done (reverted at its circle) has none.
    func taskContextMenu(_ task: ListedTask) -> some View {
        modifier(TaskContextMenu(task: task))
    }
}

private struct TaskContextMenu: ViewModifier {
    @Environment(TasksModel.self) private var model
    let task: ListedTask

    func body(content: Content) -> some View {
        content.contextMenu {
            if !task.done {
                // The palette applies a tap at once; a row that moves does so animated.
                Picker(
                    "Priority",
                    selection: Binding(
                        get: { task.priority },
                        set: { priority in withAnimation { model.setPriority(task, to: priority) } })
                ) {
                    ForEach(UInt8(1)...4, id: \.self) { priority in
                        Label {
                            Text(verbatim: "P\(priority)")
                        } icon: {
                            Image(systemName: "circle.fill").foregroundStyle(ListedTask.color(ofPriority: priority))
                        }
                        .labelStyle(.iconOnly)
                        .tag(priority)
                    }
                }
                .pickerStyle(.palette)
                Divider()
                Button("Reject", systemImage: "xmark.circle", role: .destructive) {
                    model.close(task, as: .rejected)
                }
            }
        }
    }
}

extension ListedTask {
    fileprivate func hasMarkers(showsParent: Bool) -> Bool {
        openSubtasks > 0 || (showsParent && parentTitle != nil)
    }

    fileprivate var priorityColor: Color { Self.color(ofPriority: priority) }

    /// The colour of a Priority's circle, in the rows and the palette.
    fileprivate static func color(ofPriority priority: UInt8) -> Color {
        switch priority {
        case 1: .red
        case 2: .orange
        case 3: .blue
        default: .gray
        }
    }
}

/// Asks for a Title and a Description; the Task is added. The sheet stays open after each
/// Add, cleared and focused for the next Task; Cancel or swiping down ends it.
private struct NewTaskSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var quickAdd: TaskQuickAdd
    /// Quick Add's Title, kept up to date by the field.
    @State private var title = ""
    @State private var description = ""
    /// Changes per Task, so that the field redraws empty; it keeps its focus, and so the keyboard.
    @State private var revision = 0

    init(model: TasksModel) {
        _quickAdd = State(initialValue: model.newQuickAdd())
    }

    var body: some View {
        NavigationStack {
            Form {
                QuickAddField(
                    quickAdd: quickAdd, title: $title, revision: revision, refocusOnRevision: true, onSubmit: add
                )
                .accessibilityIdentifier("task-title")
                MarkdownEditor(text: $description, placeholder: "Description", images: model.images)
                    .accessibilityIdentifier("task-description")
            }
            .formStyle(.grouped)
            .navigationTitle("New Task")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add", action: add).disabled(title.isEmpty)
                }
            }
        }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium, .large])
        .onDisappear { model.releaseImages() }
    }

    private func add() {
        let priority = quickAdd.priority() ?? 4
        let tags = quickAdd.tags()
        guard model.add(title: title, description: description, priority: priority, tags: tags) else { return }
        model.releaseImages()
        quickAdd = model.newQuickAdd()
        title = ""
        description = ""
        revision += 1
    }
}

/// The sheet over the list: a stack of Tasks following the model's, the Task
/// opened first at the bottom and each Blocker or Subtask opened from one pushed on it.
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
                    model.open(id: id)
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
        .safeAreaInset(edge: .bottom) {
            if let pendingClose = model.pendingClose { ClosedToast(closing: pendingClose) }
        }
        .animation(.default, value: model.pendingClose)
        .interactiveDismissDisabled(model.isLeavingBlocked)
        // After the forms' last save, which their own onDisappear does.
        .onDisappear { Task { model.releaseImages() } }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium, .large])
    }
}

/// Edits a Task, saving as it goes: Title and Description after a pause in typing
/// and when their field loses focus, everything else at once; the Tags are picked in their own
/// sheet. The Title is Quick Add: the pause saves it without pending Shortcuts, and Return, losing focus and leaving the view apply them. While the Title is empty the rest still
/// saves (the last saved Title stays), a hint shows, and the view can't be left.
/// On macOS `done` closes the whole sheet. An Open Blocker, a Subtask or the Parent is a row that
/// opens its own form on top (the Parent just below is gone back to); Back then shows this one as last saved.
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
    @State private var isPickingTags = false
    @State private var blockedBy: [Blocker]

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
            MarkdownEditor(text: $description, placeholder: "Description", images: model.images, onBlur: save)
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
            TagsRow(tags: tags) { isPickingTags = true }
            if let parent = task.parent {
                Section("Parent") {
                    NavigationLink(value: parent) { Text(AttributedString(title: task.parentTitle ?? "")) }
                        .disabled(!hasTitle)
                        .accessibilityIdentifier("task-parent")
                }
            }
            SubtasksSection(task: task, canOpen: hasTitle)
            Section("Blocked by") {
                ForEach(blockedBy, id: \.id) { blocker in
                    Group {
                        if blocker.closed {
                            Text(AttributedString(title: blocker.title)).strikethrough().foregroundStyle(.secondary)
                        } else {
                            NavigationLink(value: blocker.id) { Text(AttributedString(title: blocker.title)) }
                                .disabled(!hasTitle)
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
                            Button {
                                blockedBy.append(Blocker(id: candidate.id, title: candidate.title, closed: false))
                                saveBlockedBy()
                            } label: {
                                Text(AttributedString(title: candidate.title))
                            }
                        }
                    }
                    .accessibilityIdentifier("task-add-blocker")
                }
            }
        }
        .formStyle(.grouped)
        // As last saved; also the back button's label on the Blocker's form.
        .navigationTitle(Text(AttributedString(title: task.title)))
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
        .onChange(of: hasTitle) { save() }  // keeps the model's Back and Blocker rules current
        .task(id: text) {
            guard (try? await Task.sleep(for: Self.pause)) != nil else { return }
            save()
        }
        .onDisappear {
            commitQuickAdd()
            save()
        }
        .sheet(isPresented: $isPickingTags) {
            TagPicker(selected: tags) {
                tags = $0
                isPickingTags = false
                save()
            }
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

    /// Saves what is edited, if the Tags are valid.
    private func save() {
        model.edit(task, title: title, description: description, priority: priority, tags: tags)
    }

    private func saveBlockedBy() {
        model.setBlockedBy(task, to: blockedBy.map(\.id))
    }
}
