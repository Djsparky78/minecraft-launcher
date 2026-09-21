use crate::{
    model::{service_error, AuthError, Profile, Secret},
    oauth::{SCOPE, TOKEN_ENDPOINT},
};
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub struct Api {
    client: Client,
}
pub struct MicrosoftTokens {
    pub access: Secret,
    pub refresh: Option<Secret>,
}
pub struct MinecraftSession {
    pub profile: Profile,
    pub _access: Secret,
    pub expires_at: u64,
}
impl Api {
    pub fn new() -> Result<Self, AuthError> {
        let client = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert(
                    reqwest::header::ACCEPT,
                    reqwest::header::HeaderValue::from_static("application/json"),
                );
                h
            })
            .user_agent("EmberLauncher/0.1")
            .build()
            .map_err(|_| {
                AuthError::new(
                    "network",
                    "Could not initialize secure authentication networking.",
                )
            })?;
        Ok(Self { client })
    }
    async fn json(&self, stage: &str, request: RequestBuilder) -> Result<Value, AuthError> {
        let mut response = request
            .send()
            .await
            .map_err(|_| AuthError::network(stage))?;
        let status = response.status();
        let mut body = zeroize::Zeroizing::new(Vec::new());
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AuthError::network(stage))?
        {
            if body.len() + chunk.len() > 1_048_576 {
                return Err(AuthError::new(
                    "response",
                    format!("{stage} returned an oversized response."),
                ));
            }
            body.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(service_error(
                stage,
                status.as_u16(),
                value["error"].as_str(),
                value["XErr"].as_u64(),
            ));
        }
        if value.is_null() {
            return Err(AuthError::new(
                "response",
                format!("{stage} returned an invalid response. Try again later."),
            ));
        }
        Ok(value)
    }
    pub async fn code(
        &self,
        client_id: &str,
        code: &Secret,
        verifier: &Secret,
        redirect: &str,
    ) -> Result<MicrosoftTokens, AuthError> {
        self.microsoft(self.client.post(TOKEN_ENDPOINT).form(&[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code.expose()),
            ("redirect_uri", redirect),
            ("code_verifier", verifier.expose()),
            ("scope", SCOPE),
        ]))
        .await
    }
    pub async fn refresh(
        &self,
        client_id: &str,
        token: &Secret,
    ) -> Result<MicrosoftTokens, AuthError> {
        self.microsoft(self.client.post(TOKEN_ENDPOINT).form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", token.expose()),
            ("scope", SCOPE),
        ]))
        .await
    }
    async fn microsoft(&self, request: RequestBuilder) -> Result<MicrosoftTokens, AuthError> {
        let mut value = self.json("Microsoft", request).await?;
        if !value["token_type"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("bearer"))
        {
            return Err(AuthError::new(
                "response",
                "Microsoft returned an unsupported token type.",
            ));
        }
        let access = take_secret(&mut value, "access_token")?;
        let refresh = if value.get("refresh_token").is_some() {
            Some(take_secret(&mut value, "refresh_token")?)
        } else {
            None
        };
        Ok(MicrosoftTokens { access, refresh })
    }
    pub async fn minecraft(&self, access: &Secret) -> Result<MinecraftSession, AuthError> {
        let mut xbox = self.json("Xbox Live", self.client.post("https://user.auth.xboxlive.com/user/authenticate").header("x-xbl-contract-version", "1").json(&json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={}", access.expose()) },
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"
        }))).await?;
        let xbox_token = take_secret(&mut xbox, "Token")?;
        let xbox_hash = user_hash(&xbox)?;
        let mut xsts = self.json("Xbox XSTS", self.client.post("https://xsts.auth.xboxlive.com/xsts/authorize").header("x-xbl-contract-version", "1").json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbox_token.expose()] },
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"
        }))).await?;
        let xsts_token = take_secret(&mut xsts, "Token")?;
        let hash = user_hash(&xsts)?;
        if hash != xbox_hash {
            return Err(AuthError::new(
                "identity",
                "Xbox returned inconsistent account identifiers. Sign in again.",
            ));
        }
        let mut minecraft = self
            .json(
                "Minecraft login",
                self.client
                    .post("https://api.minecraftservices.com/authentication/login_with_xbox")
                    .json(&json!({
                        "identityToken": format!("XBL3.0 x={hash};{}", xsts_token.expose())
                    })),
            )
            .await?;
        let token = take_secret(&mut minecraft, "access_token")?;
        let lifetime = minecraft["expires_in"]
            .as_u64()
            .filter(|s| *s > 120 && *s <= 86400)
            .ok_or_else(|| {
                AuthError::new("response", "Minecraft returned an invalid session expiry.")
            })?;
        let entitlements = self
            .json(
                "Minecraft ownership",
                self.client
                    .get("https://api.minecraftservices.com/entitlements/license")
                    .query(&[("requestId", uuid::Uuid::new_v4().to_string())])
                    .bearer_auth(token.expose()),
            )
            .await?;
        verify_entitlements(&entitlements)?;
        let profile = self
            .json(
                "Minecraft profile",
                self.client
                    .get("https://api.minecraftservices.com/minecraft/profile")
                    .bearer_auth(token.expose()),
            )
            .await?;
        let profile: Profile = serde_json::from_value(profile).map_err(|_| AuthError::new("profile_missing", "Minecraft did not return a Java Edition profile. Set up your profile at minecraft.net and retry."))?;
        Ok(MinecraftSession {
            profile: profile.validate()?,
            _access: token,
            expires_at: crate::model::now().saturating_add(lifetime),
        })
    }
}
fn take_secret(value: &mut Value, key: &str) -> Result<Secret, AuthError> {
    match value.get_mut(key).map(Value::take) {
        Some(Value::String(s)) if !s.is_empty() => Ok(Secret::new(s)),
        _ => Err(AuthError::new(
            "response",
            "The authentication service returned an incomplete credential response. Try again.",
        )),
    }
}
fn user_hash(value: &Value) -> Result<String, AuthError> {
    value["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned)
        .ok_or_else(|| {
            AuthError::new(
                "identity",
                "Xbox did not return a valid account identifier.",
            )
        })
}
pub fn verify_entitlements(value: &Value) -> Result<(), AuthError> {
    #[derive(Deserialize)]
    struct Item {
        name: String,
    }
    #[derive(Deserialize)]
    struct Entitlements {
        items: Vec<Item>,
    }
    let parsed: Entitlements = serde_json::from_value(value.clone()).map_err(|_| {
        AuthError::new(
            "response",
            "Minecraft returned an invalid ownership response.",
        )
    })?;
    // Entitlements are obtained directly over authenticated TLS, never supplied by React.
    if parsed
        .items
        .iter()
        .any(|item| matches!(item.name.as_str(), "game_minecraft" | "product_minecraft"))
    {
        Ok(())
    } else {
        Err(AuthError::new("no_ownership", "This account does not currently have access to Minecraft Java Edition. Check your purchase or subscription, or sign in with another account."))
    }
}
