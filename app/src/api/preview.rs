// The world preview: the sibling repository's own server, started on this pack with no window of its own, since
// the launcher's window shows its page in a frame.

use std::path::PathBuf;

use super::{Api, Failure};
use crate::system;

pub const PREVIEW_URL: &str = "http://localhost:25590/";

/// The folders the native launcher gives the preview's engine: the ones `start_preview` gives serve.ps1.
pub struct PreviewPaths {
    /// The world-preview repository, whose mod the engine builds and gives the server.
    pub engine: PathBuf,
    pub pack: PathBuf,
    /// In a prod copy, the checkout it was staged from: Apply writes its pack, not the snapshot's (see source_pack_of).
    pub source_pack: Option<PathBuf>,
    pub dir: PathBuf,
    pub out: PathBuf,
    pub exports: PathBuf,
}

/// A prod copy (<checkout>\prod\current, staged by the pack repository's launcher\deploy-prod.ps1) holds a snapshot of
/// the pack that each deploy replaces: values applied to it would vanish at the next one. When `repo` is such a copy
/// and the checkout it sits in holds a pack, that pack is where the maintainer's values belong. None otherwise, or
/// when the copy was moved out of its checkout.
pub fn source_pack_of(repo: &std::path::Path) -> Option<PathBuf> {
    if !repo.join(".prod-commit").is_file() {
        return None;
    }
    let prod = repo.parent()?;
    if repo.file_name()? != "current" || prod.file_name()? != "prod" {
        return None;
    }
    let checkout = prod.parent()?;
    let pack = checkout.join("pack");
    (checkout.join(".git").exists() && pack.join("config").join("tectonic.json").is_file()).then_some(pack)
}

#[cfg(test)]
mod tests {
    use super::source_pack_of;
    use std::fs;
    use std::path::PathBuf;

    /// <root>\checkout with a pack and a .git, and <root>\checkout\prod\current staged (or not) with a snapshot.
    fn layout(name: &str, staged: bool) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("source_pack_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let checkout = root.join("checkout");
        fs::create_dir_all(checkout.join(".git")).unwrap();
        fs::create_dir_all(checkout.join("pack").join("config")).unwrap();
        fs::write(checkout.join("pack").join("config").join("tectonic.json"), "{}").unwrap();
        let current = checkout.join("prod").join("current");
        fs::create_dir_all(current.join("pack").join("config")).unwrap();
        fs::write(current.join("pack").join("config").join("tectonic.json"), "{}").unwrap();
        if staged {
            fs::write(current.join(".prod-commit"), "abc 1.0").unwrap();
        }
        (root, current)
    }

    #[test]
    fn a_prod_copy_applies_to_the_checkout_it_was_staged_from() {
        let (root, current) = layout("prod", true);
        assert_eq!(source_pack_of(&current), Some(root.join("checkout").join("pack")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_checkout_applies_to_its_own_pack() {
        // The main checkout has no .prod-commit: the caller keeps repo\pack.
        let (root, current) = layout("plain", false);
        assert_eq!(source_pack_of(&current), None);
        assert_eq!(source_pack_of(&root.join("checkout")), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_prod_copy_without_its_checkout_keeps_its_own_pack() {
        let (root, current) = layout("moved", true);
        fs::remove_dir_all(root.join("checkout").join(".git")).unwrap();
        assert_eq!(source_pack_of(&current), None);
        let _ = fs::remove_dir_all(root);
    }
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
    pub(super) fn start_preview(&self) -> Result<(), Failure> {
        let mut live = self.live();
        if live.preview.as_mut().is_some_and(|p| p.try_wait().ok().flatten().is_none()) {
            return Ok(());
        }
        let serve = self.preview_serve();
        if !serve.exists() {
            return Err(Failure::Internal(format!(
                "The world preview was not found: no scripts\\serve.ps1 in {}. Put the world-preview repository there, or set WORLD_PREVIEW_DIR to its folder.",
                serve.parent().unwrap().parent().unwrap().display()
            )));
        }
        let config = self.config()?;
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
    // <Dir>\out). The engine is the repository serve.ps1 sits in. Err says why the pack's folders are not known.
    pub fn preview_paths(&self) -> Result<PreviewPaths, String> {
        let config = self.config().map_err(|failure| failure.why().to_string())?;
        Ok(PreviewPaths {
            engine: self.preview_serve().parent().unwrap().parent().unwrap().to_path_buf(),
            pack: self.repo.join("pack"),
            source_pack: source_pack_of(&self.repo),
            dir: config.preview_dir.clone(),
            out: config.preview_dir.join("out"),
            exports: config.preview_exports.clone(),
        })
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