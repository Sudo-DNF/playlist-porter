//! Guards for CLAUDE.md compliance rules that are easy to break by accident.

use playlist_porter::core::auth::{google, spotify};
use playlist_porter::core::config::{Config, APP_NAME};
use playlist_porter::core::spotify::client as spotify_client;
use playlist_porter::core::youtube::client as youtube_client;
use playlist_porter::core::DISCLAIMER;

const ALLOWED_HOSTS: [&str; 5] = [
    "accounts.spotify.com",
    "api.spotify.com",
    "accounts.google.com",
    "oauth2.googleapis.com",
    "www.googleapis.com",
];

fn host(u: &str) -> String {
    url::Url::parse(u).unwrap().host_str().unwrap().to_string()
}

#[test]
fn every_endpoint_is_on_an_allowed_host() {
    for u in [
        spotify::AUTH_URL,
        spotify::TOKEN_URL,
        &format!("{}/me", spotify_client::API_BASE),
        google::AUTH_URL,
        google::TOKEN_URL,
        google::REVOKE_URL,
        &format!("{}/search", youtube_client::API_BASE),
    ] {
        assert!(ALLOWED_HOSTS.contains(&host(u).as_str()), "{u}");
    }
}

#[test]
fn scopes_are_exactly_as_specified() {
    assert_eq!(
        spotify::SCOPES,
        [
            "playlist-read-private",
            "playlist-read-collaborative",
            "user-library-read"
        ]
    );
    assert_eq!(google::SCOPE, "https://www.googleapis.com/auth/youtube");
}

#[test]
fn redirect_is_loopback_ip() {
    let uri = Config::default().redirect_uri();
    assert!(uri.starts_with("http://127.0.0.1:"));
    assert!(uri.ends_with("/callback"));
}

#[test]
fn naming_has_no_spotify() {
    assert_eq!(APP_NAME, "playlist-porter");
    assert!(!env!("CARGO_PKG_NAME").to_lowercase().contains("spotify"));
}

fn readme() -> String {
    std::fs::read_to_string(format!("{}/README.md", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn unwrap_quote(md: &str) -> Vec<String> {
    // Join each "> " block quote into one line.
    let mut out = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for l in md.lines() {
        if let Some(rest) = l.strip_prefix('>') {
            cur.push(rest.trim().to_string());
        } else if !cur.is_empty() {
            out.push(cur.join(" "));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out
}

#[test]
fn readme_has_verbatim_disclaimer_and_required_links() {
    let md = readme();
    assert!(unwrap_quote(&md).iter().any(|q| q == DISCLAIMER));
    for link in [
        "https://www.youtube.com/t/terms",
        "http://www.google.com/policies/privacy",
        "https://security.google.com/settings/security/permissions",
        "https://www.spotify.com/account/apps",
        "http://127.0.0.1:8888/callback",
    ] {
        assert!(md.contains(link), "README is missing {link}");
    }
    for section in [
        "## Setup",
        "## Usage",
        "## Limitations",
        "## Privacy",
        "## Disclaimer",
    ] {
        assert!(md.contains(section), "README is missing {section}");
    }
}

#[test]
fn readme_screenshots_exist() {
    let md = readme();
    let dir = env!("CARGO_MANIFEST_DIR");
    let mut n = 0;
    for part in md.split("](docs/screenshots/").skip(1) {
        let file = part.split(')').next().unwrap();
        assert!(
            std::path::Path::new(&format!("{dir}/docs/screenshots/{file}")).exists(),
            "{file}"
        );
        n += 1;
    }
    assert!(n >= 10);
}
