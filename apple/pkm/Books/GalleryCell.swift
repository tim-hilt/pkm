import SwiftUI

/// A Gallery grid cell: an A4-portrait page area with a caption of up to 2
/// lines under it. The caption always reserves both lines, so cells in a row
/// line up whatever their text. A `percentRead` shows a progress indicator
/// along the page area's bottom edge, over whatever `page` shows.
struct GalleryCell<Page: View>: View {
    let caption: String
    var percentRead: Int?
    @ViewBuilder let page: Page

    /// A4 portrait.
    private static var pageAspect: CGFloat { 1 / 1.414 }

    var body: some View {
        VStack(spacing: 6) {
            page
                .aspectRatio(Self.pageAspect, contentMode: .fit)
                .frame(maxWidth: .infinity)
                .overlay(alignment: .bottom) {
                    if let percentRead { ReadingProgressIndicator(percent: percentRead) }
                }
            Text(caption)
                .font(.caption)
                .lineLimit(2, reservesSpace: true)
                .multilineTextAlignment(.center)
        }
        .contentShape(.rect)
    }
}

/// A thin bar across the full width, filled to `percent`, with the number in
/// a capsule above it in the bottom-right corner. A Cover shows its original
/// colors whatever the Appearance, so the indicator is light too; the
/// material keeps the number readable on it.
private struct ReadingProgressIndicator: View {
    let percent: Int
    private static let barHeight: CGFloat = 3

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            Text("\(percent) %")
                .font(.caption2.monospacedDigit())
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(.ultraThinMaterial, in: .capsule)
                .padding(.trailing, 4)
            Rectangle().fill(.ultraThinMaterial)
                .overlay(alignment: .leading) {
                    GeometryReader { bar in
                        Rectangle().fill(Color.accentColor).frame(width: bar.size.width * Double(percent) / 100)
                    }
                }
                .frame(height: Self.barHeight)
        }
        .environment(\.colorScheme, .light)
        .accessibilityHidden(true)
    }
}

extension View {
    /// Rings a cell with the accent colour while the keyboard highlight is on
    /// it. The ring sits in the gap between cells, so nothing shifts.
    func galleryHighlight(_ isOn: Bool) -> some View {
        overlay {
            if isOn {
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(Color.accentColor, lineWidth: 3)
                    .padding(-6)
                    .allowsHitTesting(false)
            }
        }
    }
}

extension ShapeStyle where Self == Color {
    /// The grey of a Book's placeholder.
    static var placeholderGrey: Color { Color.gray.opacity(0.3) }
}

/// A Book's cell. The placeholder stands in until the Cover exists, then the
/// Cover fades in. A saved Cover loads as soon as the cell appears; a new one
/// is rendered only while the Gallery is still (see `ScrollActivity`), and
/// that request is dropped if the cell scrolls away first. A Cover wider than
/// the cell's page shape is scaled to fit and centred vertically; the space
/// around it is transparent. The progress indicator stays on the cell's
/// bottom edge.
struct BookCell: View {
    @Environment(LibraryModel.self) private var library
    @Environment(ScrollActivity.self) private var scroll
    let book: Book
    @State private var cover: CGImage?

    var body: some View {
        GalleryCell(caption: book.title, percentRead: book.percentRead.map(Int.init)) {
            ZStack {
                if let cover {
                    Image(decorative: cover, scale: 1)
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .transition(.opacity)
                } else {
                    Color.placeholderGrey.transition(.opacity)
                }
            }
            .animation(.default, value: cover == nil)
        }
        .task(id: LoadKey(book: book.id, galleryIsStill: scroll.isStill)) {
            if cover == nil { cover = await library.savedCover(for: book) }
            guard cover == nil, scroll.isStill else { return }
            cover = await library.cover(for: book)
        }
    }
}

/// Restarts a cell's Cover loading when its Book changes or the Gallery
/// stops (a new render may start) or starts moving (a pending one is dropped).
private struct LoadKey: Equatable {
    let book: String
    let galleryIsStill: Bool
}
