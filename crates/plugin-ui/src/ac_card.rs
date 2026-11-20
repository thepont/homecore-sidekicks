//! Manifest contract for the shared AC card bundle.

use crate::{UiAssets, UiError};
use serde_json::json;
use std::path::Path;

pub enum PanelRequirement {
    Required,
    /// Accept the legacy card-only manifest as well.
    Optional,
}

pub fn load(
    dir: &Path,
    version: &str,
    panel_requirement: PanelRequirement,
) -> Result<UiAssets, UiError> {
    UiAssets::load(dir, version, |slots| {
        let card = json!({
            "slot": "device.card", "element": "hc-ac-card", "forms": ["standard", "large"]
        });
        let panel = json!({"slot": "device.panel", "element": "hc-ac-panel"});
        match slots {
            [first, second] if *first == card && *second == panel => Ok(()),
            [only] if matches!(panel_requirement, PanelRequirement::Optional) && *only == card => {
                Ok(())
            }
            _ => Err(UiError),
        }
    })
}
