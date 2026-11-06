//! HTTP on the platform's TLS, shared by the Cloud Providers.

/// The platform's TLS: the Security framework and the Device's trust store on
/// Apple platforms, rustls with the bundled public roots elsewhere.
pub(crate) fn tls_config() -> ureq::tls::TlsConfig {
    #[cfg(target_vendor = "apple")]
    return ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    #[cfg(not(target_vendor = "apple"))]
    ureq::tls::TlsConfig::default()
}
