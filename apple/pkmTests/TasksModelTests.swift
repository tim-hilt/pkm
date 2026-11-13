import Foundation
import Testing

@testable import pkm

@MainActor @Suite struct TasksModelTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)

    func openModel() throws -> TasksModel {
        TasksModel(
            store: try TaskStore.open(
                root: root.appending(path: "Tasks").path, deviceFolder: root.appending(path: "Device").path))
    }

    @Test func aNewTaskAppearsInTheListItWasAddedTo() throws {
        let model = try openModel()
        #expect(model.add(title: "Buy milk", planning: .active))
        #expect(model.tasks.map(\.title) == ["Buy milk"])
    }

    @Test func aTaskMovesBetweenTheBacklogAndActive() throws {
        let model = try openModel()
        model.add(title: "Later", planning: .backlog)
        model.plan(model.tasks[0], as: .active)
        #expect(model.tasks.isEmpty)
        model.planning = .active
        #expect(model.tasks.map(\.title) == ["Later"])
        model.plan(model.tasks[0], as: .backlog)
        #expect(model.tasks.isEmpty)
    }

    @Test func aDoneTaskStaysMarkedUntilCommittedThenLeavesAndStays() throws {
        let model = try openModel()
        model.add(title: "Buy milk", planning: .active)
        model.toggleDone(model.tasks[0])
        #expect(model.tasks.map(\.done) == [true])
        #expect(model.closed == .init(title: "Buy milk", closing: .done))
        #expect(try openModel().tasks.map(\.title) == ["Buy milk"])  // nothing saved yet
        model.commit()
        #expect(model.tasks.isEmpty && model.closed == nil)
        #expect(try openModel().tasks.isEmpty)
    }

    @Test func tappingTheFilledCircleReverts() throws {
        let model = try openModel()
        model.add(title: "Buy milk", planning: .active)
        model.toggleDone(model.tasks[0])
        model.toggleDone(model.tasks[0])
        #expect(model.tasks.map(\.done) == [false] && model.closed == nil)
        model.commit()
        #expect(try openModel().tasks.map(\.title) == ["Buy milk"])
    }

    @Test func aRejectedTaskLeavesAtOnceAndRevertBringsItBack() throws {
        let model = try openModel()
        model.add(title: "Oops", planning: .active)
        model.close(model.tasks[0], as: .rejected)
        #expect(model.tasks.isEmpty && model.closed?.closing == .rejected)
        model.revert()
        #expect(model.tasks.map(\.title) == ["Oops"])
    }

    @Test func aCloseIsCommittedWhenItsToastGoesAway() async throws {
        let model = TasksModel(
            store: try TaskStore.open(
                root: root.appending(path: "Tasks").path, deviceFolder: root.appending(path: "Device").path),
            revertWindow: .milliseconds(20))
        model.add(title: "Gone", planning: .active)
        model.close(model.tasks[0], as: .rejected)
        try await Task.sleep(for: .milliseconds(300))
        #expect(model.closed == nil)
        #expect(try openModel().tasks.isEmpty)
    }

    @Test func addingToTheBacklogShowsTheBacklog() throws {
        let model = try openModel()
        model.add(title: "Later", planning: .backlog)
        #expect(model.planning == .backlog)
        #expect(model.tasks.map(\.title) == ["Later"])
        model.planning = .active
        #expect(model.tasks.isEmpty)
    }

    @Test func anEmptyTitleAddsNothing() throws {
        let model = try openModel()
        #expect(!model.add(title: "  \n", planning: .active))
        #expect(model.tasks.isEmpty)
    }

    @Test func tasksStayAcrossLaunches() throws {
        try openModel().add(title: "Kept", planning: .active)
        #expect(try openModel().tasks.map(\.title) == ["Kept"])
    }

    @Test func editingChangesTheRowAndStaysAcrossLaunches() throws {
        let model = try openModel()
        model.add(title: "Buy milk", planning: .active)
        #expect(model.edit(model.tasks[0], title: "Buy oat milk", description: "Two litres", priority: 1, tags: []))
        #expect(model.tasks.map(\.title) == ["Buy oat milk"])
        #expect(model.tasks.map(\.description) == ["Two litres"])
        #expect(model.tasks.map(\.priority) == [1])
        #expect(try openModel().tasks.map(\.priority) == [1])
    }

    @Test func editingToAnEmptyTitleChangesNothing() throws {
        let model = try openModel()
        model.add(title: "Keep", planning: .active)
        #expect(!model.edit(model.tasks[0], title: " ", description: "x", priority: 1, tags: []))
        #expect(model.tasks.map(\.title) == ["Keep"])
    }

    @Test func tagsAreAddedAndRemovedOnSave() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        #expect(model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["home", "work/x"]))
        #expect(model.tasks[0].tags == ["home", "work/x"])
        #expect(model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["work/x"]))
        #expect(try openModel().tasks[0].tags == ["work/x"])
    }

    @Test func anInvalidTagSavesNothing() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        #expect(!model.edit(model.tasks[0], title: "new", description: "", priority: 4, tags: ["Bad"]))
        #expect(model.tasks[0].title == "t")
    }

    @Test func knownTagsAreSuggestedByPrefix() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["home", "work"])
        #expect(model.knownTags(prefix: "w") == ["work"])
    }

    @Test func forbiddenTagCharactersAreStripped() {
        #expect(TasksModel.allowedTagText("Work 1/x") == "ork1/x")
    }
}
