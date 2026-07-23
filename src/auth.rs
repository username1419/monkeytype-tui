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
//! # Token refresh
//!
//! When `idToken` expires, the update loop in [`main::update`] automatically
//! exchanges the refresh token for a new access token:
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
    fs::{self, read_to_string, write},
    sync::Arc,
    time::Duration,
};

use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Client, header::HeaderMap};
use serde::Deserialize;
use serde_json::json;
use tokio::{
    join,
    runtime::{Handle, Runtime},
    spawn,
    sync::{Mutex, oneshot},
    time::Instant,
};

use crate::{
    State,
    notify::{QuickNotify, enotify, error, notify},
};

/// Browser user-agent string sent with all HTTP requests to monkeytype.com.
pub(crate) const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:152.0) Gecko/20100101 Firefox/152.0";

/// Global authorization state, lazily initialized and shared across threads.
pub(crate) static AUTHORIZATION: Lazy<Arc<std::sync::Mutex<Authorization>>> =
    Lazy::new(|| Arc::new(std::sync::Mutex::new(Authorization::default())));

/// Shared HTTP client with a browser-like user-agent.
pub(crate) static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .inspect_err(|e| panic!("Client build failed: {e}"))
        .unwrap()
});

/// Path to the cached monkeytype.com homepage HTML.
static MONKEYTYPE_PAGE_CACHE: &str = "./monkeytype.html";
/// Path to the cached rolldown application bundle.
static MONKEYTYPE_ROLLDOWN_CACHE: &str = "./monkeytype.js";
/// Path to the cached Firebase config script.
static MONKEYTYPE_AUTH_CONSTANTS_CACHE: &str = "./auth-constants.js";
/// Path to the cached Firebase API key.
static MONKEYTYPE_APIKEY_CACHE: &str = "./apikey";
/// Path to the persisted refresh token and session metadata.
static MONKEYTYPE_REFRESH_TOKEN_PATH: &str = "./refresh_token";

/// Tokens and metadata returned by Firebase sign-in or token refresh.
#[derive(Default, Debug, Deserialize)]
pub(crate) struct Authorization {
    api_key: String,
    display_name: String,
    access_token: String,
    #[serde(skip)]
    last_access_timestamp: Option<Instant>,
    #[serde(skip)]
    expires_in: Duration,
    token_type: String,
    refresh_token: String,
    user_id: String,
    project_id: String,
}

/// Response from `accounts:signInWithPassword`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    display_name: String,
    id_token: String,
    expires_in: String,
    refresh_token: String,
}

/// Response from the Google Secure Token refresh endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RefreshResponse {
    access_token: String,
    expires_in: String,
    token_type: String,
    refresh_token: String,
    user_id: String,
    project_id: String,
}

impl Authorization {
    /// Creates a new `Authorization` with all fields specified.
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
        let r: LoginResponse = serde_json::from_str(&response)?;

        Ok(Self {
            display_name: r.display_name,
            access_token: r.id_token,
            expires_in: Duration::from_secs(r.expires_in.parse()?),
            token_type: String::default(),
            refresh_token: r.refresh_token,
            user_id: String::default(),
            project_id: String::default(),
            last_access_timestamp: Some(Instant::now()),
            api_key: String::default(),
        })
    }

    /// Refreshes the access token using the stored refresh token, releasing
    /// the global [`AUTHORIZATION`] lock before the network request.
    pub(crate) async fn refresh_non_blocking() -> Result<(), Box<dyn Error + Send + Sync>> {
        let refresh_token;
        let api_key;
        {
            let Ok(a) = AUTHORIZATION.lock() else {
                enotify!("Authentication state is poisoned");
                return Err("Authenication state is poisoned".into());
            };
            refresh_token = a.refresh_token.clone();
            api_key = a.api_key.clone();
        }

        let authorization = get_refreshed_authorization(refresh_token, api_key).await?;

        AUTHORIZATION.lock().unwrap().update(authorization);
        Ok(())
    }

    /// Returns the user's display name.
    pub(crate) fn get_display_name(&self) -> &String {
        &self.display_name
    }

    /// Returns the short-lived access (bearer) token.
    pub(crate) fn get_access_token(&self) -> &String {
        &self.access_token
    }

    /// Returns the token lifetime duration.
    pub(crate) fn get_expires_in(&self) -> &Duration {
        &self.expires_in
    }

    /// Returns the token type (e.g. `"Bearer"`).
    pub(crate) fn get_token_type(&self) -> &String {
        &self.token_type
    }

    /// Returns the long-lived refresh token used to obtain new access tokens.
    pub(crate) fn get_refresh_token(&self) -> &String {
        &self.refresh_token
    }

    /// Returns the Firebase user ID.
    pub(crate) fn get_user_id(&self) -> &String {
        &self.user_id
    }

    /// Returns the Firebase project ID.
    pub(crate) fn get_project_id(&self) -> &String {
        &self.project_id
    }

    /// Returns the [`Instant`] at which the current access token expires.
    pub(crate) fn get_expire_instant(&self) -> Instant {
        self.last_access_timestamp.unwrap_or(Instant::now()) + self.expires_in
    }

    /// Returns `true` if the access token has expired.
    pub(crate) fn is_access_expired(&self) -> bool {
        self.get_expire_instant() < Instant::now()
    }

    /// Returns `true` if a refresh token is present (i.e. the user has logged in).
    pub(crate) fn is_logged_in(&self) -> bool {
        !self.refresh_token.is_empty()
    }

    /// Merges non-default fields from `auth` into `self`, preserving existing
    /// values where the incoming field is empty or zero.
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

    /// Parses the JSON response from the Google token refresh endpoint.
    pub(crate) fn from_refresh_response(
        response: String,
    ) -> Result<Authorization, Box<dyn Error + Send + Sync>> {
        let r: RefreshResponse = serde_json::from_str(&response)?;

        Ok(Self {
            access_token: r.access_token,
            expires_in: Duration::from_secs(r.expires_in.parse().unwrap_or(0)),
            token_type: r.token_type,
            refresh_token: r.refresh_token,
            user_id: r.user_id,
            project_id: r.project_id,
            api_key: String::default(),
            display_name: String::default(),
            last_access_timestamp: Some(Instant::now()),
        })
    }

    /// Persists the refresh token and session metadata to disk so the session
    /// can be restored on the next launch.
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
                error!(e);
            });
        }
    }
}

/// Exchanges a refresh token for a new access token via the Google Secure Token API.
async fn get_refreshed_authorization(
    refresh_token: String,
    api_key: String,
) -> Result<Authorization, Box<dyn Error + Send + Sync + 'static>> {
    let mut api_key = api_key;
    if api_key.is_empty() {
        api_key = get_api_key(&CLIENT).await?;
    }
    if refresh_token.is_empty() {
        return Err("Refresh token is empty".into());
    }
    let mut headers = HeaderMap::new();
    headers.append(
        "Content-Type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    headers.append("Referer", "https://monkeytype.com".parse().unwrap());
    let request_body = format!("grant_type=refresh_token&refresh_token={}", refresh_token);
    let response = CLIENT
        .post(format!(
            "https://securetoken.googleapis.com/v1/token?key={}",
            api_key
        ))
        .headers(headers)
        .body(request_body.clone())
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(response
            .status()
            .canonical_reason()
            .unwrap_or("Unspecified error")
            .into());
    }
    let response_text = response.text().await?;
    let mut authorization = Authorization::from_refresh_response(response_text)?;
    authorization.api_key = api_key;
    Ok(authorization)
}

/// Logs into monkeytype.com with email and password.
///
/// Resolves the Firebase API key (using on-disk caches when present), then posts to
/// Google's Identity Toolkit. Returns an [`Authorization`] containing `id_token`
/// and `refresh_token` on success.
pub(crate) async fn login(
    email: String,
    password: String,
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

/// Loads the persisted session from disk and refreshes the access token.
pub(crate) async fn refresh_from_file() -> Result<Authorization, Box<dyn Error>> {
    let serialized_authorization = fs::read_to_string(MONKEYTYPE_REFRESH_TOKEN_PATH)?;
    let mut authorization: Authorization = serde_json::from_str(&serialized_authorization)?;
    let e = match get_refreshed_authorization(
        authorization.refresh_token.clone(),
        authorization.api_key.clone(),
    )
    .await
    {
        Err(e) => e,
        Ok(a) => {
            authorization.update(a);
            return Ok(authorization);
        }
    };
    Err(e)
}
