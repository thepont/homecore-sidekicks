use std::time::Duration;

pub(crate) fn valid_base_url(url: &reqwest::Url) -> bool {
    (url.scheme() == "https" || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == "/"
}

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
