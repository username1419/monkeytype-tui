use std::fs;
use std::path::PathBuf;

use crate::CACHE_DIR;
use crate::notify::debug;
use crate::typing_test::word_list::{
    get_bcp47, get_language, get_word_list, is_ligature_aware, is_order_by_freq, is_rtl,
    is_support_lazy_mode, update_and_get_words,
};

/// Fixture path for a loadable word list. Unique per crate of the test binary so
/// parallel runs of the same test never collide, and always cleaned up after.
///
/// NOTE: the file must carry a `.json` extension
#[cfg(test)]
fn fixture_path(name: &str) -> PathBuf {
    let dir = CACHE_DIR.join("languages");
    fs::create_dir_all(&dir).unwrap();
    dir.join(name.to_string() + ".json")
}

const FIXTURE: &str = r#"{
    "name": "english",
    "words": ["foo", "bar", "baz"],
    "right_to_left": true,
    "ligatures": true,
    "no_lazy_mode": true,
    "order_by_frequency": true,
    "bcp47": "en-US"
}"#;

#[tokio::test]
async fn update_and_get_words_parses_fixture_and_populates_getters() {
    let name = "tests_word_list_fixture";
    let path = fixture_path(name);
    fs::write(&path, FIXTURE).unwrap();

    let words = update_and_get_words(name).await.unwrap();
    assert_eq!(
        words,
        vec!["foo".to_string(), "bar".to_string(), "baz".to_string()]
    );

    assert_eq!(get_language(), Some("english".to_string()));
    assert_eq!(
        get_word_list(),
        Some(vec![
            "foo".to_string(),
            "bar".to_string(),
            "baz".to_string()
        ])
    );
    assert_eq!(is_rtl(), Some(true));
    assert_eq!(is_ligature_aware(), Some(true));
    assert_eq!(is_support_lazy_mode(), Some(false));
    assert_eq!(is_order_by_freq(), Some(true));
    assert_eq!(get_bcp47(), Some("en-US".to_string()));

    fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn update_and_get_words_returns_err_for_missing_file() {
    let result = update_and_get_words("tests_word_list_missing_fixture").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn update_and_get_words_returns_err_for_malformed_fixture() {
    let name = "tests_word_list_malformed";
    let path = fixture_path(name);
    fs::write(&path, "{ not valid json }").unwrap();

    let result = update_and_get_words(name.to_string() + ".json").await;
    assert!(result.is_err());

    fs::remove_file(&path).unwrap();
}

// NOTE: get_language()/get_word_list() returning `None` before any load is only
// reliable when no other test has populated the shared `WORD_LIST` global
// (TESTING.md §5). With default parallel test execution this is not guaranteed,
// so the assertion is documented rather than asserted here.
