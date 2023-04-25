use tokio::signal::unix::SignalKind;
use tokio::sync::broadcast::{Sender, Receiver};
use anyhow::Result;

async fn signal_handler_safe(publisher: Sender<()>) -> Result<()> {
    let mut signal0 = tokio::signal::unix::signal(SignalKind::terminate())?;
    let mut signal1 = tokio::signal::unix::signal(SignalKind::interrupt())?;
    let mut signal2 = tokio::signal::unix::signal(SignalKind::quit())?;

    tokio::select! {
        _ = signal0.recv() => {
            // publisher.send(()).await?
            publisher.send(())?;
        }
        _ = signal1.recv() => {
            // publisher.send(()).await?
            publisher.send(())?;
        }
        _ = signal2.recv() => {
            // publisher.send(())?
            publisher.send(())?;
        }
    }

    Ok(())
}

pub async fn signal_handler(publisher: Sender<()>) {
    if let Err(x) = signal_handler_safe(publisher).await {
        log::error!("Error during signal handling {}", x);
    }
}


pub fn get_signal_channel() -> (Sender<()>, Receiver<()>) {
    tokio::sync::broadcast::channel::<()>(10)
}