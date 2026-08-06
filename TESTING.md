# Testing

How the application is tested today. Implementation lives in `src/tests/` (unit tests in a
binary crate) alongside the code it exercises; this document describes what is covered, what
is intentionally not covered yet, and the constraints the tests work under.

## Status

- `cargo test` runs **82 tests**: 81 pass and 1 is ignored (a live-network GitHub test).
- Most modules have real, passing coverage in `src/tests/`.
- The typing engine (`src/test.rs`) and the new `src/verify.rs` are **work in progress** and
  must not be tested yet (see [WIP — do not test](#wip--do-not-test)).
- A few behaviors remain untested (private globals/queues, network paths, and edge cases
  that would require a refactor). These are documented below.

## Scope & constraints

- The crate is a binary crate (`src/main.rs`), so there is no library target for integration
  tests to import. Tests live as unit tests under `src/tests/`, matching the existing pattern.
- **Network policy**: functionality that depends on live HTTP (monkeytype auth scraping,
  login/token refresh, GitHub tags/asset sync) stays `#[ignore]`d or deferred. A future
  httpmock-based harness would require injecting base URLs into `src/auth.rs` and
  `src/github.rs`, which is out of scope.
- **Shared globals**: `TEST`, `WORD_LIST`, `AUTHORIZATION`, `NOTIFICATIONS`, and the
  `CACHE_DIR`/`DATA_DIR` paths are process-wide `static`s. Tests touching them are
  order-dependent and must use unique fixture names plus cleanup (see
  [Shared test infrastructure](#shared-test-infrastructure)).

## Testability matrix

| Module | Functionality | Coverage | Notes |
|--------|---------------|----------|-------|
| `commandline.rs` | cursor input, delete, word-delete, prompt mode, search/fuzzy submit | Tested | `src/tests/commandline.rs`; input observed via the submit callback (no input getter) |
| `command.rs` | fuzzy matching (`find_fuzzy`) | Tested | `src/tests/fuzzy.rs` (`#[tokio::test]`) |
| `game.rs` | key event routing | Tested | `src/tests/game.rs`; `TEST`-branch routing not observable (no getter on `Test`) |
| `notify.rs` | builder, expiry, Display, macros | Tested | `src/tests/notify.rs`; queue drain/expiry not observable (`NOTIFICATIONS` private) |
| `typing_test/word_list.rs` | load language, getters, error paths | Tested | `src/tests/word_list.rs`; fixtures use the real `CACHE_DIR`; `WORD_LIST` global shared |
| `typing_test/random.rs` | XorShift128+ RNG, seed derivation, output ranges | Tested | Inline `#[cfg(test)]` module (below); deterministic for constructed states |
| `auth.rs` | now/expiry getters, parse, `is_logged_in`, `save_to_disk`, cached API key | Tested | `src/tests/auth.rs`; network login/refresh/deferred |
| `github.rs` | deserialization | Tested | `src/tests/github.rs`; tags/download are network (`#[ignore]`) |
| `test.rs` (typing engine) | word advance, targeting | **Not tested (WIP)** | See [WIP — do not test](#wip--do-not-test) |
| `verify.rs` | result verification | **Not tested (WIP)** | Stub behind `todo!()`; see above |
| `callbacks/*` | command handlers | Framework built | `src/tests/callback_framework.rs`; per-command tests added as handlers land |

## Per-module test descriptions

### 1. Command line (`src/commandline.rs`)

`src/tests/commandline.rs`. All methods are `pub(crate)`; a `Default` impl exists. Because
there is no getter for the input string, tests observe state through the `submit` callback
(the helper `observe_input` toggles out of search mode, submits, and captures the callback).

Covered:

- `default_state_is_disabled_and_searching` and `enable_and_disable_flip_enabled_state`.
- `disable_in_search_mode_does_not_reset` vs `disable_in_prompt_mode_resets_input`.
- `toggle_searching_flips_search_mode`.
- `characters_append_at_cursor_offset_zero`; `moving_cursor_left_changes_insertion_point`;
  `moving_cursor_right_restores_insertion_point`.
- `delete_character_removes_char_before_cursor`;
  `delete_character_on_empty_input_disables_command_line`.
- `delete_word_removes_word_before_cursor` (observe final input and the resulting
  `cursor_offset`, so typing appends after it).
- `prompt_input_switches_to_prompt_mode` (search disabled, enabled, custom prompt).
- `submit_in_prompt_mode_invokes_oneshot_callback_with_input` and
  `submit_with_remain_enabled_keeps_command_line_enabled`.
- `search_mode_update_populates_matches_and_submit_returns_selected_command` (ties in to
  `find_fuzzy` over `ROOT_COMMANDS`); `search_mode_submit_without_matches_disables_command_line`.

Known issue (present, may be fixed in a future change, do not rely on it in tests):
`register_character` at `src/commandline.rs:122` computes `input.len() - cursor_offset`,
which underflows/panics in debug builds when `cursor_offset > input.len()`.

### 2. Fuzzy matching (`src/command.rs`)

`src/tests/fuzzy.rs` (`#[tokio::test]`). `find_fuzzy` is async and returns a `Vec<usize>` of
indices into the command slice, sorted by match strength descending (ties keep insertion
order). The prompt is lowercased and split into terms; each term matches as a prefix of *any*
display-name word, and the term's length is summed into the strength.

Covered:

- Empty / whitespace-only prompts return no matches.
- Case-insensitive per-word prefix matching ("TEST" matches "test mode" and "restart test").
- Multi-term prompts match words in any position and combine strength ("restart test",
  "theme dark"); terms can land on different words.
- Exact display-name matches rank first ("test mode" vs. "test").
- The `options` limit caps the result count.
- Substrings not at a word start never match ("est" does not match "test").
- Trailing whitespace in the prompt is ignored.
- Results are sorted by match strength descending; ties keep insertion order.
- A command whose `display_condition` returns `false` (or `Err`) is excluded.

Known issue (present, may be fixed in a future change): once the result window is full, a
new candidate replaces the weakest entry using `>=` (`src/command.rs:149`), so an
equal-strength candidate evicts the earliest match. With tied strengths a full window
therefore keeps the later commands — `find_fuzzy("t", 2)` on the sample set yields `[2, 3]`,
not `[0, 1]`. Tests currently assert the implemented behavior accordingly.

### 3. Key event routing (`src/game.rs`)

`src/tests/game.rs`. `event_keypressed` takes an `Arc<Mutex<State>>`; `State` is defined at
the crate root, so its fields are reachable from tests. The `TEST` branch is not observable,
so tests focus on the commandline branch and global side effects:

- `ctrl_q_cancels_shutdown`.
- `esc_toggles_command_line_enabled_state`.
- A character routes to the command line when enabled (observe via submit) and to `TEST`
  when disabled (asserted only as "no panic"; see WIP `test.rs`).
- `shift_character_routes_uppercased_character`.
- `enter_submits_and_does_not_disable_command_line_when_enabled`; `enter_is_a_noop_when_command_line_disabled`.
- Arrow keys only affect the command line when enabled (`left_arrow_moves_cursor_in_command_line`,
  `arrow_keys_are_noops_when_command_line_disabled`).
- `ctrl_backspace_deletes_word_in_command_line` / `ctrl_h_deletes_word_in_command_line`.

### 4. Notifications (`src/notify.rs`)

`src/tests/notify.rs`.

- `builder_defaults_are_empty_message_and_info_level` and `builder_fluent_setters_are_applied`.
- `notification_display_shows_level_and_message` and `notif_level_display_labels`
  (`INFO`/`SUCCESS`/`WARNING`/`DEBUG`/`ERROR`).
- `debug_macro_fires_and_returns_value`, `error_macro_fires_and_returns_value`, and
  `enotify_wnotify_and_todo_macros_fire_without_panicking`.

Queue drain and expiry removal are not observable (the `NOTIFICATIONS` static and the channel
receiver are private), so they are intentionally not tested.

### 5. Word lists (`src/typing_test/word_list.rs`)

`src/tests/word_list.rs`.

- `update_and_get_words_parses_fixture_and_populates_getters` — parses a fixture and
  reflects it via the getters (`get_language`, `get_word_list`, `is_rtl`,
  `is_ligature_aware`, `is_support_lazy_mode`, `is_order_by_freq`, `get_bcp47`). The `update_and_get_words`
  appends `.json` to the given name, so fixtures are stored as `<name>.json`.
- `update_and_get_words_returns_err_for_missing_file` and
  `update_and_get_words_returns_err_for_malformed_fixture`.

The "`get_language`/`get_word_list` return `None` before any load" assertion is only reliable
when no other test has loaded a list (shared `WORD_LIST` global) and is therefore not
currently exercised.

### 6. Random number generator (`src/typing_test/random.rs`)

Tests live in an inline `#[cfg(test)]` module in the source file (not under `src/tests/`). The
generator uses `wrapping_mul`/`wrapping_add`, so every operation is overflow-safe and the tests
run identically in debug and release builds (unlike V8's bare `*`/`+` reference arithmetic).

Covered:

- `murmur_hash3_of_zero_is_zero`, and
  `murmur_hash3_is_avalanching_and_deterministic` (same input reproduces, one-bit input
  difference changes the output).
- `from_seed_derives_distinct_nonzero_states`; `same_seed_yields_same_states`;
  `different_seeds_yield_different_states`.
- `next_u64_known_sequence_for_state_0_1` and `next_u64_known_sequence_for_state_1_2` —
  hand-verified reference sequences for constructed states.
- `next_f64_stays_within_unit_interval` and `next_f64_derives_from_next_u64` (checks the
  `>> 11` / `2^53` normalization).
- `identical_states_reproduce_identical_sequences`;
  `sequences_differ_across_distinct_states`; `sequence_is_not_trivially_constant`.

### 7. Auth (`src/auth.rs`)

`src/tests/auth.rs`.

- `is_logged_in_requires_refresh_token`.
- `authorization_is_not_expired_when_fresh` and
  `access_expiry_depends_on_timestamp_and_duration` (future/past
  `last_access_timestamp`/`expires_in` combinations).
- `authorization_getters_return_constructed_values` and
  `authorization_update_merges_non_default_fields`.
- `from_login_response_parses_firebase_sign_in_json`, `from_refresh_response_parses_token_json`,
  and `from_login_response_rejects_invalid_json`.
- `save_to_disk_writes_expected_refresh_token_json` (writes to `<data>/refresh_token`, cleans
  up afterwards).
- `get_api_key_reads_cached_apikey` reads the cached key from `<data>/apikey` without network.

Network paths — `get_api_key` scraping, `login`, `refresh_from_file`/`refresh_non_blocking`
— stay `#[ignore]` / deferred to httpmock.

### 8. GitHub assets (`src/github.rs`)

`src/tests/github.rs`.

- `github_file_object_deserialize` and `github_dir_object_deserialize`.
- `has_version_changed_true_when_data_dir_empty` (environment-dependent, reliable with an
  empty real data dir).
- `download_test` is `#[ignore]`d as network; `get_tags`/`download_resources_recursive` remain
  network-only.

## WIP — do not test

The typing engine (`src/test.rs`) and the new `src/verify.rs` are **work in progress** and must
**not** be tested yet:

- `Test::register_character` indexes `target_word_list[current_word_list.len() - 1]`
  (`src/test.rs:61`), which panics on the very first character when the list is empty, and the
  target list is never populated at runtime.
- `generate_test_record` is a `todo!()`, and `verify::verify_and_update` (`src/verify.rs`) is a
  `todo!()` stub.
- There is no seed constructor or getter that would let tests observe the engine without
  hitting the panics above.

Do not write tests for the typing engine or verification until the `todo!()`s are implemented
and a seed path (such as `Test::with_target_words(...)`) exists. Desired future coverage:
character append, auto-advance when the current word matches the target, backspace, word-delete,
display string joining, and `reset`. This area may change substantially, so any test written now
would be testing unstable code.

## Callback framework

`change_test_language` (`src/callbacks/change_test_language.rs`) is **implemented**: it resets
the command line, loads the available word lists, builds a subcommand per language, and drives
the selection via `prompt_command`.

A reusable framework for testing command handlers lives in `src/tests/callback_framework.rs`
and covers:

- Constructing a `Command` and invoking its handler via `Command::call(state).await`,
  asserting the returned `Result`.
- Driving the one-shot prompt flow (`oneshot` channels, as in `src/callbacks/login.rs`) from the
  test side via the `drive_prompt_input` helper so handler input can be simulated.
- Injecting a fake `AUTHORIZATION` so commands can be tested without a real session
  (restoring defaults afterwards).

Notification-side-effect assertions are intentionally absent: the notification queue is private
and no test observer exists yet. As other command handlers land, add their tests using this
framework.

## Shared test infrastructure

- No temp-dir override exists, so fixtures that touch `CACHE_DIR` / `DATA_DIR` use unique names
  and always clean up in the test body.
- Globals (`TEST`, `WORD_LIST`, `AUTHORIZATION`, `NOTIFICATIONS`) are shared across tests; where
  unavoidable, document the ordering assumption rather than resetting the globals.
- Async tests use `#[tokio::test]`; network tests use `#[ignore]`.

## Known issues

Present today; may be fixed in future work — flagged for triage, not relied on by tests:

1. `find_fuzzy` full-window tie-handling evicts earlier equal-strength matches —
   `src/command.rs:149`.
2. `CommandLine::register_character` cursor-offset underflow panic —
   `src/commandline.rs:122`.
3. Typing engine unreachable at runtime (`target_word_list` never populated) and full of
   `todo!()`s — `src/test.rs`, `src/verify.rs` (WIP).

## Network / deferred tests

Would require httpmock (or similar) plus URL injection into `src/auth.rs` and `src/github.rs`;
deferred by policy:

- `get_api_key` full scrape pipeline.
- `login` (email + password) and token refresh.
- `get_tags` and `download_resources_recursive` (asset sync).
- `has_version_changed` with a populated data dir.

## Verification

- `cargo test` — unit tests (82 tests: 81 pass, 1 ignored).
- `cargo test -- --ignored` — opt-in network tests.
- `cargo clippy` and `cargo doc --no-deps` — lint and doc integrity.