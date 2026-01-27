use crate::logging::error_msg;
use tokio::{
    signal::unix::{SignalKind, signal},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum SignalEvent {
    Suspend,
    Resume,
}

pub fn spawn_signal_handler(
    cancel_token: CancellationToken,
) -> mpsc::UnboundedReceiver<SignalEvent> {
    let (tx, rx) = mpsc::unbounded_channel::<SignalEvent>();

    tokio::spawn(async move {
        macro_rules! register_signal {
            ($kind:expr) => {
                match signal($kind) {
                    Ok(s) => s,
                    Err(e) => {
                        error_msg!("Failed to register signal handler: {e}");
                        return;
                    }
                }
            };
        }

        let mut sigint = register_signal!(SignalKind::interrupt());
        let mut sigterm = register_signal!(SignalKind::terminate());
        let mut sighup = register_signal!(SignalKind::hangup());
        let mut sigtstp = register_signal!(SignalKind::from_raw(libc::SIGTSTP));
        let mut sigcont = register_signal!(SignalKind::from_raw(libc::SIGCONT));
        let mut sigquit = register_signal!(SignalKind::quit());

        loop {
            tokio::select! {
                _ = sigint.recv() => {
                    cancel_token.cancel();
                    break;
                }
                _ = sigterm.recv() => {
                    cancel_token.cancel();
                    break;
                }
                _ = sighup.recv() => {
                    cancel_token.cancel();
                    break;
                }
                _ = sigquit.recv() => {
                    cancel_token.cancel();
                    break;
                }
                _ = sigtstp.recv() => {
                    let _ = tx.send(SignalEvent::Suspend);
                }
                _ = sigcont.recv() => {
                    let _ = tx.send(SignalEvent::Resume);
                }
            }
        }
    });

    rx
}
