//! One MQTT identity per unit preserves retained discovery and per-device last wills.
use crate::{
    config::{Config, Unit},
    control::{Command, Controls, ReceivedCommand, MAX_COMMAND_BYTES},
    sensors,
    source::{self, Observation},
    ui,
};
use homecore_plugin_ui::{UiAssets, MAX_REQUEST_BYTES};
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, LastWill, MqttOptions, QoS};
use serde_json::{json, Map, Value};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::{JoinHandle, JoinSet},
    time::{self, Instant},
};

#[derive(Clone)]
pub struct Broker {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

struct Tasks(Vec<JoinHandle<()>>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

pub async fn run(config: Config, broker: Broker) -> Result<(), &'static str> {
    let mut tasks = JoinSet::new();
    let assets = ui::assets().map(Arc::new);
    for unit in config.units {
        tasks.spawn(run_unit(
            unit,
            broker.clone(),
            config.publish_interval_secs,
            config.stale_after_secs,
            assets.clone(),
        ));
    }
    // Units normally run forever. A crashed task must not leave a partially dead service.
    let _ = tasks.join_next().await;
    Err("Mitsubishi unit worker stopped")
}

struct Publication {
    values: Map<String, Value>,
    schema: Value,
    available: bool,
}

impl Publication {
    fn from_observation(observation: Option<&Observation>) -> Self {
        let mut values = Map::new();
        let mut attributes = Map::new();
        let available = if let Some(value) = observation {
            values.extend(value.state.values.clone());
            attributes.extend(value.state.attributes.clone());
            if let Some(info) = &value.info {
                values.extend(info.values.clone());
                attributes.extend(info.attributes.clone());
            }
            if let Some(controls) = Controls::from_snapshot(&value.state) {
                controls.describe(&mut values, &mut attributes);
            }
            values.get("cn105_connected") == Some(&Value::Bool(true))
        } else {
            false
        };
        let primary: Vec<_> = ["temperature", "setpoint", "hvac_mode"]
            .into_iter()
            .filter(|key| attributes.contains_key(*key))
            .collect();
        let schema = json!({"attributes":attributes,"primary":primary});
        Self {
            values,
            schema,
            available,
        }
    }
}

struct UnitIdentity {
    plugin_id: String,
    device_id: String,
}

impl UnitIdentity {
    fn new(unit_id: &str) -> Self {
        Self {
            plugin_id: format!("plugin.mitsubishi.{unit_id}"),
            device_id: format!("mitsubishi_{unit_id}"),
        }
    }

    fn plugin_topic(&self, suffix: &str) -> String {
        format!("homecore/plugins/{}/{suffix}", self.plugin_id)
    }

    fn device_topic(&self, suffix: &str) -> String {
        format!("homecore/devices/{}/{suffix}", self.device_id)
    }
}

struct Publisher {
    mqtt: AsyncClient,
    identity: UnitIdentity,
    schema: Option<Value>,
    available: bool,
}

impl Publisher {
    async fn send(&self, topic: String, retain: bool, body: String) -> Result<(), &'static str> {
        time::timeout(
            Duration::from_secs(2),
            self.mqtt.publish(topic, QoS::AtLeastOnce, retain, body),
        )
        .await
        .map_err(|_| "MQTT publication timed out")?
        .map_err(|_| "MQTT publication failed")
    }

    async fn register(
        &mut self,
        unit: &Unit,
        assets: Option<&UiAssets>,
    ) -> Result<(), &'static str> {
        self.schema = None;
        self.send(self.identity.plugin_topic("register"), true, json!({
            "device_id":self.identity.device_id,"plugin_id":self.identity.plugin_id,"name":unit.name,"device_type":"thermostat"
        }).to_string()).await?;
        self.send(
            self.identity.plugin_topic("capabilities"),
            true,
            json!({
                "spec":"1","plugin_id":self.identity.plugin_id,"actions":[],
                "ui":assets.map(UiAssets::descriptor)
            })
            .to_string(),
        )
        .await
    }

    async fn heartbeat(&self, started: Instant) -> Result<(), &'static str> {
        self.send(
            self.identity.plugin_topic("heartbeat"),
            false,
            json!({
                "version":env!("CARGO_PKG_VERSION"),"protocol_version":"0.1.5",
                "uptime_secs":started.elapsed().as_secs(),"notices":[]
            })
            .to_string(),
        )
        .await
    }

    async fn publish(
        &mut self,
        observation: Option<&Observation>,
        stale: Duration,
    ) -> Result<(), &'static str> {
        let observation = observation.filter(|value| value.received.elapsed() < stale);
        let Publication {
            values,
            schema,
            available,
        } = Publication::from_observation(observation);
        if self.schema.as_ref() != Some(&schema) {
            self.send(
                self.identity.device_topic("schema"),
                true,
                schema.to_string(),
            )
            .await?;
            self.schema = Some(schema);
        }
        self.send(
            self.identity.device_topic("state"),
            true,
            Value::Object(values).to_string(),
        )
        .await?;
        self.send(
            self.identity.device_topic("availability"),
            true,
            if available { "online" } else { "offline" }.into(),
        )
        .await?;
        self.available = available;
        Ok(())
    }
}

async fn drive_mqtt(
    mut eventloop: EventLoop,
    ready_tx: watch::Sender<bool>,
    management: Option<(String, mpsc::Sender<Vec<u8>>)>,
    device_commands: (String, mpsc::Sender<ReceivedCommand>),
) {
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                ready_tx.send_replace(true);
            }
            Ok(Event::Incoming(Incoming::Publish(message)))
                if message.topic == device_commands.0 =>
            {
                if !message.retain && !message.dup && message.payload.len() <= MAX_COMMAND_BYTES {
                    if let Ok(command) = serde_json::from_slice::<Command>(&message.payload) {
                        let _ = device_commands.1.try_send(ReceivedCommand {
                            command,
                            received: Instant::now(),
                        });
                    }
                }
            }
            Ok(Event::Incoming(Incoming::Publish(message))) => {
                if let Some((topic, requests)) = &management {
                    if message.topic == *topic
                        && !message.retain
                        && message.payload.len() <= MAX_REQUEST_BYTES
                    {
                        // A busy requester cannot hold up MQTT polling or allocate an unbounded queue.
                        let _ = requests.try_send(message.payload.to_vec());
                    }
                }
            }
            Ok(_) => (),
            Err(_) => {
                ready_tx.send_replace(false);
                time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn run_unit(
    unit: Unit,
    broker: Broker,
    publish_secs: u64,
    stale_secs: u64,
    assets: Option<Arc<UiAssets>>,
) {
    let identity = UnitIdentity::new(&unit.id);
    let mut options =
        MqttOptions::new(identity.plugin_id.clone(), broker.host.clone(), broker.port);
    options.set_credentials(broker.username.clone(), broker.password.clone());
    options.set_keep_alive(Duration::from_secs(15));
    // A 128 KiB UI chunk grows to about 171 KiB in the base64 MQTT response.
    options.set_max_packet_size(4096, 192 * 1024);
    options.set_inflight(2);
    options.set_last_will(LastWill::new(
        identity.device_topic("availability"),
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (mqtt, eventloop) = AsyncClient::new(options, 8);
    let (ready_tx, mut ready) = watch::channel(false);
    let (source_tx, mut latest) = watch::channel(None);
    let stale = Duration::from_secs(stale_secs);
    let command_topic = identity.plugin_topic("manage/cmd");
    let response_topic = identity.plugin_topic("manage/response");
    let (requests_tx, mut requests) = mpsc::channel(1);
    let control_topic = identity.device_topic("cmd");
    let (commands_tx, commands) = mpsc::channel(8);
    let management = assets
        .as_ref()
        .map(|_| (command_topic.clone(), requests_tx));
    let _tasks = Tasks(vec![
        tokio::spawn(drive_mqtt(
            eventloop,
            ready_tx,
            management,
            (control_topic.clone(), commands_tx),
        )),
        tokio::spawn(source::run(
            unit.clone(),
            stale,
            source_tx,
            commands,
            ready.clone(),
        )),
    ]);
    let _sensors = Tasks(
        sensors::SENSORS
            .into_iter()
            .map(|kind| {
                tokio::spawn(sensors::run(
                    unit.clone(),
                    broker.clone(),
                    kind,
                    latest.clone(),
                    Duration::from_secs(publish_secs),
                    stale,
                ))
            })
            .collect(),
    );
    let mut publisher = Publisher {
        mqtt,
        identity,
        schema: None,
        available: false,
    };
    let started = Instant::now();
    let mut publish = time::interval(Duration::from_secs(publish_secs));
    publish.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut heartbeat = time::interval(Duration::from_secs(30));
    heartbeat.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            changed = ready.changed() => {
                if changed.is_err() { return; }
                let connected = *ready.borrow_and_update();
                if connected {
                    if !matches!(
                        time::timeout(Duration::from_secs(2), publisher.mqtt.subscribe(&control_topic, QoS::AtLeastOnce)).await,
                        Ok(Ok(()))
                    ) {
                        return;
                    }
                    let serve_ui = if assets.is_some() && matches!(
                        time::timeout(Duration::from_secs(2), publisher.mqtt.subscribe(&command_topic, QoS::AtLeastOnce)).await,
                        Ok(Ok(()))
                    ) { assets.as_deref() } else { None };
                    if publisher.register(&unit, serve_ui).await.is_err() { continue; }
                    let snapshot = latest.borrow().clone();
                    let _ = publisher.publish(snapshot.as_ref(), stale).await;
                    let _ = publisher.heartbeat(started).await;
                }
            }
            changed = latest.changed() => {
                if changed.is_err() { return; }
                let snapshot = latest.borrow_and_update().clone();
                let connected = snapshot.as_ref().is_some_and(|value| value.state.values.get("cn105_connected") == Some(&Value::Bool(true)));
                let control_echo = snapshot.as_ref().is_some_and(|value| value.control_echo);
                if *ready.borrow() && (!connected || !publisher.available || control_echo) {
                    let _ = publisher.publish(snapshot.as_ref(), stale).await;
                }
            }
            Some(request) = requests.recv(), if *ready.borrow() => {
                // Only a requested chunk lives in memory, and file I/O stays off the MQTT poller.
                if let Some(assets) = assets.clone() {
                    if let Ok(Some(reply)) = tokio::task::spawn_blocking(move || assets.respond(&request)).await {
                        let _ = publisher.send(response_topic.clone(), false, reply.to_string()).await;
                    }
                }
            }
            _ = publish.tick() => {
                if *ready.borrow() {
                    let snapshot = latest.borrow().clone();
                    let _ = publisher.publish(snapshot.as_ref(), stale).await;
                }
            }
            _ = heartbeat.tick() => {
                if *ready.borrow() { let _ = publisher.heartbeat(started).await; }
            }
        }
    }
}
