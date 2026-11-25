import SwiftUI

/// A Gallery grid cell, the same for Drawings and Books: a page area of the
/// given aspect ratio, then the name (2 lines at most) and one secondary
/// detail line.
struct GalleryCell<Page: View>: View {
    let name: String
    /// The line under the name: a Drawing's modified date, a Book's Reading Progress.
    let detail: Text
    /// Width over height of the page area.
    let pageAspect: CGFloat
    @ViewBuilder let page: Page

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            page
                .aspectRatio(pageAspect, contentMode: .fit)
                .frame(maxWidth: .infinity)
            VStack(alignment: .leading, spacing: 2) {
                Text(name)
                    .font(.subheadline)
                    .lineLimit(2)
                detail
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .multilineTextAlignment(.leading)
        }
        .contentShape(.rect)
    }
}

/// A thin bar across the full width of the page area, filled to `percent`.
/// A Cover shows its original colors whatever the Appearance, so the bar is
/// light too.
private struct ReadingProgressBar: View {
    let percent: Int

    var body: some View {
        Rectangle().fill(.ultraThinMaterial)
            .overlay(alignment: .leading) {
                GeometryReader { bar in
                    Rectangle().fill(Color.accentColor).frame(width: bar.size.width * Double(percent) / 100)
                }
            }
            .frame(height: 3)
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
/// around it is transparent. The progress bar stays on the page's
/// bottom edge.
struct BookCell: View {
    @Environment(LibraryModel.self) private var library
    @Environment(ScrollActivity.self) private var scroll
    let book: Book
    @State private var cover: CGImage?

    /// A4 portrait.
    private static let pageAspect: CGFloat = 1 / 1.414

    var body: some View {
        let percent = book.percentRead.map(Int.init)
        GalleryCell(
            name: book.title, detail: Text(percent.map { "\($0) %" } ?? ""), pageAspect: Self.pageAspect
        ) {
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
            .overlay(alignment: .bottom) {
                if let percent { ReadingProgressBar(percent: percent) }
            }
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
