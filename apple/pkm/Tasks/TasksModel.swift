import Foundation
import Observation

/// The Tasks Context's state: the list being shown, from the core's store.
@MainActor @Observable
final class TasksModel {
    private let store: TaskStore

    /// The list shown: Active or the Backlog.
    var planning: Planning = .active {
        didSet { reload() }
    }
    /// The Open Tasks planned as `planning`, in their order.
    private(set) var tasks: [OpenTask] = []

    init(store: TaskStore) {
        self.store = store
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

    /// Saves a Task's Title, Description and Priority. False, saving nothing, if
    /// the Title is empty.
    @discardableResult
    func edit(_ task: OpenTask, title: String, description: String, priority: UInt8) -> Bool {
        guard (try? store.edit(id: task.id, title: title, description: description, priority: priority)) != nil
        else { return false }
        reload()
        return true
    }

    private func reload() {
        tasks = (try? store.tasks(planning: planning)) ?? []
    }
}

extension OpenTask: Identifiable {}
