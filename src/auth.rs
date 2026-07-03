use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use log::{debug, error};
use pam_client::{Context, ErrorCode, Flag};
use smithay_client_toolkit::reexports::{calloop::channel, calloop::EventLoop};
use whoami::username;
use zeroize::Zeroizing;

type AuthChannels = (
    channel::Sender<(Zeroizing<String>, u64)>,
    channel::Channel<(bool, u64)>,
    channel::Channel<String>,
);
pub struct LockConversation {
    pub password: Option<Zeroizing<String>>,
    // Forwards PAM info/error text (e.g. the faillock lockout notice) to the UI
    // thread. Upstream rustlock (like swaylock) drops these, so a lockout fails
    // silently; we surface them so it's spelled out on the lock screen.
    pub msg_send: channel::Sender<String>,
    // Set when PAM sent any conversation message during the current attempt, so
    // the auth loop knows whether it still needs to synthesize one from the
    // authenticate() error code (e.g. MAXTRIES -> account locked).
    pub had_message: bool,
}

impl pam_client::ConversationHandler for LockConversation {
    fn init(&mut self, _default_user: Option<impl AsRef<str>>) {}

    fn prompt_echo_on(&mut self, _msg: &CStr) -> Result<CString, ErrorCode> {
        Err(ErrorCode::ABORT)
    }

    fn prompt_echo_off(&mut self, _msg: &CStr) -> Result<CString, ErrorCode> {
        if let Some(password) = self.password.take() {
            CString::new(password.as_str()).map_err(|_| ErrorCode::ABORT)
        } else {
            Err(ErrorCode::ABORT)
        }
    }

    fn text_info(&mut self, msg: &CStr) {
        self.had_message = true;
        let _ = self.msg_send.send(msg.to_string_lossy().into_owned());
    }
    fn error_msg(&mut self, msg: &CStr) {
        self.had_message = true;
        let _ = self.msg_send.send(msg.to_string_lossy().into_owned());
    }
    fn radio_prompt(&mut self, _msg: &CStr) -> Result<bool, ErrorCode> {
        Ok(false)
    }
}

pub fn create_and_run_auth_loop(service_name: String) -> Option<AuthChannels> {
    let username = username();

    let (auth_req_send, auth_req_recv) = channel::channel::<(Zeroizing<String>, u64)>();
    let (auth_res_send, auth_res_recv) = channel::channel::<(bool, u64)>();
    // PAM info/error text (e.g. faillock lockout notice) → UI thread.
    let (auth_msg_send, auth_msg_recv) = channel::channel::<String>();

    thread::spawn(move || {
        let mut event_loop: EventLoop<()> = EventLoop::try_new().unwrap();

        // Create PAM context once and reuse it for all auth attempts.
        // Creating a new context each time is expensive because it
        // re-parses configs and re-loads shared libraries for every attempt.
        let conversation = LockConversation {
            password: None,
            msg_send: auth_msg_send.clone(),
            had_message: false,
        };
        let mut context =
            match Context::new(service_name.as_str(), Some(username.as_str()), conversation) {
                Ok(ctx) => {
                    debug!("Prepared to authenticate user '{}'", username);
                    ctx
                }
                Err(err) => {
                    error!("Failed to initialize PAM context: {:?}", err);
                    error!(
                        "Ensure that the PAM service '{}' is correctly configured.",
                        service_name
                    );
                    return;
                }
            };

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        event_loop
            .handle()
            .insert_source(auth_req_recv, move |evt, _metadata, _state| match evt {
                channel::Event::Msg((password, seq)) => {
                    {
                        let conv = context.conversation_mut();
                        conv.password = Some(password);
                        conv.had_message = false;
                    }
                    match context.authenticate(Flag::NONE) {
                        Ok(()) => {
                            let _ = auth_res_send.send((true, seq));
                        }
                        Err(err) => {
                            error!("Pam authenticate failed with {:?}", err);
                            // pam_unix is silent on a wrong password and faillock
                            // only sends its own message intermittently, but a
                            // lockout reliably returns MAXTRIES. If nothing was
                            // sent this attempt, synthesize a clear lockout notice.
                            if !context.conversation_mut().had_message
                                && matches!(err.code(), ErrorCode::MAXTRIES)
                            {
                                let _ = auth_msg_send.send(
                                    "Account locked — too many failed attempts".to_string(),
                                );
                            }
                            let _ = auth_res_send.send((false, seq));
                        }
                    }
                }
                channel::Event::Closed => {
                    running_clone.store(false, Ordering::SeqCst);
                }
            })
            .unwrap();

        while running.load(Ordering::SeqCst) {
            let _ = event_loop.dispatch(Some(Duration::from_millis(100)), &mut ());
        }

        debug!("PAM auth thread exiting cleanly");
    });

    Some((auth_req_send, auth_res_recv, auth_msg_recv))
}
