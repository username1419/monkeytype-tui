use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};

use crate::{
    State,
    auth::{AUTHORIZATION, login},
    command::{ClonedCommand, Command},
    notify::{QuickNotify, error, notify},
};

/// Creates the "Login with email and password" command.
pub(crate) fn create() -> Command {
    Command::new(
        "login".into(),
        "Login with email and password".into(),
        crate::command::CommandGroup::Other,
        || async move {
            let authorization = AUTHORIZATION.lock().expect("AUTHORIZATION is poisoned");
            Ok(authorization.is_logged_in())
        },
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
                    return Err(error!(err_msg));
                }
            };

            let mut auth = match AUTHORIZATION.lock() {
                Ok(auth) => auth,
                Err(e) => return Err(error!(e).to_string()),
            };
            auth.update(authorization);

            notify().success(&format!(
                "Login succeeded as {}",
                auth.get_display_name().clone()
            ));

            auth.save_to_disk();

            Ok(())
        },
    )
}

/// Opens the command line in prompt mode to collect the user's password.
/// TODO: hide this
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
        return Err(error!(err_msg));
    };
    if password.is_empty() {
        return Err("password is empty".into());
    }
    Ok(password)
}

/// Opens the command line in prompt mode to collect the user's email address.
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
        return Err(error!(err_msg));
    };
    if email.is_empty() {
        return Err("email is empty".into());
    }
    Ok(email)
}
