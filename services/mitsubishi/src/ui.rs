//! Use the shared AC card bundle without keeping its files in the process.
use homecore_plugin_ui::{
    ac_card::{self, PanelRequirement},
    installed_ui_dir, UiAssets,
};

pub(crate) fn assets() -> Option<UiAssets> {
    let dir = installed_ui_dir()?;
    match ac_card::load(&dir, env!("CARGO_PKG_VERSION"), PanelRequirement::Optional) {
        Ok(assets) => Some(assets),
        Err(_) => {
            eprintln!("AC card assets unavailable; telemetry will continue");
            None
        }
    }
}
