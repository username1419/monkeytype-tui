pub mod auth;
pub(crate) mod callbacks;
pub mod command;
pub mod commandline;
pub mod game;
pub(crate) mod github;
pub mod notify;
pub mod settings;
pub(crate) mod test;
pub(crate) mod tests;
pub mod traits;
pub(crate) mod typing_test;
pub(crate) mod verify;

use std::{
    env,
    error::Error,
    fs::{create_dir_all, read_to_string, remove_dir_all, write},
    hint::spin_loop,
    io::{self, Read, Write, stdin},
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32},
    },
    time::Duration,
};

use crossterm::event::{self, EventStream};
use once_cell::sync::Lazy;
use ratatui::{
    layout::{Alignment, Rect, Size},
    style::Stylize,
    text::{Text, ToText},
    widgets::{Block, Paragraph, Wrap},
};
use tokio::{
    join,
    runtime::{Handle, Runtime},
    spawn,
    sync::Mutex,
    task::{JoinHandle, spawn_blocking},
    time::{Instant, sleep},
};
use tokio_stream::StreamExt;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    auth::{AUTHORIZATION, Authorization, CLIENT, refresh_from_file},
    github::{download_resources_recursive, get_tags, has_version_changed},
    notify::{NotificationManager, debug, enotify, error, notify},
    test::TEST,
    traits::UpdateableWidget,
};
use crate::{commandline::CommandLine, notify::QuickNotify};

/// User-initiated actions dispatched to the game loop.
#[derive(Default, Debug)]
enum Action {
    Type(char),
    ShowMenu,
    SelectMenu,
    MenuDown,
    MenuUp,
    #[default]
    Idle,
}

/// Central application state shared across async tasks via [`Arc<Mutex<State>>`].
///
/// Created once in `main` and cloned into each background task (display, key input, and the
/// update loop). Guards access to the command line, the shutdown token, and the current
/// terminal size; the active typing test lives in the separate [`TEST`] global.
#[derive(Default, Debug)]
struct State {
    action: Action,
    commandline: CommandLine,
    shutdown: CancellationToken,
    terminal_size: Size,
    is_online: bool,
}

/// NOTE: 60fps
const UPDATE_RATE: Duration = Duration::from_micros(16666);
/// NOTE: 120fps
const DISPLAY_RATE: Duration = Duration::from_micros(8333);
/// NOTE: 1000fps
const KEY_UPDATE_RATE: Duration = Duration::from_millis(1);

/// Application name used as the subdirectory for config, data, and cache paths.
const APP_NAME: &str = "monkeytype-tui";

/// Platform-specific configuration directory (`~/.config/monkeytype-tui`).
pub(crate) static CONFIG_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let p = dirs::config_dir()
        .expect("App configuration directory not found")
        .join(APP_NAME);
    create_dir_all(&p).expect("Configuration directory creation failed");
    p
});

/// Platform-specific data directory (`~/.local/share/monkeytype-tui`).
pub(crate) static DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let p = dirs::data_dir()
        .expect("App data directory not found")
        .join(APP_NAME);
    create_dir_all(&p).expect("Data directory creation failed");
    p
});

/// Platform-specific cache directory (`~/.cache/monkeytype-tui`).
pub(crate) static CACHE_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let p = dirs::cache_dir()
        .expect("App cache directory not found")
        .join(APP_NAME);
    create_dir_all(&p).expect("Cache directory creation failed");
    p
});

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut state = State::default();
    let mut authorization = Authorization::default();

    spawn(async {
        match has_version_changed().await {
            Ok(b) => {
                if !b {
                    return;
                }
            }
            Err(e) => {
                enotify!(format!(
                    "Error encountered while attempting to check game asset version: {}",
                    e
                ));
                enotify!("Continuing with refresh...");
            }
        }

        notify().info("Current game assets version does not match. Updating...");

        let version = match get_tags(&CLIENT).await {
            Ok(mut tags) => tags.swap_remove(0),
            Err(e) => {
                enotify!(format!(
                    "Error encountered while attempting to refresh game assets: {}",
                    e
                ));
                return;
            }
        };
        match download_resources_recursive(&CLIENT, version).await {
            Ok(skipped) => notify().success(&format!(
                "Game assets updated. {} files skipped due to errors.",
                skipped
            )),
            Err(e) => {
                error!(e);
            }
        };
    });

    // NOTE: ideally we wait for this process to complete

    if let Ok(a) = refresh_from_file().await {
        authorization = a;
    }

    #[cfg(feature = "profiling")]
    console_subscriber::init();

    #[cfg(debug_assertions)]
    {
        read_to_string("../.env")
            .inspect(|contents| {
                let mut api_key = String::default();
                let mut display_name = String::default();
                let mut access_token = String::default();
                let mut expires_in = 0;
                let mut token_type = String::default();
                let mut refresh_token = String::default();
                let mut user_id = String::default();
                let mut project_id = String::default();
                let envs = contents.trim().split("\n");
                for env in envs {
                    let kv = env.split("=").collect::<Vec<_>>();
                    match kv[0] {
                        "MONKEYTYPE_API_KEY" => api_key = kv[1].to_string(),
                        "MONKEYTYPE_DISPLAY_NAME" => display_name = kv[1].to_string(),
                        "MONKEYTYPE_ACCESS_TOKEN" => access_token = kv[1].to_string(),
                        "MONKEYTYPE_EXPIRES_IN" => {
                            expires_in = kv[1]
                                .parse()
                                .expect("MONKEYTYPE_EXPIRES_IN environment param not a number")
                        }
                        "MONKEYTYPE_TOKEN_TYPE" => token_type = kv[1].to_string(),
                        "MONKEYTYPE_REFRESH_TOKEN" => refresh_token = kv[1].to_string(),
                        "MONKEYTYPE_USER_ID" => user_id = kv[1].to_string(),
                        "MONKEYTYPE_PROJECT_ID" => project_id = kv[1].to_string(),
                        s => eprintln!("Unrecognized .env variable: {}", s),
                    }
                }

                authorization = Authorization::new(
                    api_key,
                    display_name,
                    access_token,
                    Some(Instant::now()),
                    expires_in,
                    token_type,
                    refresh_token,
                    user_id,
                    project_id,
                );
            })
            .ok();
    }

    *AUTHORIZATION.lock().unwrap() = authorization;
    let cancellation_token = state.shutdown.clone();
    let state = Arc::new(Mutex::new(state));
    let tracker = TaskTracker::new();

    let _s = state.clone();
    let _t = tracker.clone();
    let __s = state.clone();
    let __t = tracker.clone();
    let ___t = tracker.clone();

    let key_update_handle = key_update(state, _t);
    let _display_handle = display(_s, ___t);
    let _update_handle = update(__s, __t);

    cancellation_token.cancelled().await;
    key_update_handle.abort();

    tracker.close();
    tracker.wait().await;
    ratatui::restore();

    if cfg!(debug_assertions) {
        print!("(Dev) Remove app specific directories? [Y/n]: ");
        let _ = io::stdout().flush();
        let mut buf = [0_u8; 1];
        if stdin().lock().read_exact(&mut buf).is_ok() {
            match buf[0] {
                0x0A | 0x59 => {
                    remove_dir_all(DATA_DIR.as_path()).ok();
                    println!("\nRemoved data directory");
                    remove_dir_all(CONFIG_DIR.as_path()).ok();
                    println!("Removed configuration directory");
                    remove_dir_all(CACHE_DIR.as_path()).ok();
                    println!("Removed cache directory");
                }
                _ => {
                    println!("\naight")
                }
            }
        }
    }

    Ok(())
}

const SHOW_FPS: bool = true;

/// Renders the terminal UI at [`DISPLAY_RATE`] (120 fps).
///
/// Draws the command line, typing test, and notification overlays on each frame. This is the
/// only task that writes to the terminal; it also records the current terminal size into
/// `state` and breaks out of its loop once shutdown has been requested (after the update
/// loop stops the game).
fn display(
    state: Arc<Mutex<State>>,
    tracker: TaskTracker,
) -> JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
    tracker.spawn(async move {
        let mut terminal = ratatui::init();
        let mut past_fps = [0_f32; 20];
        let mut avg_fps = 0_f32;

        loop {
            let last = Instant::now();
            let Ok(mut state) = state.try_lock() else {
                continue;
            };
            terminal.draw(|frame| {
                let area = frame.area();
                state.terminal_size = area.as_size();
                let width = area.width;
                let height = area.height;

                frame.render_widget(Block::new().on_dark_gray(), area);

                if state.shutdown.is_cancelled() {
                    let mut rect = frame.area();
                    rect = rect.resize(Size::new(40, 4));
                    rect.x = frame.area().width / 2 - rect.width / 2;
                    rect.y = frame.area().height / 2 - rect.height / 2;
                    frame.render_widget(
                        Paragraph::new("Waiting for background tasks to finish...")
                            .wrap(Wrap { trim: true })
                            .block(
                                Block::bordered()
                                    .title("Exit")
                                    .title_alignment(Alignment::Center),
                            )
                            .centered(),
                        rect,
                    );
                }

                state.commandline.render(frame, width, height);
                if let Ok(t) = TEST.try_lock() {
                    t.render(frame, width, height);
                }
                if SHOW_FPS {
                    frame.render_widget(avg_fps.to_text(), Rect::new(1, 1, 10, 1));
                }
                NotificationManager::try_render(frame, width, height);
            })?;

            if state.shutdown.is_cancelled() {
                break;
            }

            drop(state);

            let now = Instant::now();
            let mut delta = now.saturating_duration_since(last);
            if delta < DISPLAY_RATE {
                wait_for(DISPLAY_RATE - delta).await;
                let now = Instant::now();
                delta = now.saturating_duration_since(last);
            }
            let current_fps = (1.0 / delta.as_secs_f32()) * 100.0;
            past_fps[19] = current_fps;
            past_fps.rotate_left(1);
            avg_fps = (past_fps.iter().sum::<f32>() / 20.0 / 100.0).trunc();
        }

        Ok(())
    })
}

/// Reads terminal key events at [`KEY_UPDATE_RATE`] (1000 fps) and dispatches
/// them to [`game::event_keypressed`].
///
/// Converts raw crossterm key events into [`game::event_keypressed`] calls, each spawned as
/// its own task so input stays responsive. This task is the sole source of user-initiated
/// actions feeding the command line and typing test.
fn key_update(
    state: Arc<Mutex<State>>,
    tracker: TaskTracker,
) -> JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
    let _t = tracker.clone();
    _t.spawn(async move {
        let mut reader = EventStream::new();
        loop {
            let now = Instant::now();
            let Some(Ok(event)) = reader.next().await else {
                break Ok(());
            };

            match event {
                event::Event::FocusGained => {}
                event::Event::FocusLost => {}
                event::Event::Key(key_event) => {
                    let state = state.clone();
                    // NOTE: not too sure if deepcloning state or using a mutex would be more
                    // performant, may need more testing
                    tracker.spawn(async move {
                        game::event_keypressed(key_event, state).await.ok();
                    });
                }
                event::Event::Mouse(_mouse_event) => {}
                event::Event::Paste(_) => todo!(),
                event::Event::Resize(_, _) => todo!(),
            }

            let delta = Instant::now() - now;
            if delta < KEY_UPDATE_RATE {
                wait_for(KEY_UPDATE_RATE - delta).await;
            }
        }
    })
}

/// Periodic update loop running at [`UPDATE_RATE`] (60 fps).
///
/// Drives notification expiry, command line fuzzy search, and automatic
/// token refresh when the access token is about to expire.
fn update(
    state: Arc<Mutex<State>>,
    _tracker: TaskTracker,
) -> JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
    let _t = _tracker.clone();
    _t.spawn(async move {
        let is_refreshing = Arc::new(AtomicBool::new(false));
        loop {
            let now = Instant::now();

            {
                if let Ok(mut _state) = state.try_lock() {
                    if _state.shutdown.is_cancelled() {
                        break Ok(());
                    }

                    let (_, _) = join!(NotificationManager::update(), _state.commandline.update(),);

                    // TODO: debounce time for token refresh
                    if let Ok(authorization) = AUTHORIZATION.lock()
                        && authorization.is_access_expired()
                        && !authorization.get_refresh_token().is_empty()
                        && !is_refreshing.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        is_refreshing.store(true, std::sync::atomic::Ordering::Relaxed);
                        let i = is_refreshing.clone();
                        spawn(async move {
                            let _ = Authorization::refresh_non_blocking()
                                .await
                                .inspect_err(|e| {
                                    enotify!(format!("Error while refreshing access token: {}", e))
                                });
                            i.store(false, std::sync::atomic::Ordering::Relaxed);
                        });
                    }
                } else {
                    continue;
                };
            }

            let delta = Instant::now() - now;
            if delta < UPDATE_RATE {
                wait_for(UPDATE_RATE - delta).await;
            }
        }
    })
}

const MS_OFFSET: Duration = Duration::from_millis(2);
async fn wait_for(duration: Duration) {
    let start = Instant::now();
    sleep(duration.saturating_sub(MS_OFFSET)).await;
    while start.elapsed() < duration {
        spin_loop();
    }
}
