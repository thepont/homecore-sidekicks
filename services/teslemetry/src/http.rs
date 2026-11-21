use std::time::Duration;

pub fn client() -> reqwest::Result<reqwest::Client> {
    client_builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

pub fn blocking_client() -> reqwest::Result<reqwest::blocking::Client> {
    blocking_client_builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .tls_backend_preconfigured(homecore_bounded_http::tls::client_config(None))
}

fn blocking_client_builder() -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder()
        .tls_backend_preconfigured(homecore_bounded_http::tls::client_config(None))
}
