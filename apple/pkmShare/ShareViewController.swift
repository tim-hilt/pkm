import SwiftUI

#if canImport(UIKit)
    import UIKit

    /// The Share extension's principal class: hosts the Save sheet.
    @objc(ShareViewController)
    final class ShareViewController: UIViewController {
        override func viewDidLoad() {
            super.viewDidLoad()
            let host = UIHostingController(rootView: root)
            addChild(host)
            host.view.frame = view.bounds
            host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            view.addSubview(host.view)
            host.didMove(toParent: self)
        }
    }
#else
    import AppKit

    @objc(ShareViewController)
    final class ShareViewController: NSViewController {
        override func loadView() {
            view = NSHostingView(rootView: root)
            preferredContentSize = NSSize(width: 420, height: 320)
        }
    }
#endif

extension ShareViewController {
    @MainActor fileprivate var root: some View {
        ShareRoot(items: extensionContext?.inputItems.compactMap { $0 as? NSExtensionItem } ?? []) {
            [weak extensionContext] in extensionContext?.completeRequest(returningItems: nil)
        }
    }
}
