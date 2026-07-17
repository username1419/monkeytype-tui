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

use std::{
    env,
    error::Error,
    fs::{create_dir_all, read_to_string, remove_dir_all, write},
    io::{Read, stdin},
    path::PathBuf,
    str::FromStr,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crossterm::event::{self, EventStream};
use once_cell::sync::Lazy;
use ratatui::{
    layout::{Alignment, Rect, Size},
    style::Stylize,
    widgets::{Block, Paragraph, Wrap},
};
use tokio::{
    join, spawn,
    sync::Mutex,
    time::{Instant, sleep},
};
use tokio_stream::StreamExt;
use tokio_util::task::TaskTracker;

use crate::{
    auth::{AUTHORIZATION, Authorization, refresh_from_file},
    notify::{NotificationManager, debug, enotify, notify},
    test::TEST,
    traits::UpdateableWidget,
};
use crate::{commandline::CommandLine, notify::QuickNotify};

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

#[derive(Default, Debug)]
struct State {
    action: Action,
    commandline: CommandLine,
    shutdown: bool,
    terminal_size: Size,
}

/// NOTE: 60fps
const UPDATE_RATE: Duration = Duration::from_micros(16666);
/// NOTE: 120fps
const DISPLAY_RATE: Duration = Duration::from_micros(8333);
/// NOTE: 1000fps
const KEY_UPDATE_RATE: Duration = Duration::from_millis(1);

const APP_NAME: &str = "monkeytype-tui";

pub(crate) static CONFIG_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let p = dirs::config_dir()
        .expect("App configuration directory not found")
        .join(APP_NAME);
    create_dir_all(&p).expect("Configuration directory creation failed");
    p
});

pub(crate) static DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let p = dirs::data_dir()
        .expect("App data directory not found")
        .join(APP_NAME);
    create_dir_all(&p).expect("Data directory creation failed");
    p
});

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
    let state = Arc::new(Mutex::new(state));
    let tracker = TaskTracker::new();

    let _s = state.clone();
    let _t = tracker.clone();
    let __s = state.clone();
    let __t = tracker.clone();

    // NOTE: me when i commit a warcrime:
    tokio::select! {
        _ = key_update(state, _t) => {},
        _ = display(_s) => {},
        _ = update(__s, __t) => {}
    }

    tracker.close();
    tracker.wait().await;
    ratatui::restore();

    if cfg!(debug_assertions) {
        print!("(Dev) Remove app specific directories? [Y/n]: ");
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

async fn display(state: Arc<Mutex<State>>) -> Result<(), Box<dyn Error>> {
    let mut terminal = ratatui::init();

    loop {
        let now = Instant::now();
        let mut state = state.lock().await;
        terminal.draw(|frame| {
            let area = frame.area();
            state.terminal_size = area.as_size();
            let width = area.width;
            let height = area.height;

            frame.render_widget(
                Block::bordered().title_top("sup").blue(),
                Rect::new(0, 0, 250, 250),
            );

            if state.shutdown {
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
            NotificationManager::try_render(frame, width, height);
        })?;

        if state.shutdown {
            break;
        }

        drop(state);

        let delta = Instant::now() - now;
        if delta < DISPLAY_RATE {
            sleep(DISPLAY_RATE - delta).await;
        }
    }

    Ok(())
}

async fn key_update(state: Arc<Mutex<State>>, tracker: TaskTracker) -> Result<(), Box<dyn Error>> {
    let mut reader = EventStream::new();
    loop {
        let now = Instant::now();
        let Some(Ok(event)) = reader.next().await else {
            let delta = Instant::now() - now;
            if delta < KEY_UPDATE_RATE {
                sleep(KEY_UPDATE_RATE - delta).await;
            }
            continue;
        };

        match event {
            event::Event::FocusGained => {}
            event::Event::FocusLost => {}
            event::Event::Key(key_event) => {
                let state = state.clone();
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
            sleep(KEY_UPDATE_RATE - delta).await;
        }
    }
}

async fn update(state: Arc<Mutex<State>>, _tracker: TaskTracker) -> Result<(), Box<dyn Error>> {
    let is_refreshing = Arc::new(AtomicBool::new(false));
    loop {
        let now = Instant::now();

        {
            let mut _state = state.lock().await;

            let (_, _) = join!(NotificationManager::update(), _state.commandline.update(),);

            // TODO: debounce time for token refresh
            if let Ok(authorization) = AUTHORIZATION.lock()
                && authorization.get_expire_instant() - now < Duration::from_mins(5)
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
        }

        let delta = Instant::now() - now;
        if delta < UPDATE_RATE {
            sleep(UPDATE_RATE - delta).await;
        }
    }
}
