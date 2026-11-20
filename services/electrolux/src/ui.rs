//! Load the shared AC card without retaining asset bodies in memory.
use homecore_plugin_ui::{
    ac_card::{self, PanelRequirement},
    installed_ui_dir, UiAssets,
};

pub fn assets() -> Option<UiAssets> {
    let dir = installed_ui_dir()?;
    ac_card::load(&dir, env!("CARGO_PKG_VERSION"), PanelRequirement::Required).ok()
}
