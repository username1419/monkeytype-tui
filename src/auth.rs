//! Monkeytype.com authentication via Firebase Identity Toolkit.
//!
//! The site stores session state in browser local storage under a key like
//! `firebase:authUser:…`. The useful fields inside `value` are:
//!
//! - `stsTokenManager.accessToken` — short-lived bearer token (~1 hour)
//! - `stsTokenManager.refreshToken` — long-lived token used to obtain new access tokens
//!
//! This module reproduces the web client's login path: scrape the Firebase API key from
//! monkeytype.com, then call Google's sign-in endpoint with email and password.
//!
//! # Login flow
//!
//! 1. `GET https://monkeytype.com` — homepage HTML (cached locally; see [cache files](#cache-files))
//! 2. Parse the rolldown bundle path from `<script type="module" … src="/js/monkeytype.*.js">`
//! 3. `GET` that bundle and extract the hashed `js/firebase-config-live.*.js` path
//! 4. `GET` the firebase config script and parse `apiKey`
//! 5. `POST https://identitytoolkit.googleapis.com/v1/accounts:signInWithPassword?key=<apiKey>`
//!    with JSON body:
//!    ```json
//!    {
//!      "clientType": "CLIENT_TYPE_WEB",
//!      "email": "<email>",
//!      "password": "<password>",
//!      "returnSecureToken": true
//!    }
//!    ```
//!    Required headers: `Content-Type: application/json`, `Referer: https://monkeytype.com`,
//!    and a browser-like `User-Agent`.
//!
//! A successful response includes:
//!
//! | Field | Use |
//! |-------|-----|
//! | `idToken` | Authorization bearer token (~1 hour) |
//! | `refreshToken` | Persistent token for renewal |
//! | `expiresIn` | Token lifetime in seconds |
//!
//! # Token refresh (not yet implemented)
//!
//! When `idToken` expires, exchange the refresh token:
//!
//! 1. `POST https://securetoken.googleapis.com/v1/token?key=<apiKey>`
//!    with `Content-Type: application/x-www-form-urlencoded` and body
//!    `grant_type=refresh_token&refresh_token=<refresh_token>`.
//! 2. Response fields: `access_token`, `expires_in`, `token_type`, `refresh_token`,
//!    `id_token`, `user_id`, `project_id`.
//!
//! # Cache files
//!
//! Fetched assets are written beside the working directory to avoid repeated scraping
//! (and Cloudflare exposure) within a session:
//!
//! | File | Contents |
//! |------|----------|
//! | `./monkeytype.html` | Homepage HTML |
//! | `./monkeytype.js` | Rolldown application bundle |
//! | `./auth-constants.js` | `firebase-config-live.*.js` |
//! | `./apikey` | Parsed Firebase `apiKey` |

use std::{
    error::Error,
    fs::{read_to_string, write},
    path::Path,
    time::Duration,
};

use regex::Regex;
use reqwest::{Client, header::HeaderMap};
use serde_json::{Value, json};
use tokio::{spawn, time::Instant};

use crate::notify::{NOTIFICATIONS, QuickNotify};

static MONKEYTYPE_PAGE_CACHE: &str = "./monkeytype.html";
static MONKEYTYPE_ROLLDOWN_CACHE: &str = "./monkeytype.js";
static MONKEYTYPE_AUTH_CONSTANTS_CACHE: &str = "./auth-constants.js";
static MONKEYTYPE_APIKEY_CACHE: &str = "./apikey";
static MONKEYTYPE_REFRESH_TOKEN_PATH: &str = "./refresh_token";

/// Tokens and metadata returned by Firebase sign-in (or, when implemented, token refresh).
#[derive(Default, Debug)]
pub(crate) struct Authorization {
    api_key: String,
    display_name: String,
    access_token: String,
    last_access_timestamp: Option<Instant>,
    expires_in: Duration,
    token_type: String,
    refresh_token: String,
    user_id: String,
    project_id: String,
}

impl Authorization {
    #[cfg(debug_assertions)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        api_key: String,
        display_name: String,
        access_token: String,
        last_access_timestamp: Option<Instant>,
        expires_in: u32,
        token_type: String,
        refresh_token: String,
        user_id: String,
        project_id: String,
    ) -> Self {
        Self {
            api_key,
            display_name,
            access_token,
            expires_in: Duration::from_secs(expires_in as u64),
            token_type,
            refresh_token,
            user_id,
            project_id,
            last_access_timestamp,
        }
    }

    /// Parses the JSON body from `accounts:signInWithPassword`.
    pub(crate) fn from_login_response(
        response: String,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let obj: Value = serde_json::from_str(&response)?;

        Ok(Self {
            display_name: obj["displayName"].as_str().unwrap().into(),
            access_token: obj["idToken"].as_str().unwrap().into(),
            expires_in: Duration::from_secs(obj["expiresIn"].as_str().unwrap().parse()?),
            token_type: String::default(),
            refresh_token: obj["refreshToken"].as_str().unwrap().into(),
            user_id: String::default(),
            project_id: String::default(),
            last_access_timestamp: Some(Instant::now()),
            api_key: String::default(),
        })
    }

    pub(crate) async fn refresh(
        &mut self,
        callback: impl FnOnce(Result<Authorization, Box<dyn Error + Send + Sync>>)
        + 'static
        + Send
        + Sync,
    ) {
        let refresh_token = self.refresh_token.clone();
        let api_key = self.api_key.clone();
        let r = || async move {
            let client = Client::builder()
                .user_agent(
                    "Mozilla/5.0 (X11; Linux x86_64; rv:152.0) Gecko/20100101 Firefox/152.0",
                )
                .build()?;

            let mut headers = HeaderMap::new();
            headers.append("Content-Type", "application/x-www-form-urlencoded".parse()?);
            headers.append("Referer", "https://monkeytype.com".parse()?);

            let request = format!("grant_type=refresh_token&refresh_token={}", refresh_token);
            let authorization = client
                .post(format!(
                    "https://securetoken.googleapis.com/v1/token?key={}",
                    api_key
                ))
                .headers(headers)
                .body(request.clone())
                .send()
                .await?
                .text()
                .await?;
            let authorization = Authorization::from_refresh_response(request, authorization)?;

            Ok(authorization)
        };

        spawn(async move {
            callback(r().await);
        });
    }

    pub(crate) fn get_display_name(&self) -> &String {
        &self.display_name
    }

    pub(crate) fn get_access_token(&self) -> &String {
        &self.access_token
    }

    pub(crate) fn get_expires_in(&self) -> &Duration {
        &self.expires_in
    }

    pub(crate) fn get_token_type(&self) -> &String {
        &self.token_type
    }

    pub(crate) fn get_refresh_token(&self) -> &String {
        &self.refresh_token
    }

    pub(crate) fn get_user_id(&self) -> &String {
        &self.user_id
    }

    pub(crate) fn get_project_id(&self) -> &String {
        &self.project_id
    }

    pub(crate) fn get_expire_instant(&self) -> Instant {
        self.last_access_timestamp.unwrap_or(Instant::now()) + self.expires_in
    }

    pub(crate) fn is_access_expired(&self) -> bool {
        self.get_expire_instant() - Instant::now() == Duration::ZERO
    }

    pub(crate) fn update(&mut self, auth: Authorization) {
        if auth.display_name != String::default() {
            self.display_name = auth.display_name;
        }

        if auth.access_token != String::default() {
            self.access_token = auth.access_token;
        }

        if auth.expires_in != Duration::default() {
            self.expires_in = auth.expires_in;
        }

        if auth.token_type != String::default() {
            self.token_type = auth.token_type;
        }

        if auth.refresh_token != String::default() {
            self.refresh_token = auth.refresh_token;
        }

        if auth.user_id != String::default() {
            self.user_id = auth.user_id;
        }

        if auth.project_id != String::default() {
            self.project_id = auth.project_id;
        }
    }

    pub(crate) fn from_refresh_response(
        request: String,
        response: String,
    ) -> Result<Authorization, Box<dyn Error + Send + Sync>> {
        let _request: Value = serde_json::from_str(&request)?;
        let response: Value = serde_json::from_str(&response)?;

        Ok(Self {
            access_token: response["access_token"].as_str().unwrap().into(),
            expires_in: Duration::from_secs(
                response["expires_in"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap_or(0),
            ),
            token_type: response["token_type"].as_str().unwrap().into(),
            refresh_token: response["refresh_token"].as_str().unwrap().into(),
            user_id: response["user_id"].as_str().unwrap().into(),
            project_id: response["project_id"].as_str().unwrap().into(),
            api_key: String::default(),
            display_name: String::default(),
            last_access_timestamp: Some(Instant::now()),
        })
    }

    pub(crate) fn save_to_disk(&self) {
        let o = json!({
            "access_token": "",
            "expires_in": "0",
            "token_type": self.token_type,
            "refresh_token": self.refresh_token,
            "user_id": self.user_id,
            "project_id": self.project_id,
            "api_key": self.api_key,
            "display_name": self.display_name,
            "last_access_timestamp": "0",
        });

        if let Err(e) = write(MONKEYTYPE_REFRESH_TOKEN_PATH, o.to_string()) {
            spawn(async move {
                NOTIFICATIONS
                    .lock()
                    .expect("NOTIFICATIONS is poisoned")
                    .error(e);
            });
        }
    }
}

/// Logs into monkeytype.com with email and password.
///
/// Resolves the Firebase API key (using on-disk caches when present), then posts to
/// Google's Identity Toolkit. Returns [`AuthorizationResponse`] containing `id_token`
/// and `refresh_token` on success.
pub(crate) async fn login(
    email: String,
    password: String,
    // NOTE: honestly i dont know if using 'static here would be a good idea
    // seems like a potential mem leak but not too sure
) -> Result<Authorization, Box<dyn Error + Send + Sync>> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:152.0) Gecko/20100101 Firefox/152.0")
        .build()?;

    let api_key = get_api_key(&client).await?;

    let mut headers = HeaderMap::new();
    headers.append("Content-Type", "application/json".parse()?);
    headers.append("Referer", "https://monkeytype.com".parse()?);

    let authorization = client
        .post(format!(
            "https://identitytoolkit.googleapis.com/v1/accounts:signInWithPassword?key={}",
            api_key
        ))
        .headers(headers)
        .body(
            json!({
                "clientType": "CLIENT_TYPE_WEB",
                "email": email,
                "password": password,
                "returnSecureToken": true,
            })
            .to_string(),
        )
        .send()
        .await?
        .text()
        .await?;

    Authorization::from_login_response(authorization)
}

/// Scrapes monkeytype.com for the Firebase `apiKey`, caching each intermediate fetch.
pub(crate) async fn get_api_key(client: &Client) -> Result<String, Box<dyn Error + Send + Sync>> {
    let page;
    if let Ok(c) = read_to_string(MONKEYTYPE_PAGE_CACHE) {
        page = c;
    } else {
        let response = client.get("https://monkeytype.com").send().await?;
        let c = response.text().await?;

        write(MONKEYTYPE_PAGE_CACHE, &c)?;
        page = c;
    }

    let rolldown;
    if let Ok(c) = read_to_string(MONKEYTYPE_ROLLDOWN_CACHE) {
        rolldown = c;
    } else {
        let re = Regex::new(
            r#"<script type="module" crossorigin src="(/js/monkeytype\.[a-zA-Z\d]*\.js)">"#,
        )?;
        if let Some(capture) = re.captures(&page) {
            let path = &capture[1];
            rolldown = client
                .get(format!("https://monkeytype.com{}", path))
                .send()
                .await?
                .text()
                .await?;
            write(MONKEYTYPE_ROLLDOWN_CACHE, &rolldown)?;
        } else {
            return Err("page does not contain rolldown script".into());
        }
    }

    let auth_script;
    if let Ok(c) = read_to_string(MONKEYTYPE_AUTH_CONSTANTS_CACHE) {
        auth_script = c;
    } else {
        let re = Regex::new(r#"(js/firebase-config-live\.[\d\w]*\.js)"#)?;
        let path;
        if let Some(capture) = re.captures(&rolldown) {
            path = capture[1].to_string();
        } else {
            return Err("rolldown does not contain firebase auth constants script".into());
        }

        auth_script = client
            .get(format!("https://monkeytype.com/{}", path))
            .send()
            .await?
            .text()
            .await?;
        write(MONKEYTYPE_AUTH_CONSTANTS_CACHE, &auth_script)?;
    }

    let re = Regex::new(r#"apiKey:`([a-zA-Z\d_-]*)`"#)?;
    if let Some(capture) = re.captures(&auth_script) {
        let c = capture[1].to_string();
        write(MONKEYTYPE_APIKEY_CACHE, &c)?;
        return Ok(c);
    }

    Err("auth constants script does not contain apikey".into())
}

pub(crate) fn is_logged_in() -> bool {
    Path::new(MONKEYTYPE_REFRESH_TOKEN_PATH).exists()
}
