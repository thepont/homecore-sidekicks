//! One MQTT identity per unit preserves retained discovery and per-device last wills.
use crate::{
    config::{Config, Unit},
    source::{self, Observation},
};
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, LastWill, MqttOptions, QoS};
use serde_json::{json, Map, Value};
use std::time::Duration;
use tokio::{
    sync::watch,
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
    for unit in config.units {
        tasks.spawn(run_unit(
            unit,
            broker.clone(),
            config.publish_interval_secs,
            config.stale_after_secs,
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

struct Publisher {
    mqtt: AsyncClient,
    plugin: String,
    device: String,
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

    async fn register(&mut self, unit: &Unit) -> Result<(), &'static str> {
        self.schema = None;
        self.send(format!("homecore/plugins/{}/register", self.plugin), true, json!({
            "device_id":self.device,"plugin_id":self.plugin,"name":unit.name,"device_type":"sensor"
        }).to_string()).await?;
        self.send(
            format!("homecore/plugins/{}/capabilities", self.plugin),
            true,
            json!({"spec":"1","plugin_id":self.plugin,"actions":[]}).to_string(),
        )
        .await
    }

    async fn heartbeat(&self, started: Instant) -> Result<(), &'static str> {
        self.send(
            format!("homecore/plugins/{}/heartbeat", self.plugin),
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
                format!("homecore/devices/{}/schema", self.device),
                true,
                schema.to_string(),
            )
            .await?;
            self.schema = Some(schema);
        }
        self.send(
            format!("homecore/devices/{}/state", self.device),
            true,
            Value::Object(values).to_string(),
        )
        .await?;
        self.send(
            format!("homecore/devices/{}/availability", self.device),
            true,
            if available { "online" } else { "offline" }.into(),
        )
        .await?;
        self.available = available;
        Ok(())
    }
}

async fn drive_mqtt(mut eventloop: EventLoop, ready_tx: watch::Sender<bool>) {
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                ready_tx.send_replace(true);
            }
            Ok(_) => (),
            Err(_) => {
                ready_tx.send_replace(false);
                time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn run_unit(unit: Unit, broker: Broker, publish_secs: u64, stale_secs: u64) {
    let plugin = format!("plugin.mitsubishi.{}", unit.id);
    let device = format!("mitsubishi_{}", unit.id);
    let mut options = MqttOptions::new(plugin.clone(), broker.host, broker.port);
    options.set_credentials(broker.username, broker.password);
    options.set_keep_alive(Duration::from_secs(15));
    options.set_max_packet_size(4096, 128 * 1024);
    options.set_inflight(2);
    options.set_last_will(LastWill::new(
        format!("homecore/devices/{device}/availability"),
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (mqtt, eventloop) = AsyncClient::new(options, 8);
    let (ready_tx, mut ready) = watch::channel(false);
    let (source_tx, mut latest) = watch::channel(None);
    let stale = Duration::from_secs(stale_secs);
    let _tasks = Tasks(vec![
        tokio::spawn(drive_mqtt(eventloop, ready_tx)),
        tokio::spawn(source::run(unit.clone(), stale, source_tx)),
    ]);
    let mut publisher = Publisher {
        mqtt,
        plugin,
        device,
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
                    if publisher.register(&unit).await.is_err() { continue; }
                    let snapshot = latest.borrow().clone();
                    let _ = publisher.publish(snapshot.as_ref(), stale).await;
                    let _ = publisher.heartbeat(started).await;
                }
            }
            changed = latest.changed() => {
                if changed.is_err() { return; }
                let snapshot = latest.borrow_and_update().clone();
                let connected = snapshot.as_ref().is_some_and(|value| value.state.values.get("cn105_connected") == Some(&Value::Bool(true)));
                if *ready.borrow() && (!connected || !publisher.available) {
                    let _ = publisher.publish(snapshot.as_ref(), stale).await;
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
