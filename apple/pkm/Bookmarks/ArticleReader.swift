import SwiftUI

/// The Reader: a Bookmark's Article, read-only, in the Markdown styling of ADR-0028 with every
/// marker hidden, in the app's Appearance. Links open in the browser, and so does Open Page.
struct ArticleReader: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(\.openURL) private var openURL
    let id: String

    private var bookmark: Bookmark? { model.bookmarks.first { $0.id == id } }

    var body: some View {
        ScrollView {
            if let bookmark, let markdown = model.article(of: bookmark) {
                ArticleText(markdown: markdown)
                    .frame(maxWidth: 680)
                    .padding()
                    .frame(maxWidth: .infinity)
            }
        }
        .navigationTitle(bookmark?.title ?? "")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            if let url = bookmark.flatMap({ URL(string: $0.address) }) {
                Button("Open Page", systemImage: "safari") { openURL(url) }
            }
        }
        .accessibilityIdentifier("article-reader")
    }
}

/// Markdown drawn as the Reader shows it: a TextKit 2 text view that can be selected from but not
/// edited, as high as its text.
private struct ArticleText {
    let markdown: String

    func style(_ storage: NSTextStorage) { MarkdownStyler.style(storage, caret: nil) }
}

#if canImport(UIKit)
    extension ArticleText: UIViewRepresentable {
        func makeCoordinator() -> MarkerDisplay { MarkerDisplay() }

        func makeUIView(context: Context) -> UITextView {
            let view = UITextView(usingTextLayoutManager: true)
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate = context.coordinator
            view.isEditable = false
            view.isScrollEnabled = false
            view.backgroundColor = .clear
            view.textContainerInset = .zero
            view.textContainer.lineFragmentPadding = 0
            return view
        }

        func updateUIView(_ view: UITextView, context: Context) {
            guard view.text != markdown else { return }
            view.text = markdown
            style(view.textStorage)
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
        func makeCoordinator() -> MarkerDisplay { MarkerDisplay() }

        func makeNSView(context: Context) -> NSTextView {
            let view = NSTextView(usingTextLayoutManager: true)
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate = context.coordinator
            view.isEditable = false
            view.drawsBackground = false
            view.isVerticallyResizable = false
            view.isHorizontallyResizable = false
            view.textContainerInset = .zero
            view.textContainer?.lineFragmentPadding = 0
            view.textContainer?.widthTracksTextView = true
            return view
        }

        func updateNSView(_ view: NSTextView, context: Context) {
            guard view.string != markdown else { return }
            view.string = markdown
            if let storage = view.textStorage { style(storage) }
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
