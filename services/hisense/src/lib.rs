use quick_xml::{events::Event, Reader};
use std::{net::SocketAddr, time::Duration};

const MAX_RESPONSE: usize = 16 * 1024;
const SERVICE: &str = "urn:schemas-upnp-org:service:RenderingControl:1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub volume: u8,
    pub muted: bool,
}

pub struct Hisense {
    address: SocketAddr,
    client: reqwest::Client,
}

impl Hisense {
    pub fn new(address: SocketAddr, timeout: Duration) -> Result<Self, &'static str> {
        if address.port() == 0 || timeout.is_zero() {
            return Err("Invalid TV connection");
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(timeout)
            .build()
            .map_err(|_| "Cannot create TV client")?;
        Ok(Self { address, client })
    }

    async fn query(&self, action: &str, element: &[u8]) -> Result<String, &'static str> {
        let body = format!("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{SERVICE}\"><InstanceID>0</InstanceID><Channel>Master</Channel></u:{action}></s:Body></s:Envelope>");
        let response = self
            .client
            .post(format!("http://{}/upnp/control/mingusrcr", self.address))
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPACTION", format!("\"{SERVICE}#{action}\""))
            .body(body)
            .send()
            .await
            .map_err(|_| "TV unavailable")?;
        if !response.status().is_success() {
            return Err("TV returned an error");
        }
        let bytes = homecore_bounded_http::read_bounded(response, MAX_RESPONSE)
            .await
            .map_err(|error| match error {
                homecore_bounded_http::BodyReadError::Transfer => "TV response failed",
                homecore_bounded_http::BodyReadError::TooLarge => "TV response too large",
            })?;
        let mut reader = Reader::from_reader(bytes.as_slice());
        let mut within = false;
        let mut depth = 0usize;
        let mut roots = 0usize;
        let mut result = None;
        loop {
            match reader.read_event().map_err(|_| "Invalid TV response")? {
                Event::Start(tag) => {
                    if depth == 0 {
                        roots += 1;
                    }
                    depth += 1;
                    if depth > 64 {
                        return Err("Invalid TV response");
                    }
                    if tag.local_name().as_ref() == element {
                        if within || result.is_some() {
                            return Err("Ambiguous TV response");
                        }
                        within = true;
                    }
                }
                Event::Text(text) if within => {
                    if result.is_some() {
                        return Err("Ambiguous TV response");
                    }
                    result = Some(
                        std::str::from_utf8(text.as_ref())
                            .map_err(|_| "Invalid TV response")?
                            .trim()
                            .to_owned(),
                    );
                }
                Event::End(tag) => {
                    depth = depth.checked_sub(1).ok_or("Invalid TV response")?;
                    if tag.local_name().as_ref() == element {
                        within = false;
                    }
                }
                Event::DocType(_) => return Err("Invalid TV response"),
                Event::Eof => {
                    if depth != 0 || roots != 1 {
                        return Err("Invalid TV response");
                    }
                    break;
                }
                _ => {}
            }
        }
        result.ok_or("Missing TV reading")
    }

    pub async fn read(&self) -> Result<Reading, &'static str> {
        let volume = self
            .query("GetVolume", b"CurrentVolume")
            .await?
            .parse::<u8>()
            .map_err(|_| "Invalid volume")?;
        if volume > 100 {
            return Err("Invalid volume");
        }
        let muted = match self.query("GetMute", b"CurrentMute").await?.as_str() {
            "0" => false,
            "1" => true,
            _ => return Err("Invalid mute state"),
        };
        Ok(Reading { volume, muted })
    }
}

pub struct Broker {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

const PLUGIN: &str = "plugin.hisense";
const DEVICE: &str = "hisense_living_room";

async fn send(
    client: &rumqttc::AsyncClient,
    topic: &str,
    payload: String,
    retain: bool,
) -> Result<(), &'static str> {
    tokio::time::timeout(
        Duration::from_secs(2),
        client.publish(topic, rumqttc::QoS::AtLeastOnce, retain, payload),
    )
    .await
    .map_err(|_| "MQTT publish timed out")?
    .map_err(|_| "MQTT publish failed")
}

pub async fn run(tv: Hisense, broker: Broker, interval: Duration) -> Result<(), &'static str> {
    use rumqttc::{AsyncClient, Event, Incoming, LastWill, MqttOptions, QoS};
    use serde_json::json;
    if interval.is_zero() || broker.port == 0 {
        return Err("Invalid broker configuration");
    }
    let mut opts = MqttOptions::new(PLUGIN, broker.host, broker.port);
    opts.set_credentials(broker.username, broker.password);
    opts.set_keep_alive(Duration::from_secs(15));
    opts.set_last_will(LastWill::new(
        format!("homecore/devices/{DEVICE}/availability"),
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (mqtt, mut eventloop) = AsyncClient::new(opts, 16);
    let (ready_tx, mut ready_rx) = tokio::sync::watch::channel(false);
    let driver = tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    ready_tx.send_replace(true);
                }
                Ok(_) => {}
                Err(_) => {
                    ready_tx.send_replace(false);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
    struct Driver(tokio::task::JoinHandle<()>);
    impl Drop for Driver {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _driver = Driver(driver);
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    let started = tokio::time::Instant::now();
    let mut previous: Option<Reading> = None;
    let mut available = false;
    loop {
        tokio::select! {
            changed = ready_rx.changed() => {
                changed.map_err(|_| "MQTT driver stopped")?;
                if *ready_rx.borrow_and_update() {
                    send(&mqtt, &format!("homecore/plugins/{PLUGIN}/register"), json!({"device_id":DEVICE,"plugin_id":PLUGIN,"name":"Living room TV","device_type":"media_player"}).to_string(), true).await?;
                    send(&mqtt, &format!("homecore/plugins/{PLUGIN}/capabilities"), json!({"spec":"1","plugin_id":PLUGIN,"actions":[]}).to_string(), true).await?;
                    send(&mqtt, &format!("homecore/devices/{DEVICE}/schema"), json!({"attributes":{"volume":{"kind":"integer","writable":false,"min":0,"max":100,"unit":"%"},"muted":{"kind":"bool","writable":false}},"primary":["volume","muted"]}).to_string(), true).await?;
                    previous = None;
                    available = false;
                    send(&mqtt, &format!("homecore/devices/{DEVICE}/availability"), "offline".into(), true).await?;
                }
            }
            _ = ticks.tick(), if *ready_rx.borrow() => {
                match tv.read().await {
                    Ok(reading) => {
                        if previous.as_ref() != Some(&reading) {
                            send(&mqtt, &format!("homecore/devices/{DEVICE}/state"), json!({"volume":reading.volume,"muted":reading.muted}).to_string(), true).await?;
                            previous = Some(reading);
                        }
                        if !available {
                            send(&mqtt, &format!("homecore/devices/{DEVICE}/availability"), "online".into(), true).await?;
                            available = true;
                        }
                    }
                    Err(_) if available => {
                        send(&mqtt, &format!("homecore/devices/{DEVICE}/availability"), "offline".into(), true).await?;
                        available = false;
                        previous = None;
                    }
                    Err(_) => {}
                }
            }
            _ = heartbeat.tick(), if *ready_rx.borrow() => {
                send(&mqtt, &format!("homecore/plugins/{PLUGIN}/heartbeat"), json!({"version":env!("CARGO_PKG_VERSION"),"protocol_version":"0.1.5","uptime_secs":started.elapsed().as_secs(),"notices":[]}).to_string(), false).await?;
            }
        }
    }
}
