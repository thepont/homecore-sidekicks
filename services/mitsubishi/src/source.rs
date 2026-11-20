//! A controller's WebSocket lifetime is independent of the MQTT publisher.
use crate::{
    config::Unit,
    model::{self, Frame, Snapshot},
};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::watch,
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
}

pub(crate) async fn run(
    unit: Unit,
    stale_after: Duration,
    output: watch::Sender<Option<Observation>>,
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
            let mut metadata = None;
            let mut latest: Option<Observation> = None;
            let mut deadline = Instant::now() + stale_after;
            loop {
                let message = tokio::select! {
                    _ = time::sleep_until(deadline) => break,
                    message = socket.next() => message,
                };
                match message {
                    Some(Ok(Message::Text(text))) => match model::parse(text.as_bytes()) {
                        Ok(Some(Frame::State(state))) => {
                            let received = Instant::now();
                            deadline = received + stale_after;
                            retry = 1;
                            let observation = Observation {
                                state: Arc::new(state),
                                info: metadata.clone(),
                                received,
                            };
                            latest = Some(observation.clone());
                            output.send_replace(Some(observation));
                        }
                        Ok(Some(Frame::DeviceInfo(info))) => {
                            let info = Arc::new(info);
                            metadata = Some(info.clone());
                            if let Some(observation) = &mut latest {
                                observation.info = Some(info);
                                // Metadata/log traffic must never extend reading freshness.
                                output.send_replace(Some(observation.clone()));
                            }
                        }
                        Ok(None) => (),
                        Err(_) => break,
                    },
                    Some(Ok(Message::Ping(_))) => {
                        // Tungstenite queues the protocol pong. No application commands are sent.
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
