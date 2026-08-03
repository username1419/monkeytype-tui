use reqwest::Client;
use serde_json::json;
use tokio::{runtime::Runtime, time::Instant};

use crate::{
    auth::USER_AGENT,
    github::{
        GithubContentItem, GithubContentItemLinkCollection, download_resources_recursive,
        get_tags, has_version_changed,
    },
    notify::{NotificationManager, Notify, notify},
};

#[test]
fn github_file_object_deserialize() {
    let s = r#"{
        "_links": {
            "git": "https://api.github.com/repos/monkeytypegame/monkeytype/git/blobs/8b8974557b87a269e5977bab3e66f5c79b3990cc",
           "html": "https://github.com/monkeytypegame/monkeytype/blob/v26.28.0/frontend/static/contributors.json",
           "self": "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/contributors.json?ref=v26.28.0"
        },
        "download_url": "https://raw.githubusercontent.com/monkeytypegame/monkeytype/v26.28.0/frontend/static/contributors.json",
        "git_url": "https://api.github.com/repos/monkeytypegame/monkeytype/git/blobs/8b8974557b87a269e5977bab3e66f5c79b3990cc",
        "html_url": "https://github.com/monkeytypegame/monkeytype/blob/v26.28.0/frontend/static/contributors.json",
        "name": "contributors.json",
        "path": "frontend/static/contributors.json",
        "sha": "8b8974557b87a269e5977bab3e66f5c79b3990cc",
        "size": 19615,
        "type": "file",
        "url": "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/contributors.json?ref=v26.28.0"
        }"#;

    let j = serde_json::from_str::<GithubContentItem>(s).unwrap();
    assert_eq!(
        j,
        GithubContentItem {
            _links: GithubContentItemLinkCollection {
                git: "https://api.github.com/repos/monkeytypegame/monkeytype/git/blobs/8b8974557b87a269e5977bab3e66f5c79b3990cc".into(),
                html: "https://github.com/monkeytypegame/monkeytype/blob/v26.28.0/frontend/static/contributors.json".into(),
                _self: "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/contributors.json?ref=v26.28.0".into(),
            },
            download_url: Some("https://raw.githubusercontent.com/monkeytypegame/monkeytype/v26.28.0/frontend/static/contributors.json".into()),
            git_url: "https://api.github.com/repos/monkeytypegame/monkeytype/git/blobs/8b8974557b87a269e5977bab3e66f5c79b3990cc".into(),
            html_url: "https://github.com/monkeytypegame/monkeytype/blob/v26.28.0/frontend/static/contributors.json".into(),
            name: "contributors.json".into(),
            path: "frontend/static/contributors.json".into(),
            sha: "8b8974557b87a269e5977bab3e66f5c79b3990cc".into(),
            size: 19615,
            _type: crate::github::GithubContentItemType::File,
            url: "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/contributors.json?ref=v26.28.0".into(),
        }
    );
}

#[test]
fn github_dir_object_deserialize() {
    let s = r#"{
        "_links": {
            "git": "https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/961e5b3089b0bcf480ee3a5e1f5576f779184df6",
            "html": "https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/.well-known",
            "self": "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/.well-known?ref=v26.28.0"
        },
        "download_url": null,
        "git_url": "https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/961e5b3089b0bcf480ee3a5e1f5576f779184df6",
        "html_url": "https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/.well-known",
        "name": ".well-known",
        "path": "frontend/static/.well-known",
        "sha": "961e5b3089b0bcf480ee3a5e1f5576f779184df6",
        "size": 0,
        "type": "dir",
        "url": "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/.well-known?ref=v26.28.0"
    }"#;

    let j = serde_json::from_str::<GithubContentItem>(s).unwrap();
    assert_eq!(
        j,
        GithubContentItem {
            _links: GithubContentItemLinkCollection {
                git: "https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/961e5b3089b0bcf480ee3a5e1f5576f779184df6".into(),
                html: "https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/.well-known".into(),
                _self: "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/.well-known?ref=v26.28.0".into()
            },
            download_url: None,
            git_url: "https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/961e5b3089b0bcf480ee3a5e1f5576f779184df6".into(),
            html_url: "https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/.well-known".into(),
            name: ".well-known".into(),
            path: "frontend/static/.well-known".into(),
            sha: "961e5b3089b0bcf480ee3a5e1f5576f779184df6".into(),
            size: 0,
            _type: crate::github::GithubContentItemType::Dir,
            url: "https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/.well-known?ref=v26.28.0".into()
        }
    );
}

#[test]
#[ignore = "inet"]
fn download_test() {
    #[cfg(feature = "profiling")]
    console_subscriber::init();
    let rt = Runtime::new().unwrap();

    let client = Client::builder().user_agent(USER_AGENT).build().unwrap();
    let version = rt
        .block_on(get_tags(&client))
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let past = Instant::now();
    let c = rt
        .block_on(download_resources_recursive(&client, version))
        .unwrap();
    let duration = Instant::now() - past;

    println!("Completed. Skipped {} files.", c);
    println!("Process took {} seconds", duration.as_secs_f64());
}

// With an empty data directory `has_version_changed` short-circuits to
// `Ok(true)` without any network access. This is environment-dependent: it only
// holds when the real data dir is empty, so the test skips itself otherwise.
// It is also sensitive to test ordering (other tests may write into DATA_DIR).
#[tokio::test]
async fn has_version_changed_true_when_data_dir_empty() {
    let Ok(mut entries) = tokio::fs::read_dir(crate::DATA_DIR.as_path()).await else {
        return;
    };
    if entries.next_entry().await.ok().flatten().is_some() {
        return;
    }

    let changed = has_version_changed().await.unwrap();
    assert!(changed);
}
