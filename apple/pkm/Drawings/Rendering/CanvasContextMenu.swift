#if !os(macOS)
    import UIKit

    /// The Context menu (ADR-0006): the system's edit menu, opened by a finger
    /// held on the Canvas or a secondary click with a trackpad or mouse. The
    /// client only detects the hold and shows the menu; the core decides which
    /// menu it is and what the press selects (`Editor.longPress`). A tap, a
    /// drag and the Pencil never open it: the hold must stay inside the
    /// system's slop for the system's duration, and moving first leaves a
    /// normal drag.
    extension CanvasMetalView {
        /// Makes the Canvas open the menu on a hold or a secondary click. The
        /// recognizers watch beside the Canvas's own touch handling, never
        /// taking touches from it: the core knows a press that became a menu
        /// is no tap, so lifting afterwards changes nothing.
        func enableContextMenu() {
            let interaction = UIEditMenuInteraction(delegate: self)
            addInteraction(interaction)
            editMenuInteraction = interaction

            let hold = UILongPressGestureRecognizer(target: self, action: #selector(held(_:)))
            hold.allowedTouchTypes = [UITouch.TouchType.direct.rawValue as NSNumber]
            hold.cancelsTouchesInView = false
            addGestureRecognizer(hold)

            let click = UITapGestureRecognizer(target: self, action: #selector(secondaryClicked(_:)))
            click.allowedTouchTypes = [UITouch.TouchType.indirectPointer.rawValue as NSNumber]
            click.buttonMaskRequired = .secondary
            click.cancelsTouchesInView = false
            addGestureRecognizer(click)
        }

        @objc private func held(_ hold: UILongPressGestureRecognizer) {
            if hold.state == .began { presentContextMenu(at: hold.location(in: self), kind: .finger) }
        }

        @objc private func secondaryClicked(_ click: UITapGestureRecognizer) {
            if click.state == .ended { presentContextMenu(at: click.location(in: self), kind: .mouse) }
        }

        private func presentContextMenu(at point: CGPoint, kind: PointerKind) {
            guard let menu = contextMenuForPress(at: point, kind: kind) else { return }
            pendingContextMenu = menu
            editMenuInteraction?.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: point))
        }

        /// What a press of `kind` at `point` (in the Canvas's coordinates)
        /// opens: the core selects what is pressed and says which menu it is;
        /// `nil` if it is none. The point is remembered for a Paste from the
        /// menu, which the system hands over without one.
        func contextMenuForPress(at point: CGPoint, kind: PointerKind) -> UIMenu? {
            var result: ContextMenu?
            perform { result = $0.longPress(x: point.x, y: point.y, kind: kind) }
            switch result {
            case .element(let canGroup, let canUngroup):
                contextMenuPoint = point
                return elementMenu(canGroup: canGroup, canUngroup: canUngroup)
            case .canvas:
                contextMenuPoint = point
                return canvasMenu()
            case nil: return nil
            }
        }

        /// The system's own Paste item: it reads the pasteboard without the
        /// "Allow Paste" prompt a custom item would trigger. It lands in
        /// `paste(itemProviders:)`, at the remembered press point.
        private func pasteItem() -> UICommand {
            UICommand(
                title: String(localized: "Paste"), image: UIImage(systemName: "doc.on.clipboard"),
                action: #selector(UIResponder.paste(_:)))
        }

        /// The menu of empty Canvas: Paste and Select All. The
        /// Selection is left as it was.
        func canvasMenu() -> UIMenu {
            UIMenu(children: [
                pasteItem(),
                UIAction(
                    title: String(localized: "Select All"), image: UIImage(systemName: "checkmark.circle.badge.plus")
                ) {
                    [weak self] _ in self?.perform { $0.perform(command: .selectAll) }
                },
            ])
        }

        /// The menu of the Selection: Excalidraw's entries and order. Group and
        /// Ungroup are always there, enabled as the core says; each entry runs
        /// the command the same name has elsewhere.
        func elementMenu(canGroup: Bool, canUngroup: Bool) -> UIMenu {
            func item(
                _ title: LocalizedStringResource, _ symbol: String, disabled: Bool = false,
                destructive: Bool = false, _ run: @escaping (CanvasMetalView) -> Void
            ) -> UIAction {
                var attributes: UIMenuElement.Attributes = []
                if disabled { attributes.insert(.disabled) }
                if destructive { attributes.insert(.destructive) }
                return UIAction(
                    title: String(localized: title), image: UIImage(systemName: symbol), attributes: attributes
                ) {
                    [weak self] _ in
                    if let self { run(self) }
                }
            }
            let arrange = UIMenu(
                title: String(localized: "Arrange"), image: UIImage(systemName: "square.3.layers.3d"),
                children: [
                    item("Bring to Front", "square.3.layers.3d.top.filled") {
                        $0.perform { $0.moveInZOrder(how: .toFront) }
                    },
                    item("Bring Forward", "square.2.layers.3d.top.filled") {
                        $0.perform { $0.moveInZOrder(how: .forward) }
                    },
                    item("Send Backward", "square.2.layers.3d.bottom.filled") {
                        $0.perform { $0.moveInZOrder(how: .backward) }
                    },
                    item("Send to Back", "square.3.layers.3d.bottom.filled") {
                        $0.perform { $0.moveInZOrder(how: .toBack) }
                    },
                ])
            return UIMenu(children: [
                item("Cut", "scissors") { $0.cut(nil) },
                item("Copy", "doc.on.doc") { $0.copy(nil) },
                pasteItem(),
                item("Duplicate", "plus.square.on.square") { $0.perform { $0.duplicate() } },
                item("Group", "rectangle.3.group", disabled: !canGroup) { $0.perform { $0.group() } },
                item("Ungroup", "rectangle.3.offgrid", disabled: !canUngroup) { $0.perform { $0.ungroup() } },
                arrange,
                item("Delete", "trash", destructive: true) { $0.perform { $0.deleteSelection() } },
            ])
        }
    }

    extension CanvasMetalView: @preconcurrency UIEditMenuInteractionDelegate {
        func editMenuInteraction(
            _ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration,
            suggestedActions: [UIMenuElement]
        ) -> UIMenu? {
            defer { pendingContextMenu = nil }
            return pendingContextMenu
        }

        func editMenuInteraction(
            _ interaction: UIEditMenuInteraction, didEndFor configuration: UIEditMenuConfiguration
        ) {
            // After the chosen item has run: a Paste reads the point first.
            DispatchQueue.main.async { [weak self] in self?.contextMenuPoint = nil }
        }
    }
#endif
