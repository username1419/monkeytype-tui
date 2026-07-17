//! Utility set of functions to interact with the monkeytype official github repository

// TODO: get words list
// steps:
//   1. GET https://api.github.com/repos/monkeytypegame/monkeytype/tags
//   2. parse into json
//   3. get first object
//   4. get kv 'name'
//   5. GET https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static?ref=[name]
//   6. parse json
//      - format:
//      [{'_links': {'git': 'https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/961e5b3089b0bcf480ee3a5e1f5576f779184df6',
//             'html': 'https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/.well-known',
//             'self': 'https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/.well-known?ref=v26.28.0'},
//        'download_url': None,
//        'git_url': 'https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/961e5b3089b0bcf480ee3a5e1f5576f779184df6',
//        'html_url': 'https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/.well-known',
//        'name': '.well-known',
//        'path': 'frontend/static/.well-known',
//        'sha': '961e5b3089b0bcf480ee3a5e1f5576f779184df6',
//        'size': 0,
//        'type': 'dir',
//        'url': 'https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/.well-known?ref=v26.28.0'},
//       {'_links': {'git': 'https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/143607d35a4e077e21ed7caf15cc37f11248201a',
//                   'html': 'https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/challenges',
//                   'self': 'https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/challenges?ref=v26.28.0'},
//        'download_url': None,
//        'git_url': 'https://api.github.com/repos/monkeytypegame/monkeytype/git/trees/143607d35a4e077e21ed7caf15cc37f11248201a',
//        'html_url': 'https://github.com/monkeytypegame/monkeytype/tree/v26.28.0/frontend/static/challenges',
//        'name': 'challenges',
//        'path': 'frontend/static/challenges',
//        'sha': '143607d35a4e077e21ed7caf15cc37f11248201a',
//        'size': 0,
//        'type': 'dir',
//        'url': 'https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/challenges?ref=v26.28.0'},
//       {'_links': {'git': 'https://api.github.com/repos/monkeytypegame/monkeytype/git/blobs/8b8974557b87a269e5977bab3e66f5c79b3990cc',
//                   'html': 'https://github.com/monkeytypegame/monkeytype/blob/v26.28.0/frontend/static/contributors.json',
//                   'self': 'https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/contributors.json?ref=v26.28.0'},
//        'download_url': 'https://raw.githubusercontent.com/monkeytypegame/monkeytype/v26.28.0/frontend/static/contributors.json',
//        'git_url': 'https://api.github.com/repos/monkeytypegame/monkeytype/git/blobs/8b8974557b87a269e5977bab3e66f5c79b3990cc',
//        'html_url': 'https://github.com/monkeytypegame/monkeytype/blob/v26.28.0/frontend/static/contributors.json',
//        'name': 'contributors.json',
//        'path': 'frontend/static/contributors.json',
//        'sha': '8b8974557b87a269e5977bab3e66f5c79b3990cc',
//        'size': 19615,
//        'type': 'file',
//        'url': 'https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static/contributors.json?ref=v26.28.0'},
//        ...
//   7. pull every single object with type 'file' and recursively traverse the ones with type 'dir'
//   8. profit?

use std::{
    collections::VecDeque,
    fs::{self, create_dir},
    sync::{Arc, Mutex, atomic::AtomicU32},
    time::Duration,
};

use reqwest::Client;
use serde::Deserialize;
use tokio::{spawn, sync::Semaphore, time::sleep};
use tokio_util::task::TaskTracker;

use crate::{
    CACHE_DIR,
    notify::{debug, enotify},
};

#[derive(Deserialize, Debug, PartialEq)]
pub(crate) struct GithubContentItemLinkCollection {
    pub(crate) git: String,
    pub(crate) html: String,
    #[serde(rename = "self")]
    pub(crate) _self: String,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GithubContentItemType {
    File,
    Dir,
}

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

#[derive(Deserialize, Debug)]
pub(crate) struct GithubTagsResponse {
    pub(crate) name: String,
    pub(crate) zipball_url: String,
    pub(crate) tarball_url: String,
    pub(crate) commit: GithubTagsResponseCommit,
    pub(crate) node_id: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct GithubTagsResponseCommit {
    pub(crate) sha: String,
    pub(crate) url: String,
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

const IGNORE_DIR: &[&str] = &["images"];
/// downloads the files in https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static?ref=[version]
/// recursively, traversing using bfs
///
/// this function is blocking; call it on a worker thread. dont use .await on a non-blocking thread
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
    // NOTE: i just ball it and hope its the correct number of items
    let root = client.get(format!("https://api.github.com/repos/monkeytypegame/monkeytype/contents/frontend/static?ref={}", version)).send().await?.json::<VecDeque<GithubContentItem>>().await?;
    let skipped = Arc::new(AtomicU32::default());
    let semaphore = Arc::new(Semaphore::new(128));

    let tracker = TaskTracker::new();

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

    Ok(skipped.load(std::sync::atomic::Ordering::Acquire))
}

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
                let Ok(_permit) = semaphore.acquire().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Some(url) = item.download_url else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(response) = client.get(url).send().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                let Ok(contents) = response.bytes().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                drop(_permit);

                if let Err(e) = fs::write(&path, contents) {
                    enotify!(e);
                    enotify!(path);
                }
            }
            GithubContentItemType::Dir => {
                let path = CACHE_DIR.join(item.path.strip_prefix("frontend/static/").unwrap());

                if IGNORE_DIR.contains(&item.name.as_str()) {
                    return;
                }

                let Ok(_) = create_dir(&path) else {
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

                let Ok(dir_contents) = response.json::<Vec<GithubContentItem>>().await else {
                    skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                };

                drop(_permit);

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
