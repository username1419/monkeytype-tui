use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};

use crate::{
    State,
    auth::login,
    command::{ClonedCommand, Command},
    notify::{NOTIFICATIONS, QuickNotify},
};

pub(crate) fn create() -> Command {
    Command::new(
        "login".into(),
        "Login with email and password".into(),
        crate::command::CommandGroup::Other,
        Vec::default(),
        None,
        |state: Arc<Mutex<State>>| async move {
            let Ok(email) = prompt_user_email(&state).await else {
                return Err("error occurred while login input".into());
            };

            let Ok(password) = prompt_user_password(&state).await else {
                return Err("error occurred while login input".into());
            };

            let authorization = match login(email, password).await {
                Ok(a) => a,
                Err(e) => {
                    let err_msg = format!("Error while attempting to retrieve auth token: {:?}", e);
                    return Err(NOTIFICATIONS
                        .lock()
                        .expect("NOTIFICATIONS is poisoned")
                        .error(err_msg));
                }
            };

            if let Some(auth) = &mut state.lock().await.authentication_state {
                auth.update(authorization);
            } else {
                state.lock().await.authentication_state = Some(authorization);
            }

            let a = &state.lock().await.authentication_state;
            NOTIFICATIONS
                .lock()
                .expect("NOTIFICATIONS is poisoned")
                .success(&format!(
                    "Login succeeded as {}",
                    a.as_ref().unwrap().get_display_name().clone()
                ));

            a.as_ref().expect("Something bad happened").save_to_disk();

            Ok(String::default())
        },
    )
}

async fn prompt_user_password(state: &Arc<Mutex<State>>) -> Result<String, String> {
    let (send, recv) = oneshot::channel::<(String, Option<ClonedCommand>)>();
    state
        .lock()
        .await
        .commandline
        .prompt_input("Enter password...".into(), |input, command| {
            send.send((input, command)).ok();
        });
    let Ok((password, _)) = recv.await else {
        let err_msg = format!("recv.await returns Error: {}", line!());
        return Err(NOTIFICATIONS
            .lock()
            .expect("NOTIFICATIONS is poisoned")
            .error(err_msg));
    };
    if password.is_empty() {
        return Err("password is empty".into());
    }
    Ok(password)
}

async fn prompt_user_email(state: &Arc<Mutex<State>>) -> Result<String, String> {
    let (send, recv) = oneshot::channel::<(String, Option<ClonedCommand>)>();
    state
        .lock()
        .await
        .commandline
        .prompt_input("Enter email...".into(), |input, command| {
            send.send((input, command)).ok();
        });
    let Ok((email, _)) = recv.await else {
        let err_msg = format!("recv.await returns Error: {}", line!());
        return Err(NOTIFICATIONS
            .lock()
            .expect("NOTIFICATIONS is poisoned")
            .error(err_msg));
    };
    if email.is_empty() {
        return Err("email is empty".into());
    }
    Ok(email)
}
