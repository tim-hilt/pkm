import SwiftUI

/// The Reader: a Bookmark's Article, read-only, in the Markdown styling of ADR-0028 with every
/// marker hidden, in the app's Appearance. Links open in the browser, and so does Open Page. It
/// reopens where the user left off (Reading Progress, saved whenever the Reader rests) and has
/// Immersive Mode: a tap hides everything but the Article, on macOS full screen does. Selecting text
/// offers Highlight, in the Default Highlight Color, and Comment, on the spot; a tap on either opens
/// the Books' popover to recolor, comment on or remove it. Find (the magnifier, ⌘F on the Mac) is
/// Article Search: a bar at the bottom marks every match in the Article and Jumps between them.
struct ArticleReader: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(LibraryModel.self) private var library
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    let id: String

    @State private var position = ScrollPosition()
    @State private var spot = ReadingSpot()
    @State private var isImmersive = false
    @State private var markdown: String?
    @State private var markup = ArticleMarkup(highlights: [], pageComments: [])
    @State private var editing: MarkupEdit?
    @State private var isFinding = false
    @State private var query = ""
    @State private var matches: [NSRange] = []
    /// The index in `matches` of the match the Reader last Jumped to.
    @State private var current = 0

    private var bookmark: Bookmark? { model.bookmarks.first { $0.id == id } }

    var body: some View {
        ScrollView {
            if let markdown {
                ArticleText(
                    markdown: markdown, layout: spot.layout,
                    images: ImageSource(add: { _ in nil }, read: { model.image($0, of: id) }),
                    markup: markup, found: isFinding ? FoundText(ranges: matches, current: current) : FoundText(),
                    actions: MarkupActions(highlight: highlight, comment: comment, tap: tapped)
                )
                .frame(maxWidth: 680)
                .popover(item: $editing, attachmentAnchor: .rect(.rect(editing?.rect ?? .zero))) { edit in
                    popover(edit)
                }
                .padding(ReadingSpot.margin)
                .frame(maxWidth: .infinity)
                // Taps beside the text; those on it come from the text view.
                .background(Color.clear.contentShape(Rectangle()).onTapGesture { isImmersive.toggle() })
            }
        }
        .scrollPosition($position)
        .onScrollGeometryChange(for: ReadingSpot.Sample.self, of: ReadingSpot.Sample.init) { _, sample in
            spot.sample = sample
            if let y = spot.restoreY() { position.scrollTo(y: y) }
        }
        .onScrollPhaseChange { _, phase in if phase == .idle { saveProgress() } }
        .onAppear {
            markdown = bookmark.flatMap(model.article(of:))
            markup = model.markup(of: id)
            spot.reopen(at: bookmark?.progress.map { Int($0.offset) })
            if let launched = model.launchFind {
                model.launchFind = nil
                query = launched
                isFinding = true
            }
        }
        .safeAreaInset(edge: .bottom) {
            if isFinding {
                FindBar(query: $query, count: matches.count, current: current, jump: jump) { isFinding = false }
            }
        }
        .onChange(of: query) { find() }
        .onChange(of: isFinding) { if !isFinding { query = "" } }
        .onDisappear { saveProgress() }
        .onChange(of: scenePhase) { if scenePhase == .background { saveProgress() } }
        .navigationTitle(bookmark?.title ?? "")
        .toolbar {
            if let url = bookmark.flatMap({ URL(string: $0.address) }) {
                Button("Open Page", systemImage: "safari") { openURL(url) }
            }
            Button("Find in Article", systemImage: "magnifyingglass") { isFinding.toggle() }
                .keyboardShortcut("f")
                .accessibilityIdentifier("article-find")
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

    /// Searches the Article for `query` and Jumps to the first match.
    private func find() {
        matches = model.searchArticle(query, in: id)
        current = 0
        scroll(toMatch: 0)
    }

    /// Jumps to the next match, the previous one for -1, going around at either end.
    private func jump(_ step: Int) {
        guard !matches.isEmpty else { return }
        current = (current + step + matches.count) % matches.count
        scroll(toMatch: current)
    }

    private func scroll(toMatch index: Int) {
        guard matches.indices.contains(index) else { return }
        position.scrollTo(y: spot.y(ofOffset: matches[index].location))
    }

    private func saveProgress() {
        guard let offset = spot.offsetToSave() else { return }
        model.saveProgress(id: id, offset: offset)
    }

    private func highlight(_ range: NSRange) {
        guard let markdown,
            model.highlight(range, of: markdown, in: id, color: library.defaultHighlightColor.rgb) != nil
        else { return }
        markup = model.markup(of: id)
    }

    /// Puts a Comment on the spot, and opens it for typing.
    private func comment(at offset: Int) {
        guard let markdown, let added = model.commentSpot(at: offset, in: id) else { return }
        markup = model.markup(of: id)
        let length = markdown.utf16.count
        let character = NSRange(location: max(0, min(offset, length - 1)), length: 1)
        open(.comment(added.id), over: character, startsEditing: true)
    }

    /// A tap at `point` in the text: on Markup it opens it, elsewhere it is Immersive Mode's.
    private func tapped(at point: CGPoint) {
        let length = markdown?.utf16.count ?? 0
        if let offset = spot.layout.offset(at: point), let target = markup.target(at: offset, length: length),
            let range = markup.range(of: target, length: length)
        {
            open(target, over: range)
        } else {
            #if os(iOS)
                isImmersive.toggle()
            #endif
        }
    }

    private func open(_ target: MarkupTarget, over range: NSRange, startsEditing: Bool = false) {
        editing = MarkupEdit(
            target: target, rect: spot.layout.rect(of: range) ?? .zero, startsEditing: startsEditing)
    }

    @ViewBuilder private func popover(_ edit: MarkupEdit) -> some View {
        switch edit.target {
        case .highlight(let highlight):
            if let found = markup.highlights.first(where: { $0.id == highlight }) {
                HighlightPopover(
                    colors: library.highlightColors, color: RGB(found.color), comment: found.comment,
                    onColor: { color in
                        model.setColor(Rgb(color), ofHighlight: highlight, in: id)
                        markup = model.markup(of: id)
                    },
                    onComment: { text in
                        model.setComment(text, ofHighlight: highlight, in: id)
                        markup = model.markup(of: id)
                    },
                    onRemove: {
                        editing = nil
                        model.removeHighlight(highlight, in: id)
                        markup = model.markup(of: id)
                    })
            }
        case .comment(let comment):
            PageCommentPopover(
                text: markup.pageComments.first { $0.id == comment }?.text ?? "", startsEditing: edit.startsEditing,
                onText: { text in
                    model.setText(text, ofComment: comment, in: id)
                    markup = model.markup(of: id)
                },
                onRemove: {
                    editing = nil
                    model.removeComment(comment, in: id)
                    markup = model.markup(of: id)
                })
        }
    }
}

/// The Markup under the open popover, and where it is in the text.
private struct MarkupEdit: Identifiable {
    let target: MarkupTarget
    let rect: CGRect
    var startsEditing = false

    var id: String {
        switch target {
        case .highlight(let id), .comment(let id): id
        }
    }
}

/// The Article Search matches to mark in the text, and which of them is the current one.
struct FoundText: Equatable {
    var ranges: [NSRange] = []
    var current: Int?
    init() {}
    init(ranges: [NSRange], current: Int?) {
        self.ranges = ranges
        self.current = current
    }
}

/// Article Search's bar: the query, "3 of 12", Jumps to the previous and next match, and Done.
private struct FindBar: View {
    @Binding var query: String
    let count: Int
    let current: Int
    /// Jumps by a step: 1 to the next match, -1 to the previous one.
    let jump: (Int) -> Void
    let done: () -> Void
    @FocusState private var isFocused: Bool

    var body: some View {
        HStack(spacing: 12) {
            TextField("Find in Article", text: $query)
                .textFieldStyle(.roundedBorder)
                .focused($isFocused)
                .autocorrectionDisabled()
                .onSubmit { jump(1) }
                .accessibilityIdentifier("article-find-field")
            if !query.isEmpty {
                Text(count == 0 ? "No matches" : "\(current + 1) of \(count)")
                    .font(.caption).monospacedDigit().foregroundStyle(.secondary)
            }
            Button("Previous Match", systemImage: "chevron.up") { jump(-1) }.disabled(count == 0)
            Button("Next Match", systemImage: "chevron.down") { jump(1) }.disabled(count == 0)
            Button("Done", action: done)
        }
        .labelStyle(.iconOnly)
        .padding(.horizontal).padding(.vertical, 8)
        .background(.bar)
        .onAppear { isFocused = true }
    }
}

/// What the text view reports: a selection to Highlight, a spot to Comment on, a tap.
struct MarkupActions {
    let highlight: (NSRange) -> Void
    let comment: (Int) -> Void
    let tap: (CGPoint) -> Void
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
        return y(ofOffset: offset)
    }

    /// The scroll offset that shows the line holding the UTF-16 `offset` at the top, a little below the bars.
    func y(ofOffset offset: Int) -> CGFloat {
        max(0, layout.y(ofOffset: offset) + Self.margin - sample.insetTop - 40)
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

    /// The UTF-16 offset of the character under `point`; nil where there is no text.
    func offset(at point: CGPoint) -> Int? {
        guard let manager else { return nil }
        manager.ensureLayout(for: manager.documentRange)
        guard let fragment = manager.textLayoutFragment(for: point) else { return nil }
        let frame = fragment.layoutFragmentFrame
        let local = CGPoint(x: point.x - frame.minX, y: point.y - frame.minY)
        guard let line = fragment.textLineFragments.first(where: { $0.typographicBounds.contains(local) })
        else { return nil }
        let start = manager.offset(from: manager.documentRange.location, to: fragment.rangeInElement.location)
        return start + line.characterIndex(for: local)
    }

    /// Where the first line of the UTF-16 `range` is drawn.
    func rect(of range: NSRange) -> CGRect? {
        guard let manager,
            let start = manager.location(manager.documentRange.location, offsetBy: range.location),
            let end = manager.location(start, offsetBy: range.length),
            let text = NSTextRange(location: start, end: end)
        else { return nil }
        var found: CGRect?
        manager.enumerateTextSegments(in: text, type: .standard, options: []) { _, frame, _, _ in
            found = frame
            return false
        }
        return found
    }

    /// Marks the UTF-16 `ranges` (in order) as Article Search matches, the one at `current` stronger,
    /// in place of any marked before. Drawn over the text, not part of it.
    func mark(_ ranges: [NSRange], current: Int?) {
        guard let manager else { return }
        manager.removeRenderingAttribute(.backgroundColor, for: manager.documentRange)
        var at = manager.documentRange.location
        var offset = 0
        for (index, range) in ranges.enumerated() {
            guard let start = manager.location(at, offsetBy: range.location - offset),
                let end = manager.location(start, offsetBy: range.length),
                let text = NSTextRange(location: start, end: end)
            else { continue }
            let color = PlatformColor.systemYellow.withAlphaComponent(index == current ? 0.9 : 0.4)
            manager.addRenderingAttribute(.backgroundColor, value: color, for: text)
            (at, offset) = (end, range.upperBound)
        }
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
/// edited, as high as its text. Images are drawn from the kept files, so offline too. Highlights are
/// background colors over the styled text, a Comment's spot is dotted underneath.
@MainActor private struct ArticleText {
    let markdown: String
    let layout: ArticleLayout
    let images: ImageSource
    let markup: ArticleMarkup
    let found: FoundText
    let actions: MarkupActions

    /// What the text view's delegate and the drawn images need to outlive an update.
    @MainActor final class Coordinator: NSObject {
        let display = MarkerDisplay()
        let attachments: ImageAttachments
        var actions: MarkupActions
        /// The Markup that is drawn.
        var painted: ArticleMarkup?
        /// The Article Search matches that are marked.
        var marked = FoundText()
        init(_ images: ImageSource, _ actions: MarkupActions) {
            attachments = ImageAttachments(images)
            self.actions = actions
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(images, actions) }

    /// Styles the text, then draws the Markup over it, unless both are as they were.
    func update(_ storage: NSTextStorage, textChanged: Bool, _ coordinator: Coordinator) {
        coordinator.actions = actions
        if textChanged || coordinator.painted != markup {
            MarkdownStyler.style(storage, caret: nil, images: coordinator.attachments)
            paint(storage)
            coordinator.painted = markup
            coordinator.marked = FoundText(ranges: [], current: nil)  // styling may have dropped the marks
        }
        if coordinator.marked != found {
            layout.mark(found.ranges, current: found.current)
            coordinator.marked = found
        }
    }

    private func paint(_ storage: NSTextStorage) {
        storage.beginEditing()
        defer { storage.endEditing() }
        for highlight in markup.highlights {
            let range = NSRange(location: Int(highlight.start), length: Int(highlight.end - highlight.start))
            guard NSMaxRange(range) <= storage.length else { continue }
            storage.addAttribute(
                .backgroundColor, value: PlatformColor(highlight.color).withAlphaComponent(0.5), range: range)
        }
        for comment in markup.pageComments {
            guard let range = markup.range(of: .comment(comment.id), length: storage.length) else { continue }
            storage.addAttributes(
                [
                    .underlineStyle: NSUnderlineStyle.patternDot.union(.single).rawValue,
                    .underlineColor: PlatformColor.label,
                ],
                range: range)
        }
    }
}

extension PlatformColor {
    fileprivate convenience init(_ color: Rgb) {
        self.init(
            red: CGFloat(color.r) / 255, green: CGFloat(color.g) / 255, blue: CGFloat(color.b) / 255, alpha: 1)
    }
}

#if canImport(UIKit)
    extension ArticleText: UIViewRepresentable {
        func makeUIView(context: Context) -> UITextView {
            let view = UITextView(usingTextLayoutManager: true)
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate =
                context.coordinator.display
            view.delegate = context.coordinator
            view.isEditable = false
            view.isScrollEnabled = false
            view.backgroundColor = .clear
            view.textContainerInset = .zero
            view.textContainer.lineFragmentPadding = 0
            let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped))
            tap.cancelsTouchesInView = false
            tap.delegate = context.coordinator
            view.addGestureRecognizer(tap)
            layout.manager = view.textLayoutManager
            return view
        }

        func updateUIView(_ view: UITextView, context: Context) {
            let changed = view.text != markdown
            if changed { view.text = markdown }
            update(view.textStorage, textChanged: changed, context.coordinator)
        }

        func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextView, context: Context) -> CGSize? {
            let width = proposal.width ?? 320
            return CGSize(
                width: width, height: uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
            )
        }
    }

    extension ArticleText.Coordinator: UITextViewDelegate, UIGestureRecognizerDelegate {
        /// A selection offers Highlight and Comment before the system's own actions.
        func textView(
            _ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]
        ) -> UIMenu? {
            guard range.length > 0 else { return nil }
            let highlight = UIAction(title: String(localized: "Highlight"), image: UIImage(systemName: "highlighter")) {
                [weak self, weak textView] _ in
                textView?.selectedRange = NSRange(location: range.location, length: 0)
                self?.actions.highlight(range)
            }
            let comment = UIAction(title: String(localized: "Comment"), image: UIImage(systemName: "text.bubble")) {
                [weak self, weak textView] _ in
                textView?.selectedRange = NSRange(location: range.location, length: 0)
                self?.actions.comment(range.location)
            }
            return UIMenu(children: [highlight, comment] + suggestedActions)
        }

        @objc func tapped(_ tap: UITapGestureRecognizer) {
            actions.tap(tap.location(in: tap.view))
        }

        func gestureRecognizer(_ a: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith b: UIGestureRecognizer)
            -> Bool
        { true }
    }
#else
    extension ArticleText: NSViewRepresentable {
        func makeNSView(context: Context) -> NSTextView {
            let view = NSTextView(usingTextLayoutManager: true)
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate =
                context.coordinator.display
            view.delegate = context.coordinator
            view.isEditable = false
            view.drawsBackground = false
            view.isVerticallyResizable = false
            view.isHorizontallyResizable = false
            view.textContainerInset = .zero
            view.textContainer?.lineFragmentPadding = 0
            view.textContainer?.widthTracksTextView = true
            view.addGestureRecognizer(
                NSClickGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.clicked)))
            layout.manager = view.textLayoutManager
            return view
        }

        func updateNSView(_ view: NSTextView, context: Context) {
            let changed = view.string != markdown
            if changed { view.string = markdown }
            if let storage = view.textStorage { update(storage, textChanged: changed, context.coordinator) }
        }

        func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextView, context: Context) -> CGSize? {
            let width = proposal.width ?? 320
            nsView.textContainer?.size = CGSize(width: width, height: .greatestFiniteMagnitude)
            guard let layout = nsView.textLayoutManager else { return nil }
            layout.ensureLayout(for: layout.documentRange)
            return CGSize(width: width, height: layout.usageBoundsForTextContainer.height)
        }
    }

    extension ArticleText.Coordinator: NSTextViewDelegate {
        /// A selection offers Highlight and Comment in the context menu.
        func textView(_ view: NSTextView, menu: NSMenu, for event: NSEvent, at charIndex: Int) -> NSMenu? {
            guard view.selectedRange().length > 0 else { return menu }
            let highlight = NSMenuItem(
                title: String(localized: "Highlight"), action: #selector(highlightSelection), keyEquivalent: "")
            let comment = NSMenuItem(
                title: String(localized: "Comment"), action: #selector(commentOnSelection), keyEquivalent: "")
            for item in [highlight, comment] {
                item.target = self
                item.representedObject = view
            }
            menu.insertItem(comment, at: 0)
            menu.insertItem(highlight, at: 0)
            menu.insertItem(.separator(), at: 2)
            return menu
        }

        @objc private func highlightSelection(_ item: NSMenuItem) {
            guard let view = item.representedObject as? NSTextView else { return }
            let range = view.selectedRange()
            view.setSelectedRange(NSRange(location: range.location, length: 0))
            actions.highlight(range)
        }

        @objc private func commentOnSelection(_ item: NSMenuItem) {
            guard let view = item.representedObject as? NSTextView else { return }
            let range = view.selectedRange()
            view.setSelectedRange(NSRange(location: range.location, length: 0))
            actions.comment(range.location)
        }

        @objc func clicked(_ click: NSClickGestureRecognizer) {
            actions.tap(click.location(in: click.view))
        }
    }
#endif
