use crate::command::{Command, CommandGroup, Fuzzy};

#[cfg(test)]
fn sample_commands() -> Vec<Command> {
    vec![
        Command::new(
            "test.mode".into(),
            "test mode".into(),
            CommandGroup::Test,
            || async { Ok(true) },
            vec![],
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "theme".into(),
            "theme dark".into(),
            CommandGroup::Theme,
            || async { Ok(true) },
            vec![],
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "test".into(),
            "test".into(),
            CommandGroup::Test,
            || async { Ok(true) },
            vec![],
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "restart".into(),
            "restart test".into(),
            CommandGroup::Test,
            || async { Ok(true) },
            vec![],
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "sound".into(),
            "sound volume".into(),
            CommandGroup::Sound,
            || async { Ok(true) },
            vec![],
            None,
            |_| async { Ok(()) },
        ),
    ]
}

#[tokio::test]
async fn find_fuzzy_empty_prompt_returns_no_matches() {
    let commands = sample_commands();
    assert!(commands.find_fuzzy("", 5).await.is_empty());
    assert!(commands.find_fuzzy("   ", 5).await.is_empty());
}

#[tokio::test]
async fn find_fuzzy_matches_prefixes_case_insensitively() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("TEST", 5).await;

    // "test mode" and "test" both match with equal strength, in insertion order.
    assert_eq!(matches, vec![0, 2]);
}

#[tokio::test]
async fn find_fuzzy_matches_later_words_when_prompt_has_multiple_terms() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("restart test", 5).await;

    assert_eq!(matches, vec![3]);
}

#[tokio::test]
async fn find_fuzzy_respects_options_limit() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("t", 2).await;

    assert_eq!(matches.len(), 2);
}

#[tokio::test]
async fn find_fuzzy_prefers_stronger_matches() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("test m", 5).await;

    // "test mode" (strength 6) sorts ahead of "test" (strength 4).
    assert_eq!(matches, vec![0, 2]);
}

#[tokio::test]
async fn find_fuzzy_matches_multi_word_prompts() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("theme dark", 5).await;

    assert_eq!(matches, vec![1]);
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
            vec![],
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "hidden".into(),
            "hidden command".into(),
            CommandGroup::Other,
            || async { Ok(false) },
            vec![],
            None,
            |_| async { Ok(()) },
        ),
        Command::new(
            "err".into(),
            "err command".into(),
            CommandGroup::Other,
            || async { Err("nope".to_string()) },
            vec![],
            None,
            |_| async { Ok(()) },
        ),
    ];

    // All three would match "shown command" on strength, but the second is hidden
    // by its display_condition and the third's condition errors out.
    let matches = commands.find_fuzzy("shown command", 5).await;

    assert_eq!(matches, vec![0]);
}
