//! Utilities for interacting with the official monkeytype GitHub repository.
//!
//! This module keeps the locally cached game assets (language word lists, challenges,
//! quotes, etc.) in sync with the latest release:
//!
//! 1. `GET https://api.github.com/repos/monkeytypegame/monkeytype/tags` to find the
//!    latest release tag ([`get_tags`]).
//! 2. List `frontend/static` at that tag via the GitHub Contents API and recursively
//!    download every file using a breadth-first traversal ([`download_resources_recursive`]).
//!
//! The last successfully synced tag is persisted to `<data>/webclient_version`, so
//! [`has_version_changed`] can decide on the next launch whether a refresh is needed.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Deserialize;
use tokio::{
    fs::{self, create_dir, read_to_string, write},
    sync::Semaphore,
};
use tokio_util::task::TaskTracker;

use crate::{
    CACHE_DIR, DATA_DIR,
    auth::CLIENT,
    notify::{debug, enotify},
};

/// Hypermedia links returned by the GitHub Contents API.
#[derive(Deserialize, Debug, PartialEq)]
pub(crate) struct GithubContentItemLinkCollection {
    pub(crate) git: String,
    pub(crate) html: String,
    #[serde(rename = "self")]
    pub(crate) _self: String,
}

/// Whether a GitHub content entry is a file or a directory.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GithubContentItemType {
    File,
    Dir,
}

/// A single entry (file or directory) from the GitHub Contents API response.
#[derive(Deserialize, Debug, PartialEq)]
pub(crate) struct GithubContentItem {
    pub(crate) _links: GithubContentItemLinkCollection,
    pub(crate) download_url: Option<String>,
    pub(crate) git_url: String,
    pub(crate) html_url: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) sha: String,
    pub(crate) size: u32,
    #[serde(rename = "type")]
    pub(crate) _type: GithubContentItemType,
    pub(crate) url: String,
}

/// A single tag object from the GitHub Tags API response.
#[derive(Deserialize, Debug)]
pub(crate) struct GithubTagsResponse {
    pub(crate) name: String,
    pub(crate) zipball_url: String,
    pub(crate) tarball_url: String,
    pub(crate) commit: GithubTagsResponseCommit,
    pub(crate) node_id: String,
}

/// Commit metadata inside a [`GithubTagsResponse`] entry.
#[derive(Deserialize, Debug)]
pub(crate) struct GithubTagsResponseCommit {
    pub(crate) sha: String,
    pub(crate) url: String,
}

/// Path to the file that stores the currently cached game-asset version tag.
static VERSIONING_FILE: Lazy<PathBuf> = Lazy::new(|| DATA_DIR.join("webclient_version"));

/// Checks whether the locally cached game assets are out of date by comparing
/// the stored version tag against the latest GitHub release tag.
pub(crate) async fn has_version_changed() -> Result<bool, Box<dyn std::error::Error + Send + Sync>>
{
    if fs::read_dir(DATA_DIR.as_path())
        .await?
        .next_entry()
        .await?
        .is_none()
    {
        return Ok(true);
    }
    let saved_version = read_to_string(&*VERSIONING_FILE).await?;
    let tags = get_tags(&CLIENT).await?;
    let latest_version = tags.first().unwrap();

    // NOTE: since this heavily depends on the fact that the monkeytype team does not create beta release
    // tags on master branch
    // so uhhhh prayge
    Ok(saved_version.ne(latest_version))
}

/// GET https://api.github.com/repos/monkeytypegame/monkeytype/tags
/// and returns the output
///
/// latest tag is probably at the top (from my limited testing)
pub(crate) async fn get_tags(client: &Client) -> Result<Vec<String>, reqwest::Error> {
    Ok(client
        .get("https://api.github.com/repos/monkeytypegame/monkeytype/tags")
        .send()
        .await?
        .json::<Vec<GithubTagsResponse>>()
        .await?
        .into_iter()
        .map(|r| r.name)
        .collect())
}

/// Directory names to skip when recursively downloading game assets.
const IGNORE_DIR: &[&str] = &["images"];
/// downloads the files in https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static?ref=[version]
/// recursively, traversing using bfs
///
/// we do this because the alternative is to have git as a project dependency which would be one of
/// the choices of all time
///
/// the function returns a Result<u32, reqwest::Error>, where:
///   - Ok(u32) is the amount of items which are unable to be retrieved from github
///   - Err(reqwest::Error) is the error encountered at the start of the function where retrieving files from `contents/frontend/static` fails for one reason or another
pub(crate) async fn download_resources_recursive(
    client: &Client,
    version: String,
) -> Result<u32, reqwest::Error> {
    let root = client.get(format!("https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static?ref={}", version)).send().await?.json::<VecDeque<GithubContentItem>>().await?;
    let skipped = Arc::new(AtomicU32::default());
    // NOTE: github allows max 100 concurrent requests
    let semaphore = Arc::new(Semaphore::new(100));

    let tracker = TaskTracker::new();

    let _ = fs::remove_file(VERSIONING_FILE.as_path()).await;

    for item in root {
        download_item(
            tracker.clone(),
            item,
            client.clone(),
            semaphore.clone(),
            skipped.clone(),
        );
    }

    tracker.close();
    tracker.wait().await;
    // NOTE: we write to this file at the end since it will also indicate if our previous tries
    // have been successful or not
    match write(&*VERSIONING_FILE, version).await {
        Ok(_) => {}
        Err(e) => enotify!(format!("Error while writing version file: {}", e)),
    }

    Ok(skipped.load(std::sync::atomic::Ordering::Acquire))
}

/// Recursively downloads a single GitHub content item (file or directory) into
/// the local cache, spawning child tasks for nested directories.
fn download_item(
    tracker: TaskTracker,
    item: GithubContentItem,
    client: Client,
    semaphore: Arc<Semaphore>,
    skipped: Arc<AtomicU32>,
) {
    let _t = tracker.clone();
    _t.spawn(async move {
        match item._type {
            GithubContentItemType::File => {
                let path = CACHE_DIR.join(item.path.strip_prefix("frontend/static/").unwrap());
                let Some(url) = item.download_url else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(_permit) = semaphore.acquire().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(response) = client.get(url).send().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                drop(_permit);

                let Ok(response) = response.error_for_status() else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(contents) = response.bytes().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                if let Err(e) = fs::write(&path, contents).await {
                    enotify!(e);
                    enotify!(path);
                }
            }
            GithubContentItemType::Dir => {
                let path = CACHE_DIR.join(item.path.strip_prefix("frontend/static/").unwrap());

                if IGNORE_DIR.contains(&item.name.as_str()) {
                    return;
                }

                let Ok(_) = create_dir(&path).await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(_permit) = semaphore.acquire().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(response) = client.get(item.url).send().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                drop(_permit);

                let Ok(response) = response.error_for_status() else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(dir_contents) = response.json::<Vec<GithubContentItem>>().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                for item in dir_contents {
                    download_item(
                        tracker.clone(),
                        item,
                        client.clone(),
                        semaphore.clone(),
                        skipped.clone(),
                    );
                }
            }
        }
    });
}
