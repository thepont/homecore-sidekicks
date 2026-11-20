//! A controller's WebSocket lifetime is independent of the MQTT publisher.
use crate::{
    config::Unit,
    control::{ControllerWrite, Controls, ReceivedCommand},
    model::{self, Frame, Snapshot},
};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    time::{self, Instant},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message},
};

#[derive(Clone, Debug)]
pub(crate) struct Observation {
    pub state: Arc<Snapshot>,
    pub info: Option<Arc<Snapshot>>,
    pub received: Instant,
    pub control_echo: bool,
}

pub(crate) async fn run(
    unit: Unit,
    stale_after: Duration,
    output: watch::Sender<Option<Observation>>,
    mut commands: mpsc::Receiver<ReceivedCommand>,
    broker_ready: watch::Receiver<bool>,
) {
    let mut retry = 1;
    loop {
        let limits = WebSocketConfig::default()
            .read_buffer_size(8192)
            .write_buffer_size(0)
            .max_write_buffer_size(1024)
            .max_message_size(Some(model::MAX_FRAME_BYTES))
            .max_frame_size(Some(model::MAX_FRAME_BYTES));
        let connected = time::timeout(
            Duration::from_secs(5),
            connect_async_with_config(unit.url.as_str(), Some(limits), true),
        )
        .await;
        if let Ok(Ok((mut socket, _))) = connected {
            let session_started = Instant::now();
            let mut pending: Option<(ControllerWrite, Instant)> = None;
            let mut metadata = None;
            let mut latest: Option<Observation> = None;
            let mut deadline = Instant::now() + stale_after;
            loop {
                let message = tokio::select! {
                    _ = time::sleep_until(deadline) => break,
                    Some(request) = commands.recv() => {
                        if pending.as_ref().is_some_and(|(_, until)| Instant::now() >= *until) {
                            pending = None;
                        }
                        if !*broker_ready.borrow() || request.received < session_started
                            || request.received.elapsed() >= Duration::from_secs(1)
                            || pending.is_some()
                        {
                            continue;
                        }
                        let Some(write) = latest.as_ref()
                            .filter(|last| last.received.elapsed() < stale_after)
                            .and_then(|last| Controls::from_snapshot(&last.state))
                            .and_then(|controls| controls.frame(request.command))
                        else {
                            continue;
                        };
                        let message = Message::Text(write.frame.to_string().into());
                        if !matches!(time::timeout(Duration::from_secs(1), socket.send(message)).await, Ok(Ok(()))) {
                            break;
                        }
                        pending = Some((write, Instant::now() + Duration::from_secs(12)));
                        continue;
                    }
                    message = socket.next() => message,
                };
                match message {
                    Some(Ok(Message::Text(text))) => match model::parse(text.as_bytes()) {
                        Ok(Some(Frame::State(state))) => {
                            let received = Instant::now();
                            deadline = received + stale_after;
                            retry = 1;
                            let connected = state
                                .values
                                .get("cn105_connected")
                                .and_then(serde_json::Value::as_bool)
                                == Some(true);
                            let control_echo = connected
                                && pending
                                    .as_ref()
                                    .is_some_and(|(write, _)| write.reported_by(&state));
                            if control_echo || !connected {
                                pending = None;
                            }
                            let observation = Observation {
                                state: Arc::new(state),
                                info: metadata.clone(),
                                received,
                                control_echo,
                            };
                            latest = Some(observation.clone());
                            output.send_replace(Some(observation));
                        }
                        Ok(Some(Frame::DeviceInfo(info))) => {
                            let info = Arc::new(info);
                            metadata = Some(info.clone());
                            if let Some(observation) = &mut latest {
                                observation.info = Some(info);
                                observation.control_echo = false;
                                // Metadata/log traffic must never extend reading freshness.
                                output.send_replace(Some(observation.clone()));
                            }
                        }
                        Ok(None) => (),
                        Err(_) => break,
                    },
                    Some(Ok(Message::Ping(_))) => {
                        // Tungstenite queues the protocol pong.
                        if !matches!(
                            time::timeout(Duration::from_secs(1), socket.flush()).await,
                            Ok(Ok(()))
                        ) {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => (),
                    _ => break,
                }
            }
        }
        output.send_replace(None);
        time::sleep(Duration::from_secs(retry)).await;
        retry = (retry * 2).min(30);
    }
}
