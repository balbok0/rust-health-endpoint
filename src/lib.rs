use anyhow::Result;
use tokio::{io, sync::RwLock};
use tokio::sync::broadcast::error::TryRecvError;

use crate::env::ENV;
use futures::future::join;
use std::{
    sync::Arc,
    task::Poll,
    time::{Duration, SystemTime},
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::broadcast,
    sync::mpsc::{channel, Receiver, Sender},
};

mod env;

pub enum HealthStatus {
    HEALTHY,
    UNHEALTHY(String),
}

struct HealthStatusWithTs {
    status: HealthStatus,
    ts: SystemTime,
}

impl Default for HealthStatusWithTs {
    fn default() -> Self {
        Self { status: HealthStatus::UNHEALTHY("Not Initalized".into()), ts: SystemTime::UNIX_EPOCH }
    }
}

pub type HealthReceiver = Receiver<HealthStatus>;
pub type HealthSender = Sender<HealthStatus>;

async fn update_health_loop(
    health_sub: &mut HealthReceiver,
    health_monitor: Arc<RwLock<HealthStatusWithTs>>,
    signal_receiver: &mut broadcast::Receiver<()>,
) {
    let wait_duration = tokio::time::Duration::from_millis(ENV.health_check_interval_millis);
    let mut wait_interval = tokio::time::interval(wait_duration);

    let waker = futures::task::noop_waker();
    let mut cx = std::task::Context::from_waker(&waker);

    loop {
        match health_sub.poll_recv(&mut cx) {
            Poll::Ready(Some(status)) => {
                let status_with_ts = HealthStatusWithTs {
                    status,
                    ts: SystemTime::now()
                };
                *health_monitor.write().await = status_with_ts;
            }
            Poll::Ready(None) => {
                log::error!("Health Receiver Pool failed. Exiting...");
                break;
            }
            Poll::Pending => {
                wait_interval.tick().await;
            }
        };

        match signal_receiver.try_recv() {
            Ok(()) => {
                log::error!("Health Check received kill signal. Sleeping for 2 seconds and then closing");
                tokio::time::sleep(Duration::from_secs(2)).await;
                break;
            }
            Err(TryRecvError::Lagged(idx)) => {
                log::error!(
                    "Signal receiver in health check is lagged by {} messages. Something failed. Closing.",
                    idx
                );
                break;
            }
            Err(TryRecvError::Closed) => {
                log::error!("Signal receiver closed. Something failed. Returning.");
                break;
            }
            Err(TryRecvError::Empty) => {}
        }
    }
}

async fn process_stream(
    stream: &mut TcpStream,
    health_monitor: Arc<RwLock<HealthStatusWithTs>>,
    time_to_unhealthy: Duration,
) -> Result<()> {
    let mut data = vec![];

    loop {
        match stream.try_read(&mut data) {
            Ok(_) => break,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    let monitor = health_monitor.read().await;
    let message: &[u8] = match monitor.status {
        HealthStatus::UNHEALTHY(_) => b"HTTP/1.1 500 Internal Server Error\r\n\r\n",
        HealthStatus::HEALTHY => {
            if monitor.ts.elapsed()? > time_to_unhealthy {
                b"HTTP/1.1 500 Internal Server Error\r\n\r\n"
            } else {
                b"HTTP/1.1 200 OK\r\n\r\n"
            }
        }
    };

    stream.write_all(message).await?;
    stream.flush().await?;

    stream.shutdown().await?;
    Ok(())
}

async fn run_server(
    health_monitor: Arc<RwLock<HealthStatusWithTs>>,
    time_to_unhealthy: Duration,
    signal_receiver: &mut broadcast::Receiver<()>,
) {
    let addr = format!("127.0.0.1:{}", ENV.health_port);
    let listener = match TcpListener::bind(addr).await {
        Ok(x) => x,
        Err(x) => {
            log::error!("Could not establish health endpoint. Error {}", x);
            return;
        }
    };

    let mut wait_interval = tokio::time::interval(Duration::from_millis(500));
    let waker = futures::task::noop_waker();
    let mut cx = std::task::Context::from_waker(&waker);

    loop {
        match listener.poll_accept(&mut cx) {
            Poll::Ready(Ok((mut stream, _))) => {
                if let Err(x) = process_stream(&mut stream, health_monitor.clone(), time_to_unhealthy).await {
                    log::error!("Stream exited with error. {}", x);
                }
            }
            Poll::Ready(Err(x)) => {
                log::error!("Health Receiver Pool got error {}. Exiting...", x);
                break;
            }
            Poll::Pending => {
                wait_interval.tick().await;
            }
        }

        if signal_receiver_check(signal_receiver) {
            break;
        }
    }
}

pub struct HealthMonitor {
    time_to_unhealthy: Duration,
    _health_sub: HealthReceiver,
    _health_pub: HealthSender,
    _health_monitor: Arc<RwLock<HealthStatusWithTs>>,
}

impl HealthMonitor {
    pub fn new(time_to_unhealthy: Duration) -> HealthMonitor {
        let (sender, consumer): (HealthSender, HealthReceiver) = channel(40);
        HealthMonitor {
            time_to_unhealthy,
            _health_sub: consumer,
            _health_pub: sender,
            _health_monitor: Arc::new(RwLock::new(HealthStatusWithTs::default())),
        }
    }

    #[allow(dead_code)]
    pub fn get_publisher(&self) -> HealthSender {
        self._health_pub.clone()
    }

    pub async fn run(&mut self, signal_receiver: &mut broadcast::Receiver<()>) {
        join(
            Box::pin(run_server(
                self._health_monitor.clone(),
                self.time_to_unhealthy,
                &mut signal_receiver.resubscribe(),
            )),
            Box::pin(update_health_loop(
                &mut self._health_sub,
                self._health_monitor.clone(),
                signal_receiver,
            )),
        )
        .await;
    }
}

pub fn signal_receiver_check(signal_receiver: &mut tokio::sync::broadcast::Receiver<()>) -> bool {
    match signal_receiver.try_recv() {
        Ok(()) => {
            log::error!("Health Check received kill signal. Sleeping for few seconds and then closing");
            true
        }
        Err(TryRecvError::Lagged(idx)) => {
            log::error!(
                "Signal receiver in health check is lagged by {} messages. Something failed. Closing.",
                idx
            );
            true
        }
        Err(TryRecvError::Closed) => {
            log::error!("Signal receiver closed. Something failed. Returning.");
            true
        }
        Err(TryRecvError::Empty) => false,
    }
}