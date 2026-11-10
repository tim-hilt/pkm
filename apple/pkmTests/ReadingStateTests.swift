import Foundation
import Testing

@testable import pkm

// Reading Progress, Markup and Highlight Colors of the Library (the Reader's state).
extension LibraryModelTests {
    var place: Place { .pdf(pageIndex: 7, point: CGPoint(x: 12.5, y: 700.25)) }
    var lastRead: Date { Date(timeIntervalSince1970: 1_700_000_000.5) }

    @Test func aNeverOpenedBookHasNoReadingProgress() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)

        #expect(book.place == nil)
        #expect(book.lastRead == nil)
    }

    @Test func savingReadingProgressUpdatesTheBook() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)

        await library.saveProgress(place, of: book, at: lastRead)

        let updated = try #require(library.book(id: book.id))
        #expect(updated.place == place)
        #expect(updated.lastRead == lastRead)
    }

    @Test func readingProgressSurvivesReopeningTheLibrary() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        await library.saveProgress(place, of: book, at: lastRead)

        let reopened = try #require(try openLibrary().books.first)

        #expect(reopened.place == place)
        #expect(reopened.lastRead == lastRead)
    }

    @Test func theLatestSavedPlaceIsTheReadingProgress() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let earlier = Place.pdf(pageIndex: 3, point: .zero)

        await library.saveProgress(place, of: book, at: lastRead)
        await library.saveProgress(earlier, of: book, at: lastRead.addingTimeInterval(5))

        #expect(library.book(id: book.id)?.place == earlier)
        #expect(try openLibrary().books.first?.place == earlier)
    }

    @Test func aSaveTheGalleryDoesNotShowStillUpdatesTheListedBook() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        await library.saveProgress(place, of: book, at: lastRead)
        let later = lastRead.addingTimeInterval(5)

        // The same percent read, so the Gallery shows nothing new.
        await library.saveProgress(place, of: book, at: later)

        #expect(library.books.first?.lastRead == later)
        #expect(library.book(id: book.id)?.lastRead == later)
    }

    @Test func savingForADeletedBookDoesNothing() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        await library.delete(book)

        await library.saveProgress(place, of: book, at: lastRead)

        #expect(library.books.isEmpty)
        #expect(try openLibrary().books.isEmpty)
    }

    // MARK: Markup

    var anchor: Anchor {
        .pdf([
            PageRects(pageIndex: 2, rects: [CGRect(x: 72, y: 700.5, width: 200, height: 12)]),
            PageRects(
                pageIndex: 3,
                rects: [CGRect(x: 72, y: 720, width: 100, height: 12), CGRect(x: 72, y: 706, width: 40, height: 12)]),
        ])
    }
    var quote: String { "the passage" }

    @Test func aNeverHighlightedBookHasNoHighlights() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)

        #expect(await library.markup(of: book).highlights == [])
    }

    @Test func anAddedHighlightIsReturnedAndInTheBooksHighlights() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)

        let added = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))

        #expect(added.anchor == anchor && added.quotedText == quote)
        #expect(added.color == library.defaultHighlightColor.rgb)
        #expect(await library.markup(of: book).highlights == [added])
    }

    @Test func highlightsSurviveReopeningTheLibrary() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let added = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))

        let reopened = try openLibrary()

        #expect(await reopened.markup(of: book).highlights == [added])
    }

    @Test func anEPUBHighlightKeepsItsPositionsAndQuotedTextAcrossReopening() async throws {
        let library = try openLibrary()
        let source = try writeEPUB(named: "book.epub", chapters: ["<p>Call me Ishmael</p>", "<p>Some years</p>"])
        guard case .imported(let book) = await library.import(source) else { throw ImportFailed() }
        let epubAnchor = Anchor.epub(
            start: EpubPosition(spineIndex: 0, offset: 8), end: EpubPosition(spineIndex: 1, offset: 4))

        let added = try #require(await library.addHighlight(epubAnchor, quotedText: "Ishmael\nSome", to: book))

        #expect(added.anchor == epubAnchor && added.quotedText == "Ishmael\nSome")
        #expect(await library.markup(of: book).highlights == [added])
        #expect(await (try openLibrary()).markup(of: book).highlights == [added])
    }

    @Test func highlightsKeepTheOrderTheyWereAddedIn() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let first = try #require(
            await library.addHighlight(anchor, quotedText: quote, to: book, at: Date(timeIntervalSince1970: 1)))
        let second = try #require(
            await library.addHighlight(anchor, quotedText: quote, to: book, at: Date(timeIntervalSince1970: 2)))

        #expect(first.id != second.id)
        #expect(await library.markup(of: book).highlights == [first, second])
    }

    @Test func highlightingADeletedBookDoesNothing() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        await library.delete(book)

        #expect(await library.addHighlight(anchor, quotedText: quote, to: book) == nil)
        #expect(await library.markup(of: book).highlights == [])
    }

    @Test func deletingABookTakesItsHighlightsAlong() async throws {
        let library = try openLibrary()
        let source = try writePDF(named: "a.pdf")
        guard case .imported(let book) = await library.import(source) else { throw ImportFailed() }
        _ = await library.addHighlight(anchor, quotedText: quote, to: book)

        await library.delete(book)
        _ = await library.import(source)

        #expect(await library.markup(of: book).highlights == [])
    }

    @Test func aRecoloredHighlightKeepsItsColorAfterReopeningTheLibrary() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let added = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))
        let green = library.highlightColors[1].tint

        await library.setHighlightColor(green, of: added, in: book)

        var recolored = added
        recolored.color = Rgb(green)
        #expect(await library.markup(of: book).highlights == [recolored])
        #expect(await (try openLibrary()).markup(of: book).highlights == [recolored])
    }

    @Test func aHighlightsCommentSurvivesReopeningTheLibraryAndEmptyingItRemovesIt() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let added = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))

        await library.setComment("my thought", of: added, in: book)

        var commented = added
        commented.comment = "my thought"
        #expect(await (try openLibrary()).markup(of: book).highlights == [commented])

        await library.setComment("", of: added, in: book)
        #expect(await (try openLibrary()).markup(of: book).highlights == [added])
    }

    @Test func aNeverCommentdBookHasNoPageComments() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)

        #expect(await library.markup(of: book).pageComments == [])
    }

    @Test func anAddedPageCommentIsAtItsPlaceAndListedOnceItHasText() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let place = Place.pdf(pageIndex: 4, point: CGPoint(x: 100.5, y: 200.5))

        let added = try #require(await library.addPageComment(at: place, to: book))

        #expect(added.place == place)
        #expect(added.text == "")
        #expect(await library.markup(of: book).pageComments == [])

        await library.setText("my thought", of: added, in: book)
        var typed = added
        typed.text = "my thought"
        #expect(await library.markup(of: book).pageComments == [typed])
        #expect(await (try openLibrary()).markup(of: book).pageComments == [typed])
    }

    @Test func anEPUBPageCommentIsAtItsSpotInTheTextAndSurvivesReopening() async throws {
        let library = try openLibrary()
        let source = try writeEPUB(named: "book.epub", chapters: ["<p>One</p>", "<p>Two three</p>"])
        guard case .imported(let book) = await library.import(source) else { throw ImportFailed() }
        let place = Place.epub(spineIndex: 1, offset: 4)

        let added = try #require(await library.addPageComment(at: place, to: book))
        await library.setText("here", of: added, in: book)

        var typed = added
        typed.text = "here"
        #expect(added.place == place)
        #expect(await (try openLibrary()).markup(of: book).pageComments == [typed])
    }

    @Test func emptyingAPageCommentOrDeletingItLeavesNoPageComment() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let place = Place.pdf(pageIndex: 0, point: .zero)
        let emptied = try #require(await library.addPageComment(at: place, to: book))
        let deleted = try #require(await library.addPageComment(at: place, to: book))
        await library.setText("x", of: emptied, in: book)
        await library.setText("y", of: deleted, in: book)

        await library.setText("", of: emptied, in: book)
        await library.removePageComment(deleted, from: book)

        #expect(await (try openLibrary()).markup(of: book).pageComments == [])
    }

    @Test func addingAPageCommentToADeletedBookDoesNothing() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        await library.delete(book)

        #expect(await library.addPageComment(at: Place.pdf(pageIndex: 0, point: .zero), to: book) == nil)
    }

    @Test func aDeletedHighlightIsGoneAfterReopeningTheLibrary() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let first = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))
        let second = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))

        await library.removeHighlight(first, from: book)

        #expect(await library.markup(of: book).highlights == [second])
        #expect(await (try openLibrary()).markup(of: book).highlights == [second])
    }

    @Test func recoloringAndDeletingInADeletedBookDoNothing() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let added = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))
        await library.delete(book)

        await library.setHighlightColor(library.highlightColors[1].tint, of: added, in: book)
        await library.removeHighlight(added, from: book)

        #expect(await library.markup(of: book).highlights == [])
    }

    @Test func theHighlightColorsAreTheBuiltInOnesYellowFirst() throws {
        let library = try openLibrary()
        #expect(library.highlightColors.map(\.name) == ["Yellow", "Green", "Blue", "Pink", "Purple", "Red"])
    }

    @Test func theDefaultHighlightColorIsYellowUntilChosen() throws {
        let library = try openLibrary()
        #expect(library.defaultHighlightColor == library.highlightColors[0])
    }

    @Test func aChosenDefaultHighlightColorSurvivesReopeningTheLibrary() async throws {
        let library = try openLibrary()
        let purple = library.highlightColors[4]

        await library.setDefaultHighlightColor(purple)

        #expect(library.defaultHighlightColor == purple)
        #expect(try openLibrary().defaultHighlightColor == purple)
    }

    @Test func aCustomColorComesAfterTheBuiltInOnesInTheOrderAddedAndSurvivesReopening() async throws {
        let library = try openLibrary()
        let teal = try #require(await library.addCustomColor(RGB(0, 0.5, 0.5)))
        let brown = try #require(await library.addCustomColor(RGB(0.6, 0.3, 0)))

        #expect(library.highlightColors.suffix(2) == [teal, brown])
        #expect(teal.isCustom && !library.highlightColors[0].isCustom)
        #expect(try openLibrary().highlightColors.suffix(2) == [teal, brown])
    }

    @Test func aCustomColorCanBeTheDefaultHighlightColor() async throws {
        let library = try openLibrary()
        let teal = try #require(await library.addCustomColor(RGB(0, 0.5, 0.5)))

        await library.setDefaultHighlightColor(teal)

        #expect(try openLibrary().defaultHighlightColor == teal)
    }

    @Test func deletingTheDefaultCustomColorMakesYellowTheDefaultAgain() async throws {
        let library = try openLibrary()
        let teal = try #require(await library.addCustomColor(RGB(0, 0.5, 0.5)))
        await library.setDefaultHighlightColor(teal)

        await library.removeCustomColor(teal)

        #expect(library.highlightColors.count == 6)
        #expect(library.defaultHighlightColor == library.highlightColors[0])
        #expect(try openLibrary().highlightColors.count == 6)
        #expect(try openLibrary().defaultHighlightColor.name == "Yellow")
    }

    @Test func aBuiltInColorCannotBeDeleted() async throws {
        let library = try openLibrary()

        await library.removeCustomColor(library.highlightColors[1])

        #expect(library.highlightColors.count == 6)
    }

    @Test func aHighlightKeepsItsColorWhenItsCustomColorIsDeleted() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let teal = try #require(await library.addCustomColor(RGB(0, 0.5, 0.5)))
        await library.setDefaultHighlightColor(teal)
        _ = await library.addHighlight(anchor, quotedText: quote, to: book)

        await library.removeCustomColor(teal)

        #expect(await library.markup(of: book).highlights.map(\.color) == [teal.rgb])
    }

    @Test func aColorThatIsNoLongerInThePaletteShowsAsAnExtraCircleAtTheEnd() throws {
        let library = try openLibrary()
        let gone = RGB(0, 0.5, 0.5)

        let shown = library.highlightColors.including(gone)

        #expect(shown.count == 7 && shown.last?.rgb == Rgb(gone))
        #expect(library.highlightColors.including(library.highlightColors[2].tint) == library.highlightColors)
    }

    @Test func newHighlightsGetTheDefaultHighlightColor() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        let green = library.highlightColors[1]
        await library.setDefaultHighlightColor(green)

        let added = try #require(await library.addHighlight(anchor, quotedText: quote, to: book))

        #expect(added.color == green.rgb)
        #expect(await library.markup(of: book).highlights.map(\.color) == [green.rgb])
    }

    @Test func highlightsDoNotDisturbReadingProgress() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        await library.saveProgress(place, of: book, at: lastRead)

        _ = await library.addHighlight(anchor, quotedText: quote, to: book)

        #expect(try openLibrary().books.first?.place == place)
    }
}
