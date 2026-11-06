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

/// An agent for the OAuth and REST APIs of the cloud providers: any status is
/// an answer, not an error. `https_only` is off only for the fake servers of
/// the tests.
pub(crate) fn api_agent(https_only: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .https_only(https_only)
        .http_status_as_error(false)
        .tls_config(tls_config())
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(30)))
        .build();
    ureq::Agent::new_with_config(config)
}
