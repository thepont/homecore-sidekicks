use crate::{
    adapters::AdapterRegistration,
    registry::{AccessRequest, Operation},
    setup::Setup,
};
use serde_json::{json, Value};

pub(crate) enum Command {
    Status,
    BeginSignIn,
    RegisterAdapter,
    RequestAccess,
}

impl Command {
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        match id {
            "sign_in_status" => Some(Self::Status),
            "begin_sign_in" => Some(Self::BeginSignIn),
            "register_adapter" => Some(Self::RegisterAdapter),
            "request_access" => Some(Self::RequestAccess),
            _ => None,
        }
    }

    pub(crate) async fn execute(self, setup: &Setup, params: &Value) -> Value {
        match self {
            Self::Status => setup.action_status().await,
            Self::BeginSignIn => {
                let result = match params.get("region") {
                    None => setup.begin().await,
                    Some(Value::String(region)) => setup.begin_with_region(region).await,
                    Some(_) => Err("Invalid Xiaomi region"),
                };
                match result {
                    Ok(qr) => json!({"status":"ok", "qr_png":qr}),
                    Err(_) => json!({"status":"error", "error":"Could not start Xiaomi sign-in"}),
                }
            }
            Self::RegisterAdapter => {
                apply(
                    setup,
                    AdapterRegistration::parse(params).map(Operation::Register),
                )
                .await
            }
            Self::RequestAccess => {
                apply(setup, AccessRequest::parse(params).map(Operation::Request)).await
            }
        }
    }
}

async fn apply(setup: &Setup, operation: Result<Operation, &'static str>) -> Value {
    let result = match operation {
        Ok(operation) => setup.update_registry(operation).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => json!({"status":"ok"}),
        Err(error) => json!({"status":"error", "error":error}),
    }
}

pub async fn dispatch(setup: &Setup, action: &str) -> Option<Value> {
    dispatch_with_params(setup, action, &json!({})).await
}
pub async fn dispatch_with_params(setup: &Setup, action: &str, params: &Value) -> Option<Value> {
    Some(Command::from_id(action)?.execute(setup, params).await)
}
