import CoreGraphics
import CoreText
import Foundation

/// A few generated PDFs for `make screenshot SCREEN=books`, some of them read
/// to a different page, so that the Gallery shows Covers and Reading Progress.
enum SampleBooks {
    private static let books: [(title: String, author: String, pageIndex: UInt32?)] = [
        (title: "Seven Ways to Sketch", author: "A. Painter", pageIndex: 2),
        (title: "Notes on Rust", author: "F. Ferris", pageIndex: nil),
        (title: "The Quiet Library", author: "M. Reader", pageIndex: 6),
        (title: "Zettelkasten in Practice", author: "N. Luhmann", pageIndex: nil),
    ]
    private static let pageCount = 8

    /// Imports the sample Books into `core`, each of them read to its page.
    static func `import`(into core: Library, sort: Sort) {
        let folder = FileManager.default.temporaryDirectory.appending(
            path: "SampleBooks-\(UUID().uuidString)", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        for book in books {
            let url = folder.appending(path: "\(book.title).pdf")
            guard write(book.title, author: book.author, to: url),
                case .imported(let imported)? = try? core.import(
                    source: url.path, reader: PDFKitReader(), progress: Silent())
            else { continue }
            if let pageIndex = book.pageIndex {
                let progress = ReadingProgress(
                    place: .pdf(pageIndex: pageIndex, x: 0, y: 0),
                    lastRead: UInt64(Date.now.timeIntervalSince1970 * 1000))
                _ = try? core.saveProgress(id: imported.id, progress: progress, sort: sort)
            }
        }
    }

    private final class Silent: ImportProgress {
        func report(copied: UInt64, total: UInt64) {}
    }

    /// A PDF whose pages show `title` and `author`, with both in its metadata.
    private static func write(_ title: String, author: String, to url: URL) -> Bool {
        var box = CGRect(x: 0, y: 0, width: 612, height: 792)
        let info = [kCGPDFContextTitle: title, kCGPDFContextAuthor: author] as CFDictionary
        guard let context = CGContext(url as CFURL, mediaBox: &box, info) else { return false }
        for _ in 0..<pageCount {
            context.beginPDFPage(nil)
            context.setFillColor(CGColor(red: 0.93, green: 0.95, blue: 0.99, alpha: 1))
            context.fill(box.insetBy(dx: 60, dy: 60))
            draw(title, size: 40, at: CGPoint(x: 90, y: 560), in: context)
            draw(author, size: 24, at: CGPoint(x: 90, y: 500), in: context)
            context.endPDFPage()
        }
        context.closePDF()
        return true
    }

    private static func draw(_ text: String, size: CGFloat, at point: CGPoint, in context: CGContext) {
        let font = CTFontCreateWithName("Helvetica" as CFString, size, nil)
        let attributed = NSAttributedString(
            string: text,
            attributes: [
                NSAttributedString.Key(kCTFontAttributeName as String): font,
                NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0.1, alpha: 1),
            ])
        context.textPosition = point
        CTLineDraw(CTLineCreateWithAttributedString(attributed), context)
    }
}
