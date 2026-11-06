import AuthenticationServices

#if canImport(AppKit)
    import AppKit
#else
    import UIKit
#endif

/// The Google OAuth client of this build. Each platform has its own client in
/// the one Google Cloud project, published "In production". The redirect
/// scheme is derived from the ID by the core.
enum GoogleOAuth {
    #if os(macOS)
        static let clientID = "REPLACE-WITH-MACOS-CLIENT-ID.apps.googleusercontent.com"
    #else
        static let clientID = "REPLACE-WITH-IOS-CLIENT-ID.apps.googleusercontent.com"
    #endif
}

/// Shows a sign-in page in the system's web authentication session and hands
/// back the redirect it ends with. Throws `CancellationError` when the user
/// closes the page.
@MainActor
func showSignInPage(url: URL, callbackScheme: String) async throws -> URL {
    let presenter = SignInPresenter()
    return try await withCheckedThrowingContinuation { continuation in
        let session = ASWebAuthenticationSession(url: url, callback: .customScheme(callbackScheme)) {
            redirect, error in
            if let redirect {
                continuation.resume(returning: redirect)
            } else if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
                continuation.resume(throwing: CancellationError())
            } else {
                continuation.resume(throwing: error ?? CancellationError())
            }
            _ = presenter  // Lives as long as the page.
        }
        session.presentationContextProvider = presenter
        session.start()
    }
}

/// Where the sign-in page is shown: over the app's front window.
private final class SignInPresenter: NSObject, ASWebAuthenticationPresentationContextProviding {
    @MainActor func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        #if canImport(AppKit)
            NSApplication.shared.keyWindow ?? NSApplication.shared.windows.first ?? ASPresentationAnchor()
        #else
            let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            return scenes.flatMap(\.windows).first(where: \.isKeyWindow) ?? ASPresentationAnchor()
        #endif
    }
}
