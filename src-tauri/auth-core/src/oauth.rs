use crate::model::{AuthError, Secret};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{timeout, Duration},
};
use url::Url;

pub const SCOPE: &str = "XboxLive.signin offline_access";
pub const TOKEN_ENDPOINT: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

fn random_secret() -> Result<Secret, AuthError> {
    let mut bytes = [0u8; 32];
    OsRng.try_fill_bytes(&mut bytes).map_err(|_| {
        AuthError::new(
            "randomness",
            "Windows could not generate secure sign-in randomness. Restart Ember and try again.",
        )
    })?;
    Ok(Secret::new(URL_SAFE_NO_PAD.encode(bytes)))
}
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub struct Authorization {
    pub verifier: Secret,
    pub state: Secret,
    pub redirect: String,
    pub url: Url,
    ipv4: TcpListener,
    ipv6: Option<TcpListener>,
}
impl Authorization {
    pub async fn bind(client_id: &str) -> Result<Self, AuthError> {
        let ipv4 = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await
            .map_err(|_| AuthError::new("callback", "Could not open the local Microsoft sign-in callback. Check firewall permissions and retry."))?;
        let port = ipv4
            .local_addr()
            .map_err(|_| {
                AuthError::new(
                    "callback",
                    "Could not determine the local sign-in callback port.",
                )
            })?
            .port();
        let ipv6 = TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, port))
            .await
            .ok();
        let redirect = format!("http://localhost:{port}/");
        let verifier = random_secret()?;
        let state = random_secret()?;
        let mut url =
            Url::parse("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize")
                .expect("constant URL");
        url.query_pairs_mut().extend_pairs([
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", &redirect),
            ("response_mode", "query"),
            ("scope", SCOPE),
            ("state", state.expose()),
            ("code_challenge", &challenge(verifier.expose())),
            ("code_challenge_method", "S256"),
            ("prompt", "select_account"),
        ]);
        Ok(Self {
            verifier,
            state,
            redirect,
            url,
            ipv4,
            ipv6,
        })
    }
    pub async fn receive(&self) -> Result<Secret, AuthError> {
        timeout(Duration::from_secs(180), async {
            loop {
                let accepted = if let Some(ipv6) = &self.ipv6 {
                    tokio::select! { value = self.ipv4.accept() => value, value = ipv6.accept() => value }
                } else { self.ipv4.accept().await };
                let (mut stream, peer) = accepted.map_err(|_| AuthError::new("callback", "The local sign-in callback closed unexpectedly. Retry sign-in."))?;
                if !peer.ip().is_loopback() { continue; }
                let request = timeout(Duration::from_secs(2), read_request(&mut stream)).await;
                let parsed = request.ok().and_then(Result::ok).and_then(|r| callback(&r, &self.redirect, self.state.expose()));
                let (status, body) = if parsed.is_some() {
                    ("200 OK", "Ember received the Microsoft response. Return to the launcher to see the sign-in result. You can close this tab.")
                } else { ("400 Bad Request", "Invalid sign-in callback. Return to Ember and retry if needed.") };
                // Never echo the request, code, state, tokens, or provider error into HTML or logs.
                let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = timeout(Duration::from_secs(2), stream.write_all(response.as_bytes())).await;
                if let Some(result) = parsed { return result; }
            }
        }).await.map_err(|_| AuthError::new("timeout", "Sign-in timed out. If you closed the browser, choose Sign in with Microsoft to try again."))?
    }
}
async fn read_request(stream: &mut TcpStream) -> Result<String, ()> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 1024];
    while bytes.len() < 16384 {
        let count = stream.read(&mut chunk).await.map_err(|_| ())?;
        if count == 0 {
            return Err(());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            return String::from_utf8(bytes).map_err(|_| ());
        }
    }
    Err(())
}
// None means an unrelated/forged callback: do not consume the pending attempt.
pub fn callback(request: &str, redirect: &str, state: &str) -> Option<Result<Secret, AuthError>> {
    let expected = Url::parse(redirect).ok()?;
    let mut lines = request.split("\r\n");
    let parts: Vec<_> = lines.next()?.split_whitespace().collect();
    if parts.len() != 3
        || parts[0] != "GET"
        || !matches!(parts[2], "HTTP/1.1" | "HTTP/1.0")
        || !parts[1].starts_with("/?")
        || parts[1].contains('#')
    {
        return None;
    }
    let hosts: Vec<_> = lines
        .take_while(|s| !s.is_empty())
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| k.eq_ignore_ascii_case("host"))
        .map(|(_, v)| v.trim())
        .collect();
    if hosts != [format!("localhost:{}", expected.port()?).as_str()] {
        return None;
    }
    let received = Url::parse(&format!("{redirect}{}", &parts[1][1..])).ok()?;
    let params: Vec<_> = received.query_pairs().collect();
    let single = |key: &str| {
        let mut values = params
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_ref());
        let value = values.next()?;
        if values.next().is_some() {
            None
        } else {
            Some(value)
        }
    };
    let actual_state = single("state")?;
    if !bool::from(actual_state.as_bytes().ct_eq(state.as_bytes())) {
        return None;
    }
    let code = single("code");
    let error = single("error");
    // Reject duplicate or ambiguous code/error fields, including empty ones.
    let auth_fields = params
        .iter()
        .filter(|(k, _)| k == "code" || k == "error")
        .count();
    if auth_fields != 1 {
        return None;
    }
    if let Some(error) = error {
        return Some(Err(if error == "access_denied" {
            AuthError::cancelled()
        } else {
            AuthError::new(
                "oauth",
                "Microsoft could not authorize sign-in. Check your application setup or try again.",
            )
        }));
    }
    let code = code?;
    if code.is_empty() || code.len() > 4096 {
        return None;
    }
    Some(Ok(Secret::new(code.into())))
}
