use std::env;
use std::fs::create_dir_all;
use std::fs::remove_dir_all;
use std::fs::write;
use std::path::Path;

use tokio::time::Duration;
use tokio::time::Instant;

use crate::auth::Authorization;

#[cfg(test)]
fn sample_auth() -> Authorization {
    Authorization::new(
        "api-key".into(),
        "TestUser".into(),
        "access-token".into(),
        Some(Instant::now()),
        3600,
        "Bearer".into(),
        "refresh-token".into(),
        "user-123".into(),
        "project-abc".into(),
    )
}

#[test]
fn authorization_getters_return_constructed_values() {
    let auth = sample_auth();

    assert_eq!(auth.get_display_name(), "TestUser");
    assert_eq!(auth.get_access_token(), "access-token");
    assert_eq!(auth.get_expires_in(), &Duration::from_secs(3600));
    assert_eq!(auth.get_token_type(), "Bearer");
    assert_eq!(auth.get_refresh_token(), "refresh-token");
    assert_eq!(auth.get_user_id(), "user-123");
    assert_eq!(auth.get_project_id(), "project-abc");
}

#[test]
fn authorization_is_not_expired_when_fresh() {
    let auth = sample_auth();
    assert!(!auth.is_access_expired());
    assert!(auth.get_expire_instant() > Instant::now());
}

#[test]
fn authorization_update_merges_non_default_fields() {
    let mut auth = sample_auth();
    let patch = Authorization::new(
        String::new(),
        String::new(),
        "new-access".into(),
        None,
        7200,
        "Bearer".into(),
        String::new(),
        "new-id".into(),
        String::new(),
    );

    auth.update(patch);

    assert_eq!(auth.get_display_name(), "TestUser");
    assert_eq!(auth.get_access_token(), "new-access");
    assert_eq!(auth.get_expires_in(), &Duration::from_secs(7200));
    assert_eq!(auth.get_refresh_token(), "refresh-token");
}

#[test]
fn from_login_response_parses_firebase_sign_in_json() {
    let response = r#"{
        "displayName": "Monkey",
        "expiresIn": "3600",
        "refreshToken": "refresh-abc",
        "idToken": "id-xyz"
    }"#;

    let auth = Authorization::from_login_response(response.into()).unwrap();

    assert_eq!(auth.get_display_name(), "Monkey");
    assert_eq!(auth.get_expires_in(), &Duration::from_secs(3600));
    assert_eq!(auth.get_refresh_token(), "refresh-abc");
    assert!(auth.get_expire_instant() > Instant::now());
}

#[test]
fn from_login_response_rejects_invalid_json() {
    assert!(Authorization::from_login_response("not json".into()).is_err());
}

#[test]
fn from_refresh_response_parses_token_json() {
    let response = r#"{
        "access_token": "access-abc",
        "expires_in": "3600",
        "token_type": "Bearer",
        "refresh_token": "refresh-abc",
        "id_token": "id-abc",
        "user_id": "uid-1",
        "project_id": "proj-1"
    }"#;

    let auth = Authorization::from_refresh_response("{}".into(), response.into()).unwrap();

    assert_eq!(auth.get_access_token(), "access-abc");
    assert_eq!(auth.get_expires_in(), &Duration::from_secs(3600));
    assert_eq!(auth.get_token_type(), "Bearer");
    assert_eq!(auth.get_refresh_token(), "refresh-abc");
    assert_eq!(auth.get_user_id(), "uid-1");
    assert_eq!(auth.get_project_id(), "proj-1");
}

#[tokio::test]
async fn get_api_key_reads_from_project_cache_files() {
    if !Path::new("./monkeytype.html").exists()
        || !Path::new("./monkeytype.js").exists()
        || !Path::new("./auth-constants.js").exists()
    {
        return;
    }

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:152.0) Gecko/20100101 Firefox/152.0")
        .build()
        .unwrap();

    let api_key = crate::auth::get_api_key(&client).await.unwrap();

    assert!(!api_key.is_empty());
    if Path::new("./apikey").exists() {
        let cached = std::fs::read_to_string("./apikey").unwrap();
        assert_eq!(api_key, cached.trim());
    }
}
