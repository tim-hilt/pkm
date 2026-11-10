import SwiftUI

/// What the Gallery's context menu and the Reader's cog popover tell about a Book: Title, Author (if
/// known), Format, File size and Date Imported, in that order. The File size
/// is read from disk when made.
struct BookInfo {
    struct Row: Identifiable {
        let label: LocalizedStringResource
        let value: String
        var id: String { String(localized: label) }
    }

    let rows: [Row]

    init(_ book: Book) {
        let fileBytes = (try? book.file.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        var rows = [Row(label: "Title", value: book.title)]
        if let author = book.author { rows.append(Row(label: "Author", value: author)) }
        rows += [
            Row(label: "Format", value: book.format == .pdf ? "PDF" : "EPUB"),
            Row(label: "File Size", value: Int64(fileBytes).formatted(.byteCount(style: .file))),
            Row(label: "Imported", value: book.importedDate.formatted(date: .abbreviated, time: .omitted)),
        ]
        self.rows = rows
    }
}

extension View {
    /// A context menu offering "Delete" and showing the Book's info. On iOS
    /// the info is the preview, as menu items there truncate and grey out;
    /// macOS shows no previews, so there it's lines in the menu.
    func bookContextMenu(_ book: Book, delete: @escaping () -> Void) -> some View {
        #if os(macOS)
            contextMenu {
                ForEach(BookInfo(book).rows) { row in
                    Text("\(String(localized: row.label)): \(row.value)")
                }
                Divider()
                Button("Delete", systemImage: "trash", role: .destructive, action: delete)
            }
        #else
            contextMenu {
                Button("Delete", systemImage: "trash", role: .destructive, action: delete)
            } preview: {
                BookInfoList(info: BookInfo(book))
                    .padding()
                    .frame(width: 320)
            }
        #endif
    }
}

/// `BookInfo` as label and value lines, for a popover.
struct BookInfoList: View {
    let info: BookInfo

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(info.rows) { row in
                LabeledContent(String(localized: row.label), value: row.value)
            }
        }
        .font(.callout)
    }
}
