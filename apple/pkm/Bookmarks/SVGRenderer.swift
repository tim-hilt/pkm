import WebKit

/// Draws SVGs, which ImageIO doesn't read, with WebKit as the page showed them, to keep them as PNG
/// (ADR-0029). WebKit's process starts with the first SVG, which takes a while, and serves all
/// the others of this renderer; it ends with the renderer.
@MainActor final class SVGRenderer {
    /// Longest side drawn, so that a huge SVG doesn't fill the memory.
    private static let maxPixelSize = 4096

    /// Draws the image from its bytes as an `<img>`, where its scripts don't run and it loads nothing,
    /// in the light appearance. One sized by its viewBox alone is drawn at that size, not the
    /// 300 pixels wide a browser falls back to. A data URL, unlike a blob one, leaves the canvas
    /// readable when the SVG holds HTML in a foreignObject.
    private static let draw = """
        const text = new TextDecoder().decode(Uint8Array.from(atob(svg), c => c.charCodeAt(0)));
        const root = new DOMParser().parseFromString(text, "image/svg+xml").documentElement;
        if (root.localName !== "svg") return null;
        const img = new Image();
        img.src = "data:image/svg+xml;base64," + svg;
        try { await img.decode(); } catch { return null; }
        let [width, height] = [img.naturalWidth, img.naturalHeight];
        const box = root.viewBox.baseVal;
        if (!/^\\s*[\\d.]+(px)?\\s*$/.test(root.getAttribute("width") ?? "") && box?.width > 0 && box?.height > 0) {
            [width, height] = [box.width, box.height];
        }
        const scale = Math.min(2, max / Math.max(width, height));
        const canvas = document.createElement("canvas");
        canvas.width = Math.round(width * scale);
        canvas.height = Math.round(height * scale);
        if (canvas.width < 1 || canvas.height < 1) return null;
        canvas.getContext("2d").drawImage(img, 0, 0, canvas.width, canvas.height);
        return canvas.toDataURL("image/png");
        """

    /// The SVG in `data` as PNG, drawn at twice its size (for sharp Retina displays); nil if it is no SVG.
    func png(of data: Data) async -> Data? {
        // What is no SVG doesn't start WebKit.
        guard String(decoding: data.prefix(4096), as: UTF8.self).contains("<svg") else { return nil }
        let result = try? await web.callAsyncJavaScript(
            Self.draw, arguments: ["svg": data.base64EncodedString(), "max": Self.maxPixelSize],
            contentWorld: .defaultClient)
        guard let url = result as? String, let comma = url.firstIndex(of: ",") else { return nil }
        return Data(base64Encoded: String(url[url.index(after: comma)...]))
    }

    private lazy var web: WKWebView = {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        let web = WKWebView(frame: .zero, configuration: configuration)
        #if canImport(UIKit)
            web.overrideUserInterfaceStyle = .light
        #else
            web.appearance = NSAppearance(named: .aqua)
        #endif
        return web
    }()
}
