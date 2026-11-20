use crate::setup::Setup;
use serde_json::{json, Value};

pub async fn dispatch(setup: &Setup, action: &str) -> Option<Value> {
    dispatch_with_params(setup, action, &json!({})).await
}
pub async fn dispatch_with_params(setup: &Setup, action: &str, params: &Value) -> Option<Value> {
    match action {
        "sign_in_status" => Some(json!({"status":"ok", "setup_status":setup.status().await})),
        "begin_sign_in" => {
            let result = match params.get("region") {
                None => setup.begin().await,
                Some(Value::String(region)) => setup.begin_with_region(region).await,
                Some(_) => Err("Invalid Xiaomi region"),
            };
            Some(match result {
                Ok(qr) => json!({"status":"ok", "qr_png":qr}),
                Err(_) => json!({"status":"error", "error":"Could not start Xiaomi sign-in"}),
            })
        }
        _ => None,
    }
}
