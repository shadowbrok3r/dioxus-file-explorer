use dioxus::prelude::*;
use crate::settings::{load_settings, save_settings, UiSettings};

/// Debounced UI settings hook. Provides a Signal<UiSettings> and persists
/// changes after a quiet period to reduce write amplification.
pub fn use_settings() -> Signal<UiSettings> {
    let ui = use_signal(load_settings);
    let last_saved_json = use_signal(|| serde_json::to_string(&ui.read().clone()).unwrap_or_default());
    {
        let ui_sig = ui.clone();
        let last_saved_sig_outer = last_saved_json.clone();
        use_future(move || {
            let mut last_saved_sig = last_saved_sig_outer.clone();
            async move {
                use tokio::time::{sleep, Duration};
                loop {
                    sleep(Duration::from_millis(550)).await;
                    let cur = ui_sig.read().clone();
                    let cur_ser = match serde_json::to_string(&cur) { Ok(s) => s, Err(_) => continue };
                    if cur_ser != *last_saved_sig.read() {
                        save_settings(&cur);
                        last_saved_sig.set(cur_ser);
                    }
                }
            }
        });
    }
    ui
}
