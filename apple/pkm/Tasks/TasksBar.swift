import SwiftUI

/// The Tasks bar: the Favorites, Filters. On iPhone and iPad it scrolls sideways when
/// the entries don't fit; on the Mac it is a segmented control in the toolbar, whose overflow takes them.
struct TasksBar: View {
    @Environment(TasksModel.self) private var model

    var body: some View {
        #if os(iOS)
            ScrollViewReader { scroll in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(model.bar, id: \.self) { entry in
                            Button {
                                model.list = entry
                            } label: {
                                label(entry)
                            }
                            .buttonStyle(.bordered)
                            .background(.background, in: .capsule)
                            .tint(entry == model.barEntry ? .accentColor : .secondary)
                            .id(entry)
                            .accessibilityAddTraits(entry == model.barEntry ? .isSelected : [])
                        }
                    }
                }
                .onChange(of: model.barEntry, initial: true) { scroll.scrollTo(model.barEntry, anchor: .center) }
            }
            .accessibilityIdentifier("tasks-lists")
        #else
            Picker("List", selection: Binding(get: { model.barEntry }, set: { model.list = $0 })) {
                ForEach(model.bar, id: \.self) { label($0).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityIdentifier("tasks-lists")
        #endif
    }

    private func label(_ entry: TasksList) -> Text {
        switch entry {
        case .filters: Text("Filters")
        case .filter(let id): Text(verbatim: model.filters.first { $0.id == id }?.title ?? "")
        }
    }
}
