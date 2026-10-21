//! Live reload on the web. In development runs (`?dev=1`, which `kulisa run --web` serves),
//! the page asks the dev server for the game's version every second and fetches the new
//! pack when it changed; the runtime then reloads what differs.

use kulisa_platform::Assets;
use web_time::{Duration, Instant};
use winit::event_loop::EventLoopProxy;

use crate::UserEvent;

const VERSION_URL: &str = "__kulisa/version";
const EVERY: Duration = Duration::from_secs(1);

#[derive(Default)]
pub struct Poll {
    version: Option<String>,
    last: Option<Instant>,
    busy: bool,
}

impl Poll {
    /// Asks for the version when it is time and no request is out.
    pub fn tick(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        if self.busy || self.last.is_some_and(|t| t.elapsed() < EVERY) {
            return;
        }
        self.busy = true;
        self.last = Some(Instant::now());
        let known = self.version.clone();
        let proxy = proxy.clone();
        wasm_bindgen_futures::spawn_local(async move {
            // Pages not served by the dev server (or after it stopped) keep asking, quietly.
            let version = kulisa_platform::fetch(VERSION_URL)
                .await
                .map(|b| String::from_utf8_lossy(&b).trim().to_string())
                .unwrap_or_default();
            let new = !version.is_empty() && known.as_ref().is_some_and(|k| *k != version);
            let pack = match new {
                true => Some(kulisa_platform::fetch("game.kpak").await.and_then(Assets::pack)),
                false => None,
            };
            let _ = proxy.send_event(UserEvent::DevVersion(version, pack));
        });
    }

    /// A version arrived. Returns the new files and the paths that changed, if any.
    pub fn arrived(
        &mut self,
        version: String,
        pack: Option<Result<Assets, String>>,
        old: Option<&Assets>,
    ) -> Option<(Assets, Vec<String>)> {
        self.busy = false;
        if version.is_empty() {
            return None;
        }
        if self.version.is_none() {
            self.version = Some(version);
            return None;
        }
        let assets = match pack? {
            Ok(a) => a,
            Err(e) => {
                log::warn!("live reload: {e}");
                return None;
            }
        };
        self.version = Some(version);
        let changed = old.map(|o| assets.changed_since(o)).unwrap_or_default();
        Some((assets, changed))
    }
}
