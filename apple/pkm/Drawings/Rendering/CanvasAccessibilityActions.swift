#if !os(macOS)
    import UIKit

    /// The Element menu's actions as accessibility custom actions on the
    /// Canvas (ADR-0006), so that VoiceOver and Switch Control users reach
    /// them without a long-press. They are there while a Selection exists and
    /// run the same commands as the menu's entries; the core says whether
    /// Group and Ungroup would do anything.
    extension CanvasMetalView {
        /// The actions the Canvas offers assistive technology, in the Element
        /// menu's order. Empty without a Selection; Group and Ungroup only
        /// where the core says they apply.
        var accessibleActions: [CanvasAccessibilityAction] {
            guard let editor, !editor.selection().isEmpty else { return [] }
            func action(
                _ title: LocalizedStringResource, _ run: @escaping @MainActor (CanvasMetalView) -> Void
            ) -> CanvasAccessibilityAction {
                CanvasAccessibilityAction(name: String(localized: title), run: run)
            }
            var actions = [
                action("Cut") { $0.cut(nil) },
                action("Copy") { $0.copy(nil) },
                action("Paste") { $0.pasteFromPasteboard() },
                action("Duplicate") { $0.perform { $0.duplicate() } },
            ]
            if editor.canGroup() {
                actions.append(action("Group") { $0.perform { $0.group() } })
            }
            if editor.canUngroup() {
                actions.append(action("Ungroup") { $0.perform { $0.ungroup() } })
            }
            actions += [
                action("Bring to Front") { $0.perform { $0.moveInZOrder(how: .toFront) } },
                action("Bring Forward") { $0.perform { $0.moveInZOrder(how: .forward) } },
                action("Send Backward") { $0.perform { $0.moveInZOrder(how: .backward) } },
                action("Send to Back") { $0.perform { $0.moveInZOrder(how: .toBack) } },
                action("Delete") { $0.perform { $0.deleteSelection() } },
            ]
            return actions
        }
    }

    /// One action the Canvas offers assistive technology: the name it is
    /// spoken with and the command it runs.
    struct CanvasAccessibilityAction {
        let name: String
        let run: @MainActor (CanvasMetalView) -> Void
    }
#endif
