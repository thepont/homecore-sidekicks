use crate::{
    login::{insert_discovered_device, Provider},
    transport::{bearer, valid_identifier, valid_region, MAX_MODELS, MAX_SESSION},
};
use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    sync::Arc,
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;
use zeroize::{Zeroize, Zeroizing};

pub use crate::transport::read_service_key;

struct Worker {
    provider: Provider,
    authorization: Zeroizing<String>,
    slot: Arc<Semaphore>,
}

pub fn router(provider: Provider, mut key: [u8; 32]) -> Router {
    let authorization = Zeroizing::new(format!("Bearer {}", bearer(&key).as_str()));
    key.zeroize();
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/v1/discover", post(discover))
        .layer(DefaultBodyLimit::max(65536))
        .with_state(Arc::new(Worker {
            provider,
            authorization,
            slot: Arc::new(Semaphore::new(1)),
        }))
}

async fn discover(
    State(worker): State<Arc<Worker>>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    if !headers
        .get(header::AUTHORIZATION)
        .is_some_and(|value| bool::from(value.as_bytes().ct_eq(worker.authorization.as_bytes())))
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(region) = input["region"].as_str().filter(|value| valid_region(value)) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(models) = input["models"]
        .as_array()
        .filter(|items| items.len() <= MAX_MODELS)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let mut allowed = BTreeSet::new();
    for value in models {
        let Some(model) = value.as_str().filter(|value| valid_identifier(value)) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        allowed.insert(model.to_owned());
    }
    let Ok(lease) = worker.slot.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let Ok(challenge) = worker.provider.begin().await else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    let qr = frame(json!({"type":"qr","qr_png":challenge.qr_png}));
    let region = region.to_owned();
    // A live response retains the lease. If its caller disconnects, dropping
    // the response cancels discovery and releases the slot.
    let result = async move {
        let _lease = lease;
        let deadline = challenge.expires_after.min(MAX_SESSION);
        let result = tokio::time::timeout(deadline, async {
            let authorization = worker.provider.poll(&challenge).await?;
            let mut devices = BTreeMap::new();
            worker
                .provider
                .visit_devices(&authorization, &region, |device| {
                    let Some(id) = device.device_id.filter(|id| valid_identifier(id)) else {
                        return Ok(());
                    };
                    if allowed.contains(device.model) {
                        let value = json!({
                            "did": id, "name": device.name, "model": device.model,
                            "localip": device.address.to_string(), "token": device.token,
                        });
                        insert_discovered_device(&mut devices, id.to_owned(), value)?;
                    }
                    Ok(())
                })
                .await?;
            Ok::<_, &'static str>(
                json!({"type":"devices","devices":devices.into_values().collect::<Vec<_>>()}),
            )
        })
        .await;
        match result {
            Ok(Ok(devices)) => frame(devices),
            _ => frame(json!({"type":"error"})),
        }
    };
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(stream::once(async move { qr }).chain(stream::once(result))),
    )
        .into_response()
}

fn frame(value: Value) -> Result<Bytes, Infallible> {
    let mut frame = value.to_string();
    frame.push('\n');
    Ok(Bytes::from(frame))
}
