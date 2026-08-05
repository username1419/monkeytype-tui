use crate::command::{Command, CommandGroup, Fuzzy};

#[cfg(test)]
fn sample_commands() -> Vec<Command> {
    vec![
        Command::new(
            "test.mode".into(),
            "test mode".into(),
            CommandGroup::Test,
            || async { Ok(true) },
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "theme".into(),
            "theme dark".into(),
            CommandGroup::Theme,
            || async { Ok(true) },
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "test".into(),
            "test".into(),
            CommandGroup::Test,
            || async { Ok(true) },
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "restart".into(),
            "restart test".into(),
            CommandGroup::Test,
            || async { Ok(true) },
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "sound".into(),
            "sound volume".into(),
            CommandGroup::Sound,
            || async { Ok(true) },
            None,
            |_| async { Ok(()) },
        ),
    ]
}

#[tokio::test]
async fn find_fuzzy_empty_prompt_returns_no_matches() {
    let commands = sample_commands();
    assert!(commands.find_fuzzy("", 5).await.is_empty());
}

#[tokio::test]
async fn find_fuzzy_whitespace_prompt_returns_no_matches() {
    let commands = sample_commands();
    assert!(commands.find_fuzzy("   ", 5).await.is_empty());
}

#[tokio::test]
async fn find_fuzzy_matches_word_prefixes_case_insensitively() {
    let commands = sample_commands();

    // Every word starting with the (lowercased) prompt term matches, including
    // "restart test" whose matching word is not the first word.
    let matches = commands.find_fuzzy("TEST", 5).await;
    assert_eq!(matches, vec![0, 2, 3]);
}

#[tokio::test]
async fn find_fuzzy_keeps_insertion_order_for_equal_strengths() {
    let commands = sample_commands();

    // "test mode", "test", and "restart test" all score 4 ("TEST" term length),
    // so ties keep their original insertion order.
    let matches = commands.find_fuzzy("test", 5).await;
    assert_eq!(matches, vec![0, 2, 3]);
}

#[tokio::test]
async fn find_fuzzy_matches_later_words_when_prompt_has_multiple_terms() {
    let commands = sample_commands();

    // "restart test" scores 12 ("restart" + "test"), ahead of "test mode" and
    // "test" which only match the "test" term (score 4 each).
    let matches = commands.find_fuzzy("restart test", 5).await;
    assert_eq!(matches, vec![3, 0, 2]);
}

#[tokio::test]
async fn find_fuzzy_combines_strength_across_distinct_words() {
    let commands = sample_commands();

    // "test m" matches "test mode" on both words (4 + 1) while "test" and
    // "restart test" only match the first term (4 each).
    let matches = commands.find_fuzzy("test m", 5).await;
    assert_eq!(matches, vec![0, 2, 3]);
}

#[tokio::test]
async fn find_fuzzy_exact_display_name_ranks_first() {
    let commands = sample_commands();

    // "test mode" scores 8 (both terms) ahead of "test" and "restart test"
    // which score 4 each.
    let matches = commands.find_fuzzy("test mode", 5).await;
    assert_eq!(matches, vec![0, 2, 3]);
}

#[tokio::test]
async fn find_fuzzy_matches_multi_word_prompts() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("theme dark", 5).await;

    assert_eq!(matches, vec![1]);
}

#[tokio::test]
async fn find_fuzzy_respects_options_limit() {
    let commands = sample_commands();

    // Every command starting with "t" scores 1. The result window never grows
    // past the limit; once full, a new equal-strength candidate evicts the
    // earliest match, so the later commands win the ties.
    let matches = commands.find_fuzzy("t", 2).await;
    assert_eq!(matches.len(), 2);
    assert_eq!(matches, vec![2, 3]);
}

#[tokio::test]
async fn find_fuzzy_substrings_not_at_word_start_do_not_match() {
    let commands = sample_commands();

    // Matching is a per-word prefix match, so a substring that is not a word
    // prefix ("est" inside "test") never matches.
    assert!(commands.find_fuzzy("est", 5).await.is_empty());
}

#[tokio::test]
async fn find_fuzzy_trailing_whitespace_in_prompt_is_ignored() {
    let commands = sample_commands();

    // The trailing space splits into an empty term which contributes no
    // strength, so results match the trimmed prompt.
    let matches = commands.find_fuzzy("test ", 5).await;
    assert_eq!(matches, commands.find_fuzzy("test", 5).await);
}

#[tokio::test]
async fn find_fuzzy_returns_no_match_for_unrelated_prompt() {
    let commands = sample_commands();
    assert!(commands.find_fuzzy("zzzz", 5).await.is_empty());
}

#[tokio::test]
async fn find_fuzzy_excludes_commands_hidden_by_display_condition() {
    let commands = [
        Command::new(
            "shown".into(),
            "shown command".into(),
            CommandGroup::Other,
            || async { Ok(true) },
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "hidden".into(),
            "hidden command".into(),
            CommandGroup::Other,
            || async { Ok(false) },
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "err".into(),
            "err command".into(),
            CommandGroup::Other,
            || async { Err("nope".to_string()) },
            None,
            |_| async { Ok(()) },
        ),
    ];

    // All three match on strength, but the second is hidden by its
    // display_condition and the third's condition errors out.
    let matches = commands.find_fuzzy("shown command", 5).await;

    assert_eq!(matches, vec![0]);
}
