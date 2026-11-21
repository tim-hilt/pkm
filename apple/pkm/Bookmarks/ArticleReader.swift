import SwiftUI

/// The Reader: a Bookmark's Article, read-only, in the Markdown styling of ADR-0028 with every
/// marker hidden, in the app's Appearance. Links open in the browser, and so does Open Page. It
/// reopens where the user left off (Reading Progress, saved whenever the Reader rests) and has
/// Immersive Mode: a tap hides everything but the Article, on macOS full screen does.
struct ArticleReader: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    let id: String

    @State private var position = ScrollPosition()
    @State private var spot = ReadingSpot()
    @State private var isImmersive = false

    private var bookmark: Bookmark? { model.bookmarks.first { $0.id == id } }

    var body: some View {
        ScrollView {
            if let bookmark, let markdown = model.article(of: bookmark) {
                ArticleText(
                    markdown: markdown, layout: spot.layout,
                    images: ImageSource(add: { _ in nil }, read: { model.image($0, of: id) })
                )
                .frame(maxWidth: 680)
                .padding(ReadingSpot.margin)
                .frame(maxWidth: .infinity)
            }
        }
        .scrollPosition($position)
        .onScrollGeometryChange(for: ReadingSpot.Sample.self, of: ReadingSpot.Sample.init) { _, sample in
            spot.sample = sample
            if let y = spot.restoreY() { position.scrollTo(y: y) }
        }
        .onScrollPhaseChange { _, phase in if phase == .idle { saveProgress() } }
        .onAppear { spot.reopen(at: bookmark?.progress.map { Int($0.offset) }) }
        .onDisappear { saveProgress() }
        .onChange(of: scenePhase) { if scenePhase == .background { saveProgress() } }
        .onTapGesture { isImmersive.toggle() }
        .navigationTitle(bookmark?.title ?? "")
        .toolbar {
            if let url = bookmark.flatMap({ URL(string: $0.address) }) {
                Button("Open Page", systemImage: "safari") { openURL(url) }
            }
            Button("Details", systemImage: "info.circle") { model.detailsID = id }
                .accessibilityIdentifier("bookmark-details")
        }
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            .toolbarVisibility(isImmersive ? .hidden : .automatic, for: .navigationBar)
            .statusBarHidden(isImmersive)
            // Hiding system overlays also lets the home indicator fade.
            .persistentSystemOverlays(isImmersive ? .hidden : .automatic)
            .animation(.default, value: isImmersive)
        #else
            .onAppear { isImmersive = NSApp.keyWindow?.styleMask.contains(.fullScreen) ?? false }
            .onReceive(NotificationCenter.default.publisher(for: NSWindow.didEnterFullScreenNotification)) { _ in
                isImmersive = true
            }
            .onReceive(NotificationCenter.default.publisher(for: NSWindow.didExitFullScreenNotification)) { _ in
                isImmersive = false
            }
            .windowToolbarFullScreenVisibility(.onHover)
        #endif
        .bookmarkDetailsSheet(isInFront: true)
        .accessibilityIdentifier("article-reader")
    }

    private func saveProgress() {
        guard let offset = spot.offsetToSave() else { return }
        model.saveProgress(id: id, offset: offset)
    }
}

/// Where the Reader is in its Article, between the scroll view's points and the Article's UTF-16
/// offsets (Bookmarks ADR-0002). Not observed: scrolling must not redraw the Reader.
@MainActor
private final class ReadingSpot {
    /// Space around the Article inside the scroll view.
    static let margin: CGFloat = 16

    /// What of the scroll view the mapping needs.
    struct Sample: Equatable {
        /// The content's point at the top of what is visible, below the bars.
        var visibleY: CGFloat
        /// How far the scroll view's own offset runs ahead of `visibleY`.
        var insetTop: CGFloat

        init() {
            visibleY = 0
            insetTop = 0
        }

        init(_ geometry: ScrollGeometry) {
            visibleY = geometry.contentOffset.y + geometry.contentInsets.top
            insetTop = geometry.contentInsets.top
        }
    }

    let layout = ArticleLayout()
    var sample = Sample()
    /// The offset to scroll to once the text is laid out; nil when there is none or it was reached.
    private var pending: Int?
    private var saved: Int?

    func reopen(at offset: Int?) {
        pending = offset
        saved = offset
    }

    /// The scroll offset that shows the pending Reading Progress at the top, once the text is
    /// laid out; nil if there is nothing to scroll to or it is not yet.
    func restoreY() -> CGFloat? {
        guard let offset = pending, layout.isLaidOut else { return nil }
        pending = nil
        return layout.y(ofOffset: offset) + Self.margin - sample.insetTop
    }

    /// The offset at the top of the view if it moved since it was last saved; nil while a restore
    /// is pending, so that reopening never overwrites where the user was.
    func offsetToSave() -> Int? {
        guard pending == nil, layout.isLaidOut else { return nil }
        let offset = layout.offset(atY: sample.visibleY - Self.margin)
        defer { saved = offset }
        return offset == saved ? nil : offset
    }
}

/// The text layout of the Article, to find where an offset is and which offset is where.
@MainActor
final class ArticleLayout {
    weak var manager: NSTextLayoutManager?

    var isLaidOut: Bool {
        guard let manager else { return false }
        return manager.offset(from: manager.documentRange.location, to: manager.documentRange.endLocation) > 0
    }

    /// The UTF-16 offset of the line at `y` in the text, the last line for a `y` below it.
    func offset(atY y: CGFloat) -> Int {
        guard let manager else { return 0 }
        manager.ensureLayout(for: manager.documentRange)
        let start = manager.documentRange.location
        guard let fragment = manager.textLayoutFragment(for: CGPoint(x: 0, y: max(0, y))) else {
            return y > 0 ? manager.offset(from: start, to: manager.documentRange.endLocation) : 0
        }
        let local = y - fragment.layoutFragmentFrame.minY
        let line = fragment.textLineFragments.last { $0.typographicBounds.minY <= local }
        return manager.offset(from: start, to: fragment.rangeInElement.location) + (line?.characterRange.location ?? 0)
    }

    /// Where in the text the line holding the UTF-16 `offset` starts.
    func y(ofOffset offset: Int) -> CGFloat {
        guard let manager else { return 0 }
        manager.ensureLayout(for: manager.documentRange)
        guard let location = manager.location(manager.documentRange.location, offsetBy: offset),
            let fragment = manager.textLayoutFragment(for: location)
        else { return 0 }
        let local = manager.offset(from: fragment.rangeInElement.location, to: location)
        let line = fragment.textLineFragments.last { $0.characterRange.location <= local }
        return fragment.layoutFragmentFrame.minY + (line?.typographicBounds.minY ?? 0)
    }
}

/// Markdown drawn as the Reader shows it: a TextKit 2 text view that can be selected from but not
/// edited, as high as its text. Images are drawn from the kept files, so offline too.
private struct ArticleText {
    let markdown: String
    let layout: ArticleLayout
    let images: ImageSource

    /// What the text view's delegate and the drawn images need to outlive an update.
    final class Coordinator {
        let display = MarkerDisplay()
        let attachments: ImageAttachments
        init(_ images: ImageSource) { attachments = ImageAttachments(images) }
    }

    func makeCoordinator() -> Coordinator { Coordinator(images) }

    func style(_ storage: NSTextStorage, _ coordinator: Coordinator) {
        MarkdownStyler.style(storage, caret: nil, images: coordinator.attachments)
    }
}

#if canImport(UIKit)
    extension ArticleText: UIViewRepresentable {
        func makeUIView(context: Context) -> UITextView {
            let view = UITextView(usingTextLayoutManager: true)
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate =
                context.coordinator.display
            view.isEditable = false
            view.isScrollEnabled = false
            view.backgroundColor = .clear
            view.textContainerInset = .zero
            view.textContainer.lineFragmentPadding = 0
            layout.manager = view.textLayoutManager
            return view
        }

        func updateUIView(_ view: UITextView, context: Context) {
            guard view.text != markdown else { return }
            view.text = markdown
            style(view.textStorage, context.coordinator)
        }

        func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextView, context: Context) -> CGSize? {
            let width = proposal.width ?? 320
            return CGSize(
                width: width, height: uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
            )
        }
    }
#else
    extension ArticleText: NSViewRepresentable {
        func makeNSView(context: Context) -> NSTextView {
            let view = NSTextView(usingTextLayoutManager: true)
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate =
                context.coordinator.display
            view.isEditable = false
            view.drawsBackground = false
            view.isVerticallyResizable = false
            view.isHorizontallyResizable = false
            view.textContainerInset = .zero
            view.textContainer?.lineFragmentPadding = 0
            view.textContainer?.widthTracksTextView = true
            layout.manager = view.textLayoutManager
            return view
        }

        func updateNSView(_ view: NSTextView, context: Context) {
            guard view.string != markdown else { return }
            view.string = markdown
            if let storage = view.textStorage { style(storage, context.coordinator) }
        }

        func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextView, context: Context) -> CGSize? {
            let width = proposal.width ?? 320
            nsView.textContainer?.size = CGSize(width: width, height: .greatestFiniteMagnitude)
            guard let layout = nsView.textLayoutManager else { return nil }
            layout.ensureLayout(for: layout.documentRange)
            return CGSize(width: width, height: layout.usageBoundsForTextContainer.height)
        }
    }
#endif
