use crate::command::{Command, CommandGroup, Fuzzy};

fn sample_commands() -> Vec<Command> {
    vec![
        Command::new(
            "test.mode".into(),
            "test mode".into(),
            CommandGroup::Test,
            vec![],
            None,
            |_| async { Ok(String::new()) },
        ),
        Command::new(
            "theme".into(),
            "theme dark".into(),
            CommandGroup::Theme,
            vec![],
            None,
            |_| async { Ok(String::new()) },
        ),
        Command::new(
            "restart".into(),
            "restart test".into(),
            CommandGroup::Test,
            vec![],
            None,
            |_| async { Ok(String::new()) },
        ),
        Command::new(
            "sound".into(),
            "sound volume".into(),
            CommandGroup::Sound,
            vec![],
            None,
            |_| async { Ok(String::new()) },
        ),
    ]
}

#[test]
fn find_fuzzy_empty_prompt_returns_no_matches() {
    let commands = sample_commands();
    assert!(commands.find_fuzzy("", 5).is_empty());
    assert!(commands.find_fuzzy("   ", 5).is_empty());
}

#[test]
fn find_fuzzy_matches_prefixes_case_insensitively() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("TEST", 5);

    assert_eq!(matches, vec![0]);
}

#[test]
fn find_fuzzy_matches_later_words_when_prompt_has_multiple_terms() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("restart test", 5);

    assert_eq!(matches, vec![2]);
}

#[test]
fn find_fuzzy_respects_options_limit() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("t", 2);

    assert_eq!(matches.len(), 2);
}

#[test]
fn find_fuzzy_prefers_stronger_matches() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("test mode", 5);

    assert_eq!(matches.first().copied(), Some(0));
}

#[test]
fn find_fuzzy_matches_multi_word_prompts() {
    let commands = sample_commands();
    let matches = commands.find_fuzzy("theme dark", 5);

    assert_eq!(matches, vec![1]);
}

#[test]
fn find_fuzzy_returns_no_match_for_unrelated_prompt() {
    let commands = sample_commands();
    assert!(commands.find_fuzzy("zzzz", 5).is_empty());
}
