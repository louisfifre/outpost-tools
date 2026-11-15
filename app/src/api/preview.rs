// The world preview: the sibling repository's own server, started on this pack with no window of its own, since
// the launcher's window shows its page in a frame.

use std::path::PathBuf;

use super::Api;
use crate::system;

pub const PREVIEW_URL: &str = "http://localhost:25590/";

impl Api {
    // Its own repository, the sibling folder ..\world-preview or WORLD_PREVIEW_DIR when set.
    pub(super) fn preview_serve(&self) -> PathBuf {
        let dir = std::env::var_os("WORLD_PREVIEW_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.repo.parent().unwrap().join("world-preview"));
        dir.join("scripts").join("serve.ps1")
    }

    // Starts the preview's server on this pack, with no window of its own: the page holds it. Its server watches
    // this process and stops with it. The glue mod is not in the pack: the preview's server takes its jar on top,
    // so the section shows the pack's own ores. A preview from before -Exports keeps its exports in <Dir>\exports;
    // one from before -ParentPid stops once its pings stop instead.
    pub(super) fn start_preview(&self) -> Result<(), String> {
        let mut live = self.live.lock().unwrap();
        if live.preview.as_mut().is_some_and(|p| p.try_wait().ok().flatten().is_none()) {
            return Ok(());
        }
        let serve = self.preview_serve();
        if !serve.exists() {
            return Err(format!(
                "The world preview was not found: no scripts\\serve.ps1 in {}. Put the world-preview repository there, or set WORLD_PREVIEW_DIR to its folder.",
                serve.parent().unwrap().parent().unwrap().display()
            ));
        }
        let config = self.config();
        let script = format!(
            ". '{common}'; $env:WORLD_PREVIEW_MODS = Build-GlueMod; \
             $serve = '{serve}'; $known = (Get-Command $serve).Parameters; \
             $a = @{{ Pack = '{pack}'; Dir = '{dir}'; NoWindow = $true }}; \
             if ($known.ContainsKey('Exports')) {{ $a.Exports = '{exports}' }}; \
             if ($known.ContainsKey('ParentPid')) {{ $a.ParentPid = {pid} }}; \
             & $serve @a",
            common = self.repo.join("scripts").join("common.ps1").display(),
            serve = serve.display(),
            pack = self.repo.join("pack").display(),
            dir = config.preview_dir.display(),
            exports = config.preview_exports.display(),
            pid = std::process::id(),
        );
        let child = system::hidden("pwsh").args(["-NoProfile", "-Command", &script]).spawn().map_err(|e| e.to_string())?;
        live.preview = Some(child);
        Ok(())
    }
}
