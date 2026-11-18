import Foundation
import Observation

/// What the Tasks Context shows: the list of Filters, or the Tasks a Filter (by ID) matches.
enum TasksList: Hashable {
    /// The Filters list.
    case filters
    case filter(String)

    /// The Filter's ID, if it is a Filter.
    var filterID: String? {
        if case .filter(let id) = self { id } else { nil }
    }

    /// The list as stored on a Device: `filters` or `filter:<id>`.
    var stored: String {
        switch self {
        case .filters: "filters"
        case .filter(let id): "filter:\(id)"
        }
    }

    init?(stored: String) {
        switch stored {
        case "filters": self = .filters
        case _ where stored.hasPrefix("filter:"): self = .filter(String(stored.dropFirst("filter:".count)))
        default: return nil
        }
    }
}

/// The Tasks Context's state: the list being shown, from the core's store.
@MainActor @Observable
final class TasksModel {
    private let store: TaskStore
    private let revertWindow: Duration
    private var commitTimer: Task<Void, Never>?
    @ObservationIgnored private let defaults: UserDefaults?
    private static let lastListKey = "lastTasksList"

    /// The list shown; this Device opens on it again.
    var list: TasksList = .filters {
        didSet {
            defaults?.set(list.stored, forKey: Self.lastListKey)
            reload()
        }
    }
    /// The Tasks of the open sheet's stack, by id, first opened first: a Blocker
    /// or Subtask opened from a Task is pushed on it. Empty when no sheet is open. Over the
    /// list shown, which opening a Blocker never changes.
    private(set) var editing: [String] = []
    /// Whether the sheet can't be left or popped: the Task on top was last
    /// saved with an empty Title, which keeps the stack as it is until it has one.
    private(set) var isLeavingBlocked = false
    /// The Tasks of the list shown, in their order, each followed by its Subtasks; none while the
    /// Filters list is shown.
    private(set) var tasks: [ListedTask] = []
    /// Every Filter, in the Filters list's order.
    private(set) var filters: [Filter] = []
    /// Every known Tag with its Tag Color, alphabetically, whether or not a Task carries it.
    private(set) var allTags: [TagEntry] = []

    /// Opens the sheet of the Open Task a link or address names, as the only one in the stack,
    /// over the list shown if it holds the Task, else over the first Favorite that does. `nil` opens
    /// no sheet; an unknown or Closed `id` shows the list a Device opens on without a last one.
    func openLinked(id: String?) {
        closeEditing()
        guard let id else { return }
        let candidates = [list] + filters.filter(\.favorite).map { TasksList.filter($0.id) }
        guard let task = task(id: id) else {
            list = startList
            return
        }
        if let home = candidates.first(where: { tasks(in: $0).contains { $0.id == id } }) { list = home }
        startEditing(task)
    }

    /// Opens the sheet of an Open `task` of the list shown, as the only one in the stack.
    func startEditing(_ task: ListedTask) {
        editing = [task.id]
        isLeavingBlocked = false
    }

    /// Pushes the Open Task `id`, a Blocker, Subtask or Parent of the one on top, onto the
    /// stack, leaving the list shown as it is. The Task just below is popped to instead.
    /// False, changing nothing, if it is Closed or gone, or while the Title on top is empty.
    @discardableResult
    func open(id: String) -> Bool {
        guard !editing.isEmpty, !isLeavingBlocked, task(id: id) != nil else { return false }
        if editing.dropLast().last == id { editing.removeLast() } else { editing.append(id) }
        return true
    }

    /// Pops the Task on top. False, popping nothing, while its Title is empty or it is the last.
    @discardableResult
    func back() -> Bool {
        guard editing.count > 1, !isLeavingBlocked else { return false }
        editing.removeLast()
        return true
    }

    /// Closes the sheet, whatever the stack's depth.
    func closeEditing() {
        editing = []
        isLeavingBlocked = false
    }

    /// The Open Task `id` as last saved, whichever list shows it.
    func task(id: String) -> ListedTask? {
        tasks.first { $0.id == id } ?? openTasks().first { $0.id == id }
    }

    /// Every Open Task, Subtasks included.
    private func openTasks() -> [ListedTask] {
        [Planning.active, .backlog].flatMap { (try? store.tasks(planning: $0)) ?? [] }
    }

    /// The Tasks `list` shows, in its order; none for the Filters list.
    private func tasks(in list: TasksList) -> [ListedTask] {
        list.filterID.flatMap { try? store.filterTasks(id: $0) } ?? []
    }

    /// The direct Subtasks of the Open Task `id`, in list order: Open, or Done and still
    /// Revertable; never Rejected. From either list.
    func subtasks(of id: String) -> [ListedTask] {
        _ = tasks  // reloads change it: views asking for Subtasks update with the list
        return (try? store.subtasks(parent: id)) ?? []
    }

    /// The Close that can still be Reverted, for the toast.
    private(set) var pendingClose: Closing?

    /// The bar's entries: the Favorites in the Filters list's order, then Filters.
    var bar: [TasksList] {
        filters.filter(\.favorite).map { .filter($0.id) } + [.filters]
    }

    /// Where a Device with no usable last list starts: the first Favorite, or the Filters list.
    private var startList: TasksList {
        filters.first(where: \.favorite).map { .filter($0.id) } ?? .filters
    }

    /// The bar's entry that shows the list shown: a Filter that isn't a Favorite is under Filters.
    var barEntry: TasksList {
        bar.contains(list) ? list : .filters
    }

    /// The Filter open on top of the Filters list, as the navigation path; going back leaves
    /// it. A Favorite is no part of it.
    var openFilterPath: [String] {
        get { if case .filter(let id) = list, barEntry == .filters { [id] } else { [] } }
        set {
            if let id = newValue.last {
                list = .filter(id)
            } else if case .filter = list {
                list = .filters
            }
        }
    }

    /// A Close is saved once its toast has been shown for `revertWindow`. `defaults` holds the last
    /// list, which opens again unless it is a Filter that is gone; then the first Favorite, or the Filters
    /// list. `nil` remembers nothing.
    init(store: TaskStore, revertWindow: Duration = .seconds(5), defaults: UserDefaults? = nil) {
        self.store = store
        self.revertWindow = revertWindow
        self.defaults = defaults
        reload()
        if let last = defaults?.string(forKey: Self.lastListKey).flatMap(TasksList.init(stored:)),
            last.filterID.map({ id in filters.contains { $0.id == id } }) ?? true
        {
            list = last
        } else {
            list = startList
        }
    }

    /// Adds a Task, with a Description, Priority and Tags if given. In a Filter it gets the Tags the
    /// Query requires, so that it stays there. False, adding nothing, if the Title is empty or a Tag invalid.
    @discardableResult
    func add(
        title: String, description: String = "", priority: UInt8 = 4, tags: [String] = []
    ) -> Bool {
        guard
            let task = try? store.create(
                title: title, planning: .active, priority: priority, tags: tags, filter: list.filterID)
        else {
            return false
        }
        describe(task, description)
        reload()
        return true
    }

    /// Adds a Subtask under `parent`, with a Description, Priority and Tags if given, and shows it.
    /// False, adding nothing, if the Title is empty or a Tag invalid.
    @discardableResult
    func addSubtask(
        title: String, description: String = "", priority: UInt8 = 4, tags: [String] = [], under parent: ListedTask
    ) -> Bool {
        guard
            let task = try? store.createSubtask(parent: parent.id, title: title, priority: priority, tags: tags)
        else { return false }
        describe(task, description)
        reload()
        return true
    }

    /// Gives a Task just created its Description, if any.
    private func describe(_ task: ListedTask, _ description: String) {
        guard !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        _ = try? store.edit(
            id: task.id, title: task.title, description: description, priority: task.priority, addTags: [])
    }

    /// Saves a Task's Title, Description, Priority and Tags (adding and removing
    /// what differs from what is saved now, not from `task`, which may be stale).
    /// An empty Title keeps the last saved one and saves the rest. False, saving
    /// nothing, if a Tag is invalid. The store writes only what differs, so saving twice is quiet.
    @discardableResult
    func edit(_ task: ListedTask, title: String, description: String, priority: UInt8, tags: [String]) -> Bool {
        guard tags.allSatisfy(isValidTag) else { return false }
        let saved = current(task)
        let isEmpty = title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        if task.id == editing.last { isLeavingBlocked = isEmpty }
        let title = isEmpty ? saved.title : title
        let added = tags.filter { tag in !saved.tags.contains { $0.name == tag } }
        guard
            (try? store.edit(
                id: task.id, title: title, description: description, priority: priority, addTags: added)) != nil
        else { return false }
        for tag in saved.tags.map(\.name) where !tags.contains(tag) { _ = try? store.removeTag(id: task.id, tag: tag) }
        reload()
        return true
    }

    /// Commits Quick Add's Shortcuts to a Task being edited: its Title without them, a Priority
    /// Shortcut replacing `priority`, its Tags added to `tags` (never removed), saved in one write
    /// like `edit`. Returns the Task as saved, or nil, changing nothing, if only Shortcuts were typed
    /// or the save failed.
    func commit(
        _ quickAdd: TaskQuickAdd, to task: ListedTask, description: String, priority: UInt8, tags: [String]
    ) -> ListedTask? {
        guard !quickAdd.title().isEmpty else { return nil }
        let merged = tags + quickAdd.tags().filter { !tags.contains($0) }
        guard
            edit(
                task, title: quickAdd.title(), description: description, priority: quickAdd.priority() ?? priority,
                tags: merged)
        else { return nil }
        return self.task(id: task.id)
    }

    /// Makes a Task Blocked by exactly the Tasks `ids` (adding and removing what
    /// differs from what is saved now). False if one is refused, which `blockerCandidates` never offers.
    @discardableResult
    func setBlockedBy(_ task: ListedTask, to ids: [String]) -> Bool {
        let current = self.current(task).blockedBy.map(\.id)
        for id in current where !ids.contains(id) { _ = try? store.removeBlocker(id: task.id, blocker: id) }
        defer { reload() }
        for id in ids where !current.contains(id) {
            guard (try? store.addBlocker(id: task.id, blocker: id)) != nil else { return false }
        }
        return true
    }

    /// The Open Tasks of both lists `task` could be Blocked by (see the store).
    func blockerCandidates(for task: ListedTask) -> [ListedTask] {
        (try? store.blockerCandidates(id: task.id)) ?? []
    }

    /// Moves the row at `source` to land before the row at `destination` of `tasks`
    /// (`onMove`'s meaning); past the last row, to the end of the list. The Parent stays. Only the
    /// Filter's own order changes.
    func move(from source: IndexSet, to destination: Int) {
        guard let id = list.filterID, let index = source.first, index != destination else { return }
        let target = destination < tasks.count ? tasks[destination] : nil
        try? store.moveInFilter(id: id, task: tasks[index].id, before: target?.id)
        reload()
    }

    /// Moves the Subtask at `source` of `subtasks(of: parent)` to land before the one at
    /// `destination` (`onMove`'s meaning); past the last, after it. It stays among its siblings.
    func moveSubtask(of parent: String, from source: IndexSet, to destination: Int) {
        let siblings = subtasks(of: parent)
        guard let index = source.first, index != destination else { return }
        let target = destination < siblings.count ? siblings[destination] : nil
        _ = try? store.moveSubtask(id: siblings[index].id, before: target?.id)
        reload()
    }

    /// Closes a Task, committing the previous Close first. Done leaves it in
    /// place, marked, Rejected removes it; either way nothing is saved until
    /// `commit`, which follows `revertWindow` later.
    func close(_ task: ListedTask, as closing: Closing) {
        commitTimer?.cancel()
        guard (try? store.close(id: task.id, closing: closing)) != nil else {
            pendingClose = nil
            reload()
            return
        }
        pendingClose = closing
        reload()
        commitTimer = Task { [revertWindow] in
            try? await Task.sleep(for: revertWindow)
            if !Task.isCancelled { commit() }
        }
    }

    /// The circle of a Task: Done if it is Open, Reverts the Close if it is Done.
    func toggleDone(_ task: ListedTask) {
        if task.done { revert() } else { close(task, as: .done) }
    }

    /// Takes the pending Close back, as if it never happened.
    func revert() {
        commitTimer?.cancel()
        store.revert()
        pendingClose = nil
        reload()
    }

    /// Saves the pending Close, if any; a Done Task leaves the list.
    func commit() {
        commitTimer?.cancel()
        try? store.commit()
        pendingClose = nil
        reload()
    }

    /// Creates a Filter at the end of the Filters list. False, creating nothing, if the Title is
    /// empty or the Query can't be read (see `queryProblem`).
    @discardableResult
    func addFilter(title: String, query: String) -> Bool {
        guard (try? store.createFilter(title: title, query: query)) != nil else { return false }
        reload()
        return true
    }

    /// Changes a Filter's Title and Query. False, changing nothing, if `addFilter` would refuse them.
    @discardableResult
    func editFilter(_ id: String, title: String, query: String) -> Bool {
        guard (try? store.editFilter(id: id, title: title, query: query)) != nil else { return false }
        reload()
        return true
    }

    /// Marks a Filter as a Favorite, which gives it an entry in the bar, or unmarks it.
    func setFavorite(_ id: String, to favorite: Bool) {
        try? store.setFavorite(id: id, favorite: favorite)
        reload()
    }

    /// Deletes a Filter; no Task changes. Showing it, the Filters list shows instead.
    func deleteFilter(_ id: String) {
        try? store.deleteFilter(id: id)
        reload()
    }

    /// Moves the Filter at `source` to land before the one at `destination` of `filters`
    /// (`onMove`'s meaning); past the last, to the end.
    func moveFilters(from source: IndexSet, to destination: Int) {
        guard let index = source.first, index != destination else { return }
        let target = destination < filters.count ? filters[destination] : nil
        try? store.moveFilter(id: filters[index].id, before: target?.id)
        reload()
    }

    /// What is wrong with the Query `text`, or which Tags it names that don't exist; saves nothing.
    func checkQuery(_ text: String) -> QueryCheck { store.checkQuery(text: text) }

    /// `text` as a Query may be typed: lowercase, without characters neither a Tag nor the Query hold.
    static func allowedQueryText(_ text: String) -> String { typedQuery(text: text) }

    /// The known Tags to suggest for the Tag being typed at the end of the Query `text`, as Quick Add
    /// suggests them; none if it doesn't end in one.
    func suggestQueryTags(_ text: String) -> [String] {
        guard let word = unfinishedQueryTag(text: text) else { return [] }
        return suggestTags(word, leaveOut: []).matches
    }

    /// The Query `text` with the Tag being typed at its end completed to `tag`.
    static func completingQuery(_ text: String, with tag: String) -> String {
        let word = unfinishedQueryTag(text: text) ?? ""
        return String(text.dropLast(word.count)) + tag
    }

    /// Gives a Tag the Tag Color `color` (`#rrggbb`), or Default for nil; the lists show it at once.
    func setTagColor(_ tag: String, to color: String?) {
        try? store.setTagColor(tag: tag, color: color)
        reload()
    }

    /// Deletes a Tag: it is Removed from every Task, Open or Closed, and its Tag Color is forgotten.
    func deleteTag(_ tag: String) {
        try? store.deleteTag(tag: tag)
        reload()
    }

    /// The Tag Color of a Tag by name; Default if it isn't known.
    func tagEntry(_ name: String) -> TagEntry {
        allTags.first { $0.name == name } ?? TagEntry(name: name, color: nil)
    }

    /// What a Tag field offers for the typed `query`, leaving out the Tags the
    /// Task has: known Tags starting with it, then containing it, and whether to offer Create.
    func suggestTags(_ query: String, leaveOut: [String]) -> TagSuggestions {
        (try? store.suggestTags(query: query, leaveOut: leaveOut)) ?? TagSuggestions(matches: [], create: false)
    }

    /// `text` without the characters a Tag can never contain.
    static func allowedTagText(_ text: String) -> String { stripForbiddenTagCharacters(text: text) }

    /// Sync wrote or deleted Task files: the list shows what is there now.
    func syncDidChange() { reload() }

    /// `task` as last saved, if it is in either list; else as given.
    private func current(_ task: ListedTask) -> ListedTask {
        self.task(id: task.id) ?? task
    }

    /// Reads the list shown and the Filters again; a Filter that is gone (Deleted here or by Sync)
    /// shows the Filters list instead.
    private func reload() {
        filters = (try? store.filters()) ?? []
        if let id = list.filterID, !filters.contains(where: { $0.id == id }) {
            list = .filters  // reloads
            return
        }
        tasks = tasks(in: list)
        allTags = store.tags()
    }
}

extension ListedTask: Identifiable {}
extension Filter: Identifiable {}
