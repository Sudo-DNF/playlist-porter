//! One-shot OAuth callback listener on `127.0.0.1` only.
//!
//! Accepts exactly one `/callback` request, validates `state`, answers with a
//! "You can close this tab" page and shuts down. Times out after 5 minutes
//! and can be cancelled from the UI.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use crate::core::error::{CoreError, CoreResult};

pub const TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Outcome of parsing one HTTP request target.
#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    /// Not our path (e.g. `/favicon.ico`): answer 404 and keep waiting.
    NotCallback,
    Code(String),
    Failed(CoreError),
}

/// Parse a request target such as `/callback?code=..&state=..`.
pub fn parse_callback(target: &str, expected_state: &str) -> Callback {
    let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        return Callback::NotCallback;
    };
    if url.path() != "/callback" {
        return Callback::NotCallback;
    }
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Callback::Failed(CoreError::Auth(
            "the browser response didn't match this sign-in attempt (state mismatch); \
             try again"
                .into(),
        ));
    }
    if let Some(err) = error {
        let msg = if err == "access_denied" {
            "access was declined in the browser".to_string()
        } else {
            format!("the provider returned an error ({err})")
        };
        return Callback::Failed(CoreError::Auth(msg));
    }
    match code {
        Some(c) if !c.is_empty() => Callback::Code(c),
        _ => Callback::Failed(CoreError::Auth("no authorization code was returned".into())),
    }
}

/// Extract the request target from the first line of an HTTP request.
pub fn request_target(head: &str) -> Option<&str> {
    let line = head.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    (method == "GET").then_some(target)
}

/// Bind before opening the browser so a busy port fails early.
pub async fn bind(port: u16) -> CoreResult<TcpListener> {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|e| CoreError::LoopbackBind {
            port,
            reason: e.kind().to_string(),
        })
}

const DONE_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>playlist-porter</title></head><body style=\"font-family:sans-serif;margin:3em\">\
<h2>You can close this tab</h2><p>Return to playlist-porter to continue.</p></body></html>";

const FAIL_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>playlist-porter</title></head><body style=\"font-family:sans-serif;margin:3em\">\
<h2>Sign-in didn't complete</h2><p>You can close this tab and try again in playlist-porter.</p>\
</body></html>";

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buf = vec![0u8; 8192];
    let mut len = 0;
    loop {
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf[len..]))
            .await
            .ok()?
            .ok()?;
        if n == 0 {
            break;
        }
        len += n;
        if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") || len == buf.len() {
            break;
        }
    }
    String::from_utf8(buf[..len].to_vec()).ok()
}

/// Wait for the single callback. Returns the authorization code.
pub async fn wait_for_code(
    listener: TcpListener,
    expected_state: String,
    cancel: CancellationToken,
) -> CoreResult<String> {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        let accepted = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::AuthCancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(CoreError::AuthTimeout),
            a = listener.accept() => a,
        };
        let Ok((mut stream, peer)) = accepted else {
            continue;
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        let Some(head) = read_head(&mut stream).await else {
            continue;
        };
        let Some(target) = request_target(&head) else {
            respond(&mut stream, "405 Method Not Allowed", "").await;
            continue;
        };
        match parse_callback(target, &expected_state) {
            Callback::NotCallback => {
                respond(&mut stream, "404 Not Found", "").await;
            }
            Callback::Code(code) => {
                respond(&mut stream, "200 OK", DONE_PAGE).await;
                return Ok(code);
            }
            Callback::Failed(e) => {
                respond(&mut stream, "400 Bad Request", FAIL_PAGE).await;
                return Err(e);
            }
        }
        // Listener is dropped on return, which shuts it down.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_code_with_valid_state() {
        assert_eq!(
            parse_callback("/callback?code=abc&state=xyz", "xyz"),
            Callback::Code("abc".into())
        );
    }

    #[test]
    fn rejects_state_mismatch() {
        assert!(matches!(
            parse_callback("/callback?code=abc&state=evil", "xyz"),
            Callback::Failed(CoreError::Auth(_))
        ));
        assert!(matches!(
            parse_callback("/callback?code=abc", "xyz"),
            Callback::Failed(_)
        ));
    }

    #[test]
    fn handles_denied_and_other_paths() {
        assert!(matches!(
            parse_callback("/callback?error=access_denied&state=s", "s"),
            Callback::Failed(CoreError::Auth(m)) if m.contains("declined")
        ));
        assert_eq!(parse_callback("/favicon.ico", "s"), Callback::NotCallback);
    }

    #[test]
    fn request_line() {
        assert_eq!(
            request_target("GET /callback?x=1 HTTP/1.1\r\nHost: a\r\n\r\n"),
            Some("/callback?x=1")
        );
        assert_eq!(request_target("POST /callback HTTP/1.1\r\n"), None);
    }

    #[tokio::test]
    async fn end_to_end_on_loopback() {
        let listener = bind(0).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(wait_for_code(listener, "st".into(), cancel));
        // A stray request first, then the real callback.
        for target in ["/favicon.ico", "/callback?code=C0DE&state=st"] {
            let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            if target.starts_with("/callback") {
                assert!(out.contains("You can close this tab"));
            } else {
                assert!(out.starts_with("HTTP/1.1 404"));
            }
        }
        assert_eq!(task.await.unwrap().unwrap(), "C0DE");
    }

    #[tokio::test]
    async fn cancel_stops_listener() {
        let listener = bind(0).await.unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(
            wait_for_code(listener, "s".into(), cancel).await,
            Err(CoreError::AuthCancelled)
        );
    }
}
