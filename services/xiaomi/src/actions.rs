use crate::setup::Setup;
use serde_json::{json, Value};

pub async fn dispatch(setup: &Setup, action: &str) -> Option<Value> {
    match action {
        "sign_in_status" => Some(json!({"status":"ok", "setup_status":setup.status().await})),
        "begin_sign_in" => Some(match setup.begin().await {
            Ok(qr) => json!({"status":"ok", "qr_png":qr}),
            Err(_) => json!({"status":"error", "error":"Could not start Xiaomi sign-in"}),
        }),
        _ => None,
    }
}
