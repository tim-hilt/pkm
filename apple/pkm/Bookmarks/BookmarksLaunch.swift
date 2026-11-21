import Foundation
import ImageIO

extension BookmarksModel {
    /// The Bookmarks folder in the app's Documents, which Files and Finder show.
    static let documentsFolder = URL.documentsDirectory.appending(path: "Bookmarks", directoryHint: .isDirectory)

    /// The Bookmarks for this launch: the Bookmarks folder. For UI tests and screenshots (`-UITesting…`)
    /// it is temporary, with sample Bookmarks when launched with `-UITestingOpenBookmarks`, and the
    /// first one's Article open in the Reader with `-UITestingOpenArticle`.
    @MainActor
    static func forLaunch() -> BookmarksModel {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return BookmarksModel(store: openOrTemporary(documentsFolder))
        }
        let store = openOrTemporary(temporaryFolder())
        let opensArticle = arguments.contains("-UITestingOpenArticle")
        if opensArticle || arguments.contains("-UITestingOpenBookmarks") {
            let samples = [
                ("https://example.com/rust", "Rust in Action", "Example Blog"),
                ("https://example.org/design", "A Philosophy of Software Design", "example.org"),
                (
                    "https://example.net/long", "A page whose title is long enough to need two lines in a narrow row",
                    "Example News"
                ),
                ("https://example.dev/offline", "https://example.dev/offline", "example.dev"),
            ]
            for (address, title, site) in samples.reversed() {
                let details = Details(title: title, site: site, cover: nil, excerpt: "", author: "", published: "")
                let article = address.hasSuffix("/rust") ? readArticle(html: sampleArticle, address: address) : nil
                _ = try? store.save(
                    address: address, details: details, title: "", cover: nil, article: article?.markdown,
                    images: article.map { _ in [sampleImageAddress: sampleImage] } ?? [:])
            }
        }
        let model = BookmarksModel(store: store)
        if opensArticle { model.openID = model.bookmarks.first { $0.hasArticle }?.id }
        return model
    }

    private static let sampleImageAddress = "https://example.com/figure.png"

    /// A picture for the sample Article: a gradient.
    private static var sampleImage: Data {
        let context = CGContext(
            data: nil, width: 640, height: 320, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        let colors = [
            CGColor(red: 0.2, green: 0.5, blue: 0.9, alpha: 1), CGColor(red: 0.9, green: 0.4, blue: 0.3, alpha: 1),
        ]
        let gradient = CGGradient(colorsSpace: nil, colors: colors as CFArray, locations: nil)!
        context.drawLinearGradient(gradient, start: .zero, end: CGPoint(x: 640, y: 320), options: [])
        let png = NSMutableData()
        let destination = CGImageDestinationCreateWithData(png, "public.png" as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, context.makeImage()!, nil)
        CGImageDestinationFinalize(destination)
        return png as Data
    }

    /// A page to read: headings, Inline formatting, links, lists, a quote, code, an image and a table.
    private static let sampleArticle = """
        <body><nav><a href="/">Home</a> <a href="/blog">Blog</a></nav><article>
        <h1>Rust in Action</h1>
        <p>Learning <em>systems programming</em> is easier with a <strong>safe</strong> language, as the
        <a href="/book">book</a> shows: the compiler checks what you would otherwise debug for days, and
        <code>cargo</code> does the rest.</p>
        <h2>What you need</h2>
        <ul><li>A recent toolchain<ul><li>via rustup</li></ul></li><li>An editor</li><li>Some patience</li></ul>
        <img src="/figure.png" alt="A figure">
        <blockquote><p>Make it work, make it right, make it fast.</p></blockquote>
        <h4>A first program</h4>
        <pre><code>fn main() {
            println!("Hello");
        }</code></pre>
        <table><tr><th>Type</th><th>Size</th></tr><tr><td>u8</td><td>1 byte</td></tr></table>
        <p>Every chapter ends with exercises, and the answers are in the back, so that nobody has to guess whether the
        code that compiles is also the code that was meant, which is a rare comfort in this line of work.</p>
        </article><footer>Copyright</footer></body>
        """

    private static func temporaryFolder() -> URL {
        FileManager.default.temporaryDirectory.appending(
            path: "pkmUITests-\(UUID().uuidString)/Bookmarks", directoryHint: .isDirectory)
    }

    /// The store in `folder`, created if needed. Where that fails, a temporary one: the app still
    /// runs, its Bookmarks just don't stay.
    private static func openOrTemporary(_ folder: URL) -> BookmarkStore {
        (try? BookmarkStore.open(root: folder.path)) ?? (try! BookmarkStore.open(root: temporaryFolder().path))
    }
}
