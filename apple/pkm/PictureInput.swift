import Foundation
import UniformTypeIdentifiers

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// Pictures that reach a view from outside, pasted or dropped: the bytes of image files in any
/// format ImageIO reads. What is done with them is up to the view (the Canvas's `insertImages`,
/// the Description's Images).
enum PictureInput {
    #if os(macOS)
        /// The pictures on a pasteboard: the files copied or dragged from the Finder if they are
        /// images, else the picture itself (what a screenshot is).
        static func pictures(in pasteboard: NSPasteboard) -> [Data] {
            let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: imageFiles) as? [URL] ?? []
            let files = urls.compactMap { try? Data(contentsOf: $0) }
            if !files.isEmpty { return files }
            return pictureTypes.lazy.compactMap { pasteboard.data(forType: $0) }.first.map { [$0] } ?? []
        }

        /// Whether `pictures(in:)` finds anything, without reading it: asked on every drag movement
        /// and menu validation.
        static func hasPictures(in pasteboard: NSPasteboard) -> Bool {
            pasteboard.canReadObject(forClasses: [NSURL.self], options: imageFiles)
                || pasteboard.availableType(from: pictureTypes) != nil
        }

        /// The types a view registers for to take drops of pictures.
        static let droppedTypes: [NSPasteboard.PasteboardType] = [.fileURL, .png, .tiff]

        /// Files that are images, of a pasteboard.
        private static var imageFiles: [NSPasteboard.ReadingOptionKey: Any] {
            [.urlReadingFileURLsOnly: true, .urlReadingContentsConformToTypes: [UTType.image.identifier]]
        }

        /// The picture itself on a pasteboard, best first.
        private static let pictureTypes: [NSPasteboard.PasteboardType] = [.png, .tiff]
    #else
        /// The pictures `providers` hold (those of a paste or a drop).
        @MainActor static func pictures(of providers: [NSItemProvider]) async -> [Data] {
            var sources: [Data] = []
            for provider in providers where provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
                let data = await withCheckedContinuation { continuation in
                    provider.loadDataRepresentation(forTypeIdentifier: UTType.image.identifier) { data, _ in
                        continuation.resume(returning: data)
                    }
                }
                if let data { sources.append(data) }
            }
            return sources
        }

        /// The picture on the pasteboard, as its first image type holds it.
        static func picture(in pasteboard: UIPasteboard) -> Data? {
            let type = pasteboard.types.first { UTType($0)?.conforms(to: .image) == true }
            return type.flatMap { pasteboard.data(forPasteboardType: $0) }
        }
    #endif
}
