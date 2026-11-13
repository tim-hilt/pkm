import Foundation
import Observation

/// The Tasks Context's state: the list being shown, from the core's store.
@MainActor @Observable
final class TasksModel {
    private let store: TaskStore
    private let revertWindow: Duration
    private var commitTimer: Task<Void, Never>?

    /// The list shown: Active or the Backlog.
    var planning: Planning = .active {
        didSet { reload() }
    }
    /// The Tasks planned as `planning`, in their order, each followed by its Subtasks.
    private(set) var tasks: [OpenTask] = []
    /// Parents whose Subtasks are hidden.
    private var collapsed: Set<String> = []

    /// `tasks` without the Subtasks of a collapsed Parent.
    var rows: [OpenTask] {
        var hiddenBelow: UInt32?
        return tasks.filter { task in
            if let depth = hiddenBelow {
                if task.depth > depth { return false }
                hiddenBelow = nil
            }
            if collapsed.contains(task.id) { hiddenBelow = task.depth }
            return true
        }
    }

    func isCollapsed(_ task: OpenTask) -> Bool { collapsed.contains(task.id) }

    /// Hides or shows a Parent's Subtasks.
    func toggleCollapsed(_ task: OpenTask) {
        if !collapsed.insert(task.id).inserted { collapsed.remove(task.id) }
    }

    /// The Close that can still be Reverted, for the toast.
    struct Closed: Equatable {
        let title: String
        let closing: Closing
    }
    private(set) var closed: Closed?

    /// A Close is saved once its toast has been shown for `revertWindow`.
    init(store: TaskStore, revertWindow: Duration = .seconds(5)) {
        self.store = store
        self.revertWindow = revertWindow
        reload()
    }

    /// Adds a Task and shows the list it landed in. False, adding nothing, if
    /// the Title is empty.
    @discardableResult
    func add(title: String, planning: Planning) -> Bool {
        guard (try? store.create(title: title, planning: planning)) != nil else { return false }
        if self.planning == planning { reload() } else { self.planning = planning }
        return true
    }

    /// Adds a Subtask under `parent` and shows it. False, adding nothing, if the Title is empty.
    @discardableResult
    func addSubtask(title: String, under parent: OpenTask) -> Bool {
        guard (try? store.createSubtask(parent: parent.id, title: title)) != nil else { return false }
        collapsed.remove(parent.id)
        reload()
        return true
    }

    /// Saves a Task's Title, Description, Priority and Tags (adding and removing
    /// what differs). False, saving nothing, if the Title is empty or a Tag is invalid.
    @discardableResult
    func edit(_ task: OpenTask, title: String, description: String, priority: UInt8, tags: [String]) -> Bool {
        guard tags.allSatisfy(isValidTag) else { return false }
        guard (try? store.edit(id: task.id, title: title, description: description, priority: priority)) != nil
        else { return false }
        for tag in task.tags where !tags.contains(tag) { _ = try? store.removeTag(id: task.id, tag: tag) }
        for tag in tags where !task.tags.contains(tag) { _ = try? store.addTag(id: task.id, tag: tag) }
        reload()
        return true
    }

    /// Makes a Task Active or moves it to the Backlog; it leaves the list shown.
    func plan(_ task: OpenTask, as planning: Planning) {
        guard (try? store.plan(id: task.id, planning: planning)) != nil else { return }
        reload()
    }

    /// Closes a Task, committing the previous Close first. Done leaves it in
    /// place, marked, Rejected removes it; either way nothing is saved until
    /// `commit`, which follows `revertWindow` later.
    func close(_ task: OpenTask, as closing: Closing) {
        commitTimer?.cancel()
        guard (try? store.close(id: task.id, closing: closing)) != nil else {
            closed = nil
            reload()
            return
        }
        closed = Closed(title: task.title, closing: closing)
        reload()
        commitTimer = Task { [revertWindow] in
            try? await Task.sleep(for: revertWindow)
            if !Task.isCancelled { commit() }
        }
    }

    /// The circle of a Task: Done if it is Open, Reverts the Close if it is Done.
    func toggleDone(_ task: OpenTask) {
        if task.done { revert() } else { close(task, as: .done) }
    }

    /// Takes the pending Close back, as if it never happened.
    func revert() {
        commitTimer?.cancel()
        store.revert()
        closed = nil
        reload()
    }

    /// Saves the pending Close, if any; a Done Task leaves the list.
    func commit() {
        commitTimer?.cancel()
        try? store.commit()
        closed = nil
        reload()
    }

    /// The known Tags starting with `prefix`, to pick from.
    func knownTags(prefix: String) -> [String] {
        (try? store.tagsWithPrefix(prefix: prefix)) ?? []
    }

    /// `text` without the characters a Tag can never contain.
    static func allowedTagText(_ text: String) -> String { stripForbiddenTagCharacters(text: text) }

    private func reload() {
        tasks = (try? store.tasks(planning: planning)) ?? []
    }
}

extension OpenTask: Identifiable {}
