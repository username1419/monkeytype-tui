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

    let auth = Authorization::from_refresh_response(response.into()).unwrap();

    assert_eq!(auth.get_access_token(), "access-abc");
    assert_eq!(auth.get_expires_in(), &Duration::from_secs(3600));
    assert_eq!(auth.get_token_type(), "Bearer");
    assert_eq!(auth.get_refresh_token(), "refresh-abc");
    assert_eq!(auth.get_user_id(), "uid-1");
    assert_eq!(auth.get_project_id(), "proj-1");
}

// Reads the cached Firebase API key straight from the real data directory.
// No network is involved (get_api_key short-circuits on the on-disk cache); the
// test is skipped entirely when the cache has never been populated.
#[tokio::test]
async fn get_api_key_reads_cached_apikey() {
    let cached = crate::DATA_DIR.join("apikey");
    let Ok(contents) = std::fs::read_to_string(&cached) else {
        return;
    };

    let client = reqwest::Client::new();
    let api_key = crate::auth::get_api_key(&client).await.unwrap();

    assert_eq!(api_key, contents.trim());
}

#[test]
fn is_logged_in_requires_refresh_token() {
    let logged_in = Authorization::new(
        String::new(),
        String::new(),
        String::new(),
        None,
        3600,
        String::new(),
        "refresh-token".into(),
        String::new(),
        String::new(),
    );
    assert!(logged_in.is_logged_in());

    let logged_out = Authorization::new(
        String::new(),
        String::new(),
        String::new(),
        None,
        3600,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );
    assert!(!logged_out.is_logged_in());
}

#[test]
fn access_expiry_depends_on_timestamp_and_duration() {
    // Fresh token: issued now, lives for an hour.
    let fresh = Authorization::new(
        String::new(),
        String::new(),
        String::new(),
        Some(Instant::now()),
        3600,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );
    assert!(!fresh.is_access_expired());
    assert!(fresh.get_expire_instant() > Instant::now());

    // Stale token: issued an hour ago, lives for a second.
    let stale = Authorization::new(
        String::new(),
        String::new(),
        String::new(),
        Some(Instant::now() - Duration::from_secs(3600)),
        1,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );
    assert!(stale.is_access_expired());
    assert!(stale.get_expire_instant() < Instant::now());

    // No timestamp: defaults to now, so a positive lifetime is not yet expired.
    let no_timestamp = Authorization::new(
        String::new(),
        String::new(),
        String::new(),
        None,
        3600,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );
    assert!(!no_timestamp.is_access_expired());
}

#[test]
fn save_to_disk_writes_expected_refresh_token_json() {
    let path = crate::DATA_DIR.join("refresh_token");
    let original = std::fs::read_to_string(&path).ok();

    let auth = Authorization::new(
        "api-key".into(),
        "TestUser".into(),
        "access-token".into(),
        Some(Instant::now()),
        3600,
        "Bearer".into(),
        "refresh-token".into(),
        "user-123".into(),
        "project-abc".into(),
    );
    auth.save_to_disk();

    let contents = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(json["refresh_token"], "refresh-token");
    assert_eq!(json["display_name"], "TestUser");
    assert_eq!(json["user_id"], "user-123");
    assert_eq!(json["project_id"], "project-abc");
    assert_eq!(json["token_type"], "Bearer");
    assert_eq!(json["api_key"], "api-key");

    // Restore the pre-test state so we don't clobber a real session file.
    match original {
        Some(contents) => {
            std::fs::write(&path, contents).unwrap();
        }
        None => {
            std::fs::remove_file(&path).ok();
        }
    }
}
