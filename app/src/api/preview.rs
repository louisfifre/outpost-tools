// The world preview: the sibling repository's own server, started on this pack with no window of its own, since
// the launcher's window shows its page in a frame.

use std::path::PathBuf;

use super::Api;
use crate::system;

pub const PREVIEW_URL: &str = "http://localhost:25590/";

/// The folders the native launcher gives the preview's generator: the ones `start_preview` gives serve.ps1.
pub struct PreviewPaths {
    pub generator: PathBuf,
    pub pack: PathBuf,
    pub dir: PathBuf,
    pub out: PathBuf,
    pub exports: PathBuf,
}

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

    // Where the preview's parts are on this pack, as start_preview gives them to serve.ps1 (its own Out default is
    // <Dir>\out). The generator is preview.ps1, beside serve.ps1.
    pub fn preview_paths(&self) -> PreviewPaths {
        let config = self.config();
        PreviewPaths {
            generator: self.preview_serve().with_file_name("preview.ps1"),
            pack: self.repo.join("pack"),
            dir: config.preview_dir.clone(),
            out: config.preview_dir.join("out"),
            exports: config.preview_exports.clone(),
        }
    }

    // The glue mod's jar, built as start_preview builds it (`$env:WORLD_PREVIEW_MODS = Build-GlueMod`): the path
    // pwsh prints. Hidden, and it takes a moment the first time.
    pub fn glue_mod(&self) -> Result<PathBuf, String> {
        let script = format!(". '{}'; Build-GlueMod", self.repo.join("scripts").join("common.ps1").display());
        let out = system::hidden("pwsh").args(["-NoProfile", "-Command", &script]).output().map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or_default();
        if !out.status.success() || line.is_empty() {
            return Err(format!("The glue mod could not be built: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(PathBuf::from(line))
    }
}