use std::{error::Error, ffi::OsString, sync::Mutex};

use once_cell::sync::Lazy;
use serde::Deserialize;
use tokio::{fs, io};

use crate::{CACHE_DIR, notify::enotify};

pub(crate) async fn get_word_lists() -> io::Result<Vec<OsString>> {
    let dirs = fs::read_dir(CACHE_DIR.join("languages")).await;

    match dirs {
        Ok(mut dirs) => {
            let mut v = Vec::new();
            while let Ok(Some(dir)) = dirs.next_entry().await {
                if let Some(language) = dir.path().file_stem() {
                    // i dont like this but eh
                    v.push(language.to_os_string());
                }
            }
            Ok(v)
        }
        Err(err) => Err(err),
    }
}

static WORD_LIST: Lazy<Mutex<Option<WordList>>> = Lazy::new(|| Mutex::new(None));

#[derive(Deserialize)]
struct WordList {
    name: String,
    words: Vec<String>,
    #[serde(default)]
    right_to_left: bool, // optional
    #[serde(default)]
    ligatures: bool, // optional
    #[serde(default)]
    no_lazy_mode: bool, // optional
    #[serde(default)]
    order_by_frequency: bool, // optional
    #[serde(default)]
    bcp47: String, // optional
}

fn update_word_list(word_list: WordList) {
    *WORD_LIST.lock().expect("WORD_LIST is poisoned") = Some(word_list);
}

pub(crate) fn get_language() -> Option<String> {
    WORD_LIST
        .lock()
        .expect("WORD_LIST is poisoned")
        .as_ref()
        .map(|w| w.name.clone())
}

pub(crate) fn get_word_list() -> Option<Vec<String>> {
    WORD_LIST
        .lock()
        .expect("WORD_LIST is poisoned")
        .as_ref()
        .map(|w| w.words.clone())
}

pub(crate) fn is_rtl() -> Option<bool> {
    WORD_LIST
        .lock()
        .expect("WORD_LIST is poisoned")
        .as_ref()
        .map(|w| w.right_to_left)
}

pub(crate) fn is_ligature_aware() -> Option<bool> {
    WORD_LIST
        .lock()
        .expect("WORD_LIST is poisoned")
        .as_ref()
        .map(|w| w.ligatures)
}

pub(crate) fn is_support_lazy_mode() -> Option<bool> {
    WORD_LIST
        .lock()
        .expect("WORD_LIST is poisoned")
        .as_ref()
        .map(|w| !w.no_lazy_mode)
}

pub(crate) fn is_order_by_freq() -> Option<bool> {
    WORD_LIST
        .lock()
        .expect("WORD_LIST is poisoned")
        .as_ref()
        .map(|w| w.order_by_frequency)
}

pub(crate) fn get_bcp47() -> Option<String> {
    WORD_LIST
        .lock()
        .expect("WORD_LIST is poisoned")
        .as_ref()
        .map(|w| w.bcp47.clone())
}

pub(crate) async fn update_and_get_words(
    language: OsString,
) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    let mut rel_path = OsString::from("languages/");
    rel_path.push(language);
    let file_contents = fs::read_to_string(CACHE_DIR.join(rel_path)).await;

    match file_contents {
        Ok(contents) => match serde_json::from_str::<WordList>(&contents) {
            Ok(word_list) => {
                update_word_list(word_list);
                Ok(get_word_list().unwrap())
            }
            Err(err) => {
                enotify!(&err);
                Err(err.into())
            }
        },
        Err(err) => Err(err.into()),
    }
}
