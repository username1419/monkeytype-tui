# Functionality Test Plan

A plan for exercising the application's behavior through functionality tests. Tests are
described here but **not yet implemented**. The goal is to pin down how the app actually
functions today, before (or alongside) finishing incomplete features.

## Scope & constraints

- The crate is a binary crate (`src/main.rs`), so there is no library target for
  integration tests to import. Tests live as unit tests under `src/tests/`, matching the
  existing pattern.
- **No production code changes**: every described test must work against the current API.
  Behavior that cannot be reached without a refactor is documented as *blocked* rather
  than worked around.
- **Network policy**: functionality that depends on live HTTP (monkeytype auth scraping,
  login/token refresh, GitHub tags/asset sync) is kept `#[ignore]`d or deferred. A future
  httpmock-based harness would require injecting base URLs into `src/auth.rs` and
  `src/github.rs`, which is out of scope.
- **Shared globals**: `TEST`, `WORD_LIST`, `AUTHORIZATION`, `NOTIFICATIONS`, and the
  `CACHE_DIR`/`DATA_DIR` paths are process-wide `static`s. Tests touching them are
  order-dependent and must use unique fixture names plus cleanup (see
  [Shared test infrastructure](#shared-test-infrastructure)).

## Testability matrix

| Module | Functionality | Testable now | Notes |
|--------|---------------|--------------|-------|
| `commandline.rs` | cursor input, delete, word-delete, prompt mode, search/fuzzy submit | Yes | All methods `pub(crate)`; observe input via the submit callback (no input getter) |
| `command.rs` | fuzzy matching (`find_fuzzy`) | Yes | Rewrite the commented-out `src/tests/fuzzy.rs` as `#[tokio::test]` |
| `game.rs` | key event routing | Partial | `TEST`-branch routing not observable (no getter on `Test`); commandline branch is |
| `notify.rs` | builder, expiry, Display formats | Partial | Queue drain/expiry not observable (`NOTIFICATIONS` is private) |
| `typing_test/word_list.rs` | load language, getters, error paths | Yes (caveats) | Fixtures written to the real `CACHE_DIR`; `WORD_LIST` global is shared |
| `auth.rs` | `is_logged_in`, expiry, `save_to_disk` | Yes (non-network) | Network login/refresh/`get_api_key` deferred |
| `github.rs` | deserialization | Yes | tags/download are network (`#[ignore]`) |
| `test.rs` (typing engine) | word advance, targeting | Blocked | `target_word_list` is private and never seeded; `register_character` panics on an empty list |
| `callbacks/*` | command handlers | Partial | `change_test_language` is unfinished; see [Callback framework](#callback-framework) |

## Per-module test descriptions

### 1. Command line (`src/commandline.rs`)

All methods are `pub(crate)`; a `Default` impl exists. Because there is no getter for the
input string, tests observe state through the `submit` callback.

- `enable` / `disable` / `is_enabled` / `is_searching` state transitions; `disable` resets
  the widget (input cleared, back to search mode) unless it is in search mode.
- `toggle_searching` flips search mode.
- Character insertion appends at the cursor (`cursor_offset == 0`); `register_move_left` /
  `register_move_right` shift the cursor and change where the next character lands.
- `register_delete_character` removes the character before the cursor; deleting from an
  empty input disables the command line.
- `register_delete_word` removes the word before the cursor (observe final input and the
  resulting `cursor_offset`).
- `prompt_input` switches to prompt mode (search disabled, enabled, custom prompt) and sets
  a one-shot submit callback.
- `submit` in prompt mode invokes the callback with the input text; in search mode it
  invokes the callback with the selected matched command. Submitting with
  `remain_enabled = false` disables the command line.
- Search mode: after typing a query, `update()` populates `matched_commands` via
  `find_fuzzy` over `ROOT_COMMANDS`; submitting then returns the selected command.

Known issue (do not test, but document): `register_character` at
`src/commandline.rs:117` computes `input.len() - cursor_offset`, which underflows/panics in
debug builds when `cursor_offset > input.len()`.

### 2. Fuzzy matching (`src/command.rs`)

Tests are implemented in `src/tests/fuzzy.rs`. `find_fuzzy` is async and returns a
`Vec<usize>` of indices into the command slice, sorted by match strength descending (ties
keep insertion order). The prompt is lowercased and split into terms; each term is matched
as a prefix of *any* display-name word, and the term's length is summed into the strength.

Coverage:

- Empty / whitespace-only prompts return no matches.
- Case-insensitive per-word prefix matching ("TEST" matches "test mode" and "restart test").
- Multi-term prompts match words in any position and combine strength ("restart test",
  "theme dark").
- Exact display-name matches rank first ("test mode" vs. "test").
- The `options` limit caps the result count.
- Substrings not at a word start never match ("est" does not match "test").
- Trailing whitespace in the prompt is ignored.
- Results are sorted by match strength descending; ties keep insertion order.
- A command whose `display_condition` returns `false` (or `Err`) is excluded.

Known issue (documented in the `options`-limit test, not fixed): once the result window is
full, a new candidate replaces the weakest entry using `>=` (`src/command.rs:148`), so an
equal-strength candidate evicts the earliest match. With tied strengths a full window
therefore keeps the later commands — `find_fuzzy("t", 2)` on the sample set yields `[2, 3]`,
not `[0, 1]`.

### 3. Key event routing (`src/game.rs`)

`event_keypressed` takes an `Arc<Mutex<State>>`; `State` is defined at the crate root, so
its fields are reachable from tests. The `TEST` branch is not observable, so tests focus on
the commandline branch and global side effects:

- `Ctrl+Q` cancels `state.shutdown`.
- `Esc` toggles the command line enabled state.
- A character routes to the command line when enabled (observe via submit) and to `TEST`
  when disabled (currently only assertable as "no panic"; see blocked `test.rs`).
- `Shift+char` routes the uppercased character.
- `Enter` submits the command line when enabled (and disables it after `submit(false)`);
  it is a no-op when disabled.
- `Left`/`Right`/`Up`/`Down` only affect the command line when enabled.
- `Ctrl+Backspace` / `Ctrl+H` route to word deletion.

### 4. Notifications (`src/notify.rs`)

- `NotificationBuilder` defaults (empty title/message, zero duration, `Info` level) and
  fluent setters.
- `build()` sets `expires_at` to `now + duration` (assert within a small tolerance).
- `Display` for `Notification` ("LEVEL: message") and each `NotifLevel` label
  (`INFO`/`SUCCESS`/`WARNING`/`DEBUG`/`ERROR`).
- `debug!` / `error!` / `enotify!` / `wnotify!` / `todo!` fire without panicking;
  `debug!` and `error!` pass the expression value through.

Queue drain and expiry removal are not observable (the `NOTIFICATIONS` static and the
channel receiver are private), so they are intentionally not tested.

### 5. Word lists (`src/typing_test/word_list.rs`)

- `update_and_get_words` parses a fixture and returns its words. It appends the `.json`
  extension to the given name (`src/typing_test/word_list.rs:122`), so test fixtures are
  stored on disk as `<name>.json`.
- Getters reflect the loaded fixture: `get_language`, `get_word_list`, `is_rtl`,
  `is_ligature_aware`, `is_support_lazy_mode`, `is_order_by_freq`, `get_bcp47`.
- `update_and_get_words` on a missing file returns `Err`.
- A malformed fixture returns `Err` (via `enotify!` + `Err`).
- `get_language` / `get_word_list` return `None` before any load — this assertion is only
  reliable when no other test has loaded a list (shared `WORD_LIST` global).

### 6. Auth (`src/auth.rs`)

Extend the existing `src/tests/auth.rs`:

- `is_logged_in` is `true` only when a refresh token is present.
- `get_expire_instant` / `is_access_expired` behave correctly for future and past
  `last_access_timestamp` / `expires_in` combinations.
- `save_to_disk` writes `refresh_token` to `<data>/refresh_token` with the expected JSON
  shape (clean up the file afterwards).
- `get_api_key` reads the cached API key from `<data>/apikey` without network (skipped when
  the cache has never been populated).

Network paths — `get_api_key` scraping, `login`, `refresh_from_file`/`refresh_non_blocking`
— stay `#[ignore]` / deferred to httpmock.

### 7. GitHub assets (`src/github.rs`)

- Deserialization tests already exist and are retained.
- `has_version_changed` returns `Ok(true)` when `<data>` is empty (no network). This is
  environment-dependent and only reliable with an empty real data dir.
- `get_tags` / `download_resources_recursive` remain `#[ignore]` (network).

### 8. Typing engine (`src/test.rs`) — blocked

The engine's core behavior cannot be exercised through the current API:

- Fields are private and `target_word_list` is never populated at runtime.
- `register_character` indexes `target_word_list[current_word_list.len() - 1]`
  (`src/test.rs:36`), which panics on the very first character when the list is empty.

Desired tests (deferred until a seed constructor such as `Test::with_target_words(...)`
exists): character append, auto-advance when the current word matches the target, backspace,
word-delete, display string joining, and `reset`. No production change is allowed under the
current constraint, so these stay documented and unimplemented.

## Callback framework

`change_test_language` (`src/callbacks/change_test_language.rs`) is **not finished** — its
handler is currently a stub that only resets the command line. Do not write tests that
assume behavior that does not exist yet.

Instead, design a reusable framework for testing command handlers (the `Command` /
`CommandCallback` plumbing in `src/command.rs`) so that once commands are implemented they
can be verified uniformly. The framework should cover:

- Constructing a `Command` and invoking its handler via `Command::call(state).await`,
  asserting the returned `Result`.
- Driving the one-shot prompt flow (`oneshot` channels used by
  `src/callbacks/login.rs`) from the test side so handler input can be simulated.
- Injecting fake authorization / state so commands can be tested without real sessions.
- Asserting notification side effects (requires exposing the notification queue or a
  test observer).

This framework should be built in `src/tests/` alongside the first real command tests,
rather than writing one-off handler tests.

## Shared test infrastructure

- No temp-dir override exists (would require a production change), so fixtures that touch
  `CACHE_DIR` / `DATA_DIR` use unique names and always clean up in the test body.
- Globals (`TEST`, `WORD_LIST`, `AUTHORIZATION`, `NOTIFICATIONS`) are shared across tests;
  where unavoidable, document the ordering assumption rather than resetting the globals.
- Async tests use `#[tokio::test]`; network tests use `#[ignore]`.

## Known issues surfaced by this plan

Flagged for triage; not fixed here (no production changes allowed):

1. `find_fuzzy` full-window tie-handling evicts earlier equal-strength matches — `src/command.rs:148`.
2. `CommandLine::register_character` cursor-offset underflow panic — `src/commandline.rs:117`.
3. Typing engine unreachable at runtime (`target_word_list` never populated) —
   `src/test.rs`.

## Network / deferred tests

Would require httpmock (or similar) plus URL injection into `src/auth.rs` and
`src/github.rs`; deferred by policy:

- `get_api_key` full scrape pipeline.
- `login` (email + password) and token refresh.
- `get_tags` and `download_resources_recursive` (asset sync).
- `has_version_changed` with a populated data dir.

## Verification

- `cargo test` — unit tests.
- `cargo test -- --ignored` — opt-in network tests.
- `cargo clippy` and `cargo doc --no-deps` — lint and doc integrity.
