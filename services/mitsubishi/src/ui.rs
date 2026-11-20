//! Use the shared AC card bundle without keeping its files in the process.
use homecore_plugin_ui::{UiAssets, UiError};
use serde_json::json;
use std::path::PathBuf;

pub(crate) fn assets() -> Option<UiAssets> {
    let dir = std::env::var_os("HC_PLUGIN_UI_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(|p| p.join("ui")))?;
    match UiAssets::load(&dir, env!("CARGO_PKG_VERSION"), |slots| {
        let card = json!({
            "slot": "device.card", "element": "hc-ac-card", "forms": ["standard", "large"]
        });
        let panel = json!({"slot": "device.panel", "element": "hc-ac-panel"});
        match slots {
            [only] if *only == card => Ok(()),
            [first, second] if *first == card && *second == panel => Ok(()),
            _ => Err(UiError),
        }
    }) {
        Ok(assets) => Some(assets),
        Err(_) => {
            eprintln!("AC card assets unavailable; telemetry will continue");
            None
        }
    }
}
