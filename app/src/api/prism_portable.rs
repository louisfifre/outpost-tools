// Not wired to the page or to native/ yet: where it is used is decided in the design track.
#![allow(dead_code)]

// Outpost Tools' own Prism Launcher: the portable Windows build, found on GitHub's releases with no account, put in
// `<root>/prism/` while Prism's data (accounts, instances, settings) lives apart in `<root>/prism-data/`, given to
// Prism by `--dir`. Replacing the program never touches the data. Nothing here starts Prism or signs in: the
// command lines are built and handed back to whoever runs them. profiles.rs still finds the Prism of Scoop; this is
// the path that will replace it.

use std::{
    ffi::OsString,
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The version Outpost Tools was checked with. Prism moves to a newer one only when this is raised.
pub const PINNED_VERSION: &str = "11.1.1";

const RELEASES_URL: &str = "https://api.github.com/repos/PrismLauncher/PrismLauncher/releases";
const PROGRAM: &str = "prismlauncher.exe";
// Prism's own updater is on only while this file sits beside the program (Application::updaterEnabled).
const UPDATER: &str = "prismlauncher_updater.exe";
// Where Outpost Tools remembers which version it put in the program folder.
const VERSION_FILE: &str = "outpost-prism-version.txt";
// The updater's own settings, read from the data folder (PrismExternalUpdater.cpp): a second switch, in case the
// updater file comes back.
const UPDATER_SETTINGS: &str = "prismlauncher_update.cfg";

// ---------- Where the releases come from ----------

/// What the network gives: the bytes behind a URL. Tests give fixtures instead.
pub trait Fetch {
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;
}

impl<F: Fn(&str) -> Result<Vec<u8>, String>> Fetch for F {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self(url)
    }
}

/// The real network: blocking, no account, no personal data.
pub struct Http;

impl Fetch for Http {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(concat!("outpost-tools/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(std::time::Duration::from_secs(600)))
            .build()
            .into();
        let mut reply = agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        reply
            .body_mut()
            .with_config()
            .limit(512 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| format!("{url}: {e}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub fn current() -> Arch {
        if cfg!(target_arch = "aarch64") { Arch::Arm64 } else { Arch::X64 }
    }
}

/// One release of Prism and its portable Windows zip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrismRelease {
    pub version: String,
    pub asset: String,
    pub url: String,
    /// The zip's size in bytes, to say before the download.
    pub size: u64,
    /// Lowercase hex, when GitHub states it.
    pub sha256: Option<String>,
}

/// The name of the portable Windows zip of a version (MSVC build).
pub fn portable_asset_name(version: &str, arch: Arch) -> String {
    match arch {
        Arch::X64 => format!("PrismLauncher-Windows-MSVC-Portable-{version}.zip"),
        Arch::Arm64 => format!("PrismLauncher-Windows-MSVC-arm64-Portable-{version}.zip"),
    }
}

/// The newest stable release. One request: GitHub allows 60 an hour with no account.
pub fn latest(net: &impl Fetch, arch: Arch) -> Result<PrismRelease, String> {
    parse_release(&net.get(&format!("{RELEASES_URL}/latest"))?, arch)
}

/// A given version, by its tag (Prism's tags are the bare version, `11.1.1`).
pub fn pinned(net: &impl Fetch, version: &str, arch: Arch) -> Result<PrismRelease, String> {
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(format!("not a Prism version: {version:?}"));
    }
    parse_release(&net.get(&format!("{RELEASES_URL}/tags/{version}"))?, arch)
}

/// Reads a GitHub release's JSON and picks its portable Windows zip.
pub fn parse_release(body: &[u8], arch: Arch) -> Result<PrismRelease, String> {
    let release: Value = serde_json::from_slice(body).map_err(|e| format!("GitHub's answer is not JSON: {e}"))?;
    let version = release["tag_name"].as_str().ok_or("the release has no tag")?.to_string();
    if release["draft"].as_bool() == Some(true) {
        return Err(format!("release {version} is a draft"));
    }
    let name = portable_asset_name(&version, arch);
    let asset = release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"].as_str() == Some(&name)))
        .ok_or_else(|| format!("release {version} has no {name}"))?;
    Ok(PrismRelease {
        asset: name,
        url: asset["browser_download_url"].as_str().ok_or("the asset has no URL")?.to_string(),
        size: asset["size"].as_u64().ok_or("the asset has no size")?,
        sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_lowercase),
        version,
    })
}

/// Downloads the zip of a release and checks its size and its hash. The caller says when to call it: the user
/// agrees to the download first.
pub fn download(net: &impl Fetch, release: &PrismRelease) -> Result<Vec<u8>, String> {
    let bytes = net.get(&release.url)?;
    if bytes.len() as u64 != release.size {
        return Err(format!("{} is {} bytes, GitHub says {}", release.asset, bytes.len(), release.size));
    }
    check_sha256(&bytes, release.sha256.as_deref())?;
    Ok(bytes)
}

fn check_sha256(bytes: &[u8], expected: Option<&str>) -> Result<(), String> {
    let Some(expected) = expected else { return Ok(()) };
    let got: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    if got.eq_ignore_ascii_case(expected) { Ok(()) } else { Err(format!("the zip's hash is {got}, expected {expected}")) }
}

// ---------- Folders ----------

/// Where Prism's program and data sit under Outpost Tools' folder.
pub struct Layout {
    pub program_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Layout {
    pub fn new(root: &Path) -> Layout {
        Layout { program_dir: root.join("prism"), data_dir: root.join("prism-data") }
    }
    pub fn program(&self) -> PathBuf {
        self.program_dir.join(PROGRAM)
    }
}

/// The version Outpost Tools put in `<root>/prism/`, None when there is none (or someone else's Prism).
pub fn installed_version(root: &Path) -> Option<String> {
    let layout = Layout::new(root);
    if !layout.program().is_file() {
        return None;
    }
    let text = fs::read_to_string(layout.program_dir.join(VERSION_FILE)).ok()?;
    Some(text.trim().to_string()).filter(|v| !v.is_empty())
}

// ---------- Install and update ----------

/// Puts the zip of `version` in `<root>/prism/`, replacing a previous program, and turns Prism's own updater off.
/// `<root>/prism-data/` is only given its updater setting: nothing else in it is read or written. Any failure
/// before the swap leaves the previous program in place.
pub fn install(root: &Path, version: &str, zip: &[u8], sha256: Option<&str>) -> Result<(), String> {
    check_sha256(zip, sha256)?;
    let layout = Layout::new(root);
    let staging = root.join("prism.new");
    let old = root.join("prism.old");
    remove_dir(&staging)?;
    remove_dir(&old)?;
    fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;

    let staged = extract(zip, &staging).and_then(|()| {
        if !staging.join(PROGRAM).is_file() {
            return Err(format!("the zip holds no {PROGRAM}"));
        }
        // Without this file Prism has no updater at all.
        let updater = staging.join(UPDATER);
        if updater.exists() {
            fs::remove_file(&updater).map_err(|e| format!("{}: {e}", updater.display()))?;
        }
        fs::write(staging.join(VERSION_FILE), version).map_err(|e| e.to_string())
    });
    if let Err(why) = staged {
        let _ = fs::remove_dir_all(&staging);
        return Err(why);
    }

    let had_previous = layout.program_dir.exists();
    if had_previous {
        // Fails while Prism runs from there: nothing has changed yet.
        if let Err(e) = fs::rename(&layout.program_dir, &old) {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("the previous Prism cannot be moved (is it running?): {e}"));
        }
    }
    if let Err(e) = fs::rename(&staging, &layout.program_dir) {
        if had_previous {
            let _ = fs::rename(&old, &layout.program_dir);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("the new Prism cannot be put in place: {e}"));
    }
    let _ = fs::remove_dir_all(&old);
    turn_updater_off(&layout.data_dir)
}

/// Writes `auto_check=false` in the updater's settings file of the data folder, keeping any other line.
pub fn turn_updater_off(data_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| format!("{}: {e}", data_dir.display()))?;
    let file = data_dir.join(UPDATER_SETTINGS);
    let text = fs::read_to_string(&file).unwrap_or_default();
    fs::write(&file, set_ini_value(&text, "General", "auto_check", "false")).map_err(|e| format!("{}: {e}", file.display()))
}

/// Sets `key=value` in `[section]` of an ini text (Qt's format), adding the section or the key when missing.
pub fn set_ini_value(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = format!("[{section}]");
    let line = format!("{key}={value}");
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        if lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.push(line);
        return lines.join("\r\n") + "\r\n";
    };
    let end = lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    match lines[start + 1..end].iter().position(|l| l.split('=').next().is_some_and(|k| k.trim() == key)) {
        Some(i) => lines[start + 1 + i] = line,
        None => lines.insert(start + 1, line),
    }
    lines.join("\r\n") + "\r\n"
}

fn remove_dir(dir: &Path) -> Result<(), String> {
    match fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", dir.display())),
        _ => Ok(()),
    }
}

// Unpacks into `dest`. When the program is not at the zip's top and every entry sits under one folder, that folder
// is dropped.
fn extract(zip: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| format!("not a zip: {e}"))?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        names.push(entry.enclosed_name().ok_or_else(|| format!("unsafe path in the zip: {}", entry.name()))?);
    }
    let first_dir = |n: &PathBuf| match n.components().next() {
        Some(Component::Normal(c)) if n.components().count() > 1 => Some(c.to_os_string()),
        _ => None,
    };
    let skip = match names.first().and_then(first_dir) {
        Some(top) if !names.iter().any(|n| n.as_path() == Path::new(PROGRAM)) && names.iter().all(|n| first_dir(n).as_ref() == Some(&top)) => 1,
        _ => 0,
    };
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for (i, name) in names.iter().enumerate() {
        let target = dest.join(name.components().skip(skip).collect::<PathBuf>());
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        fs::write(&target, data).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

// ---------- Command lines ----------

/// A command line to run, not run here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

fn base(root: &Path) -> Command {
    let layout = Layout::new(root);
    Command { program: layout.program(), args: vec!["--dir".into(), layout.data_dir.into_os_string()] }
}

/// Opens Prism on Outpost Tools' data folder. Prism has no command that opens the sign-in: with no account its
/// first-start wizard shows it (docs/research/launcher.md, section 4).
pub fn sign_in(root: &Path) -> Command {
    base(root)
}

/// Launches an instance by its id (its folder name in `prism-data/instances`). Prism stays hidden.
pub fn launch(root: &Path, instance: &str) -> Result<Command, String> {
    if instance.is_empty() || instance.starts_with('-') || instance.contains(['/', '\\']) {
        return Err(format!("not an instance id: {instance:?}"));
    }
    let mut command = base(root);
    command.args.extend(["--launch".into(), instance.into()]);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/prism-release-11.1.1.json");

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("outpost-prism-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in files {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn picks_the_portable_msvc_zip() {
        let release = parse_release(FIXTURE, Arch::X64).unwrap();
        assert_eq!(release.version, "11.1.1");
        assert_eq!(release.asset, "PrismLauncher-Windows-MSVC-Portable-11.1.1.zip");
        assert_eq!(release.size, 20396629);
        assert!(release.url.ends_with("/11.1.1/PrismLauncher-Windows-MSVC-Portable-11.1.1.zip"));
        assert_eq!(release.sha256.as_deref(), Some("ab35a770fb06d89d2ccc098079db5db329fb4e68f42b72babd8b095efde3d2d7"));
        let arm = parse_release(FIXTURE, Arch::Arm64).unwrap();
        assert_eq!(arm.asset, "PrismLauncher-Windows-MSVC-arm64-Portable-11.1.1.zip");
    }

    #[test]
    fn refuses_a_release_without_the_zip_or_a_draft() {
        assert!(parse_release(br#"{"tag_name":"1.0","draft":false,"assets":[]}"#, Arch::X64).is_err());
        assert!(parse_release(br#"{"tag_name":"1.0","draft":true,"assets":[]}"#, Arch::X64).is_err());
        assert!(parse_release(b"<html>", Arch::X64).is_err());
    }

    #[test]
    fn asks_github_for_latest_and_for_a_tag() {
        let seen = std::cell::RefCell::new(Vec::new());
        let net = |url: &str| {
            seen.borrow_mut().push(url.to_string());
            Ok(FIXTURE.to_vec())
        };
        latest(&net, Arch::X64).unwrap();
        pinned(&net, PINNED_VERSION, Arch::X64).unwrap();
        assert_eq!(
            *seen.borrow(),
            [
                "https://api.github.com/repos/PrismLauncher/PrismLauncher/releases/latest",
                "https://api.github.com/repos/PrismLauncher/PrismLauncher/releases/tags/11.1.1",
            ]
        );
        assert!(pinned(&net, "../x", Arch::X64).is_err());
    }

    #[test]
    fn download_checks_size_and_hash() {
        let bytes = b"abc".to_vec();
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let mut release =
            PrismRelease { version: "1".into(), asset: "a.zip".into(), url: "u".into(), size: 3, sha256: Some(hash.into()) };
        let net = |_: &str| Ok(bytes.clone());
        assert_eq!(download(&net, &release).unwrap(), bytes);
        release.size = 4;
        assert!(download(&net, &release).is_err());
        release.size = 3;
        release.sha256 = Some("00".into());
        assert!(download(&net, &release).is_err());
    }

    #[test]
    fn installs_and_replaces_without_touching_the_data() {
        let root = scratch("install");
        let v1 = make_zip(&[(PROGRAM, "v1"), (UPDATER, "updater"), ("platforms/q.dll", "q")]);
        install(&root, "11.1.1", &v1, None).unwrap();
        let layout = Layout::new(&root);
        assert_eq!(fs::read_to_string(layout.program()).unwrap(), "v1");
        assert!(!layout.program_dir.join(UPDATER).exists(), "the updater file must go");
        assert!(layout.program_dir.join("platforms/q.dll").is_file());
        assert_eq!(installed_version(&root).as_deref(), Some("11.1.1"));
        let settings = fs::read_to_string(layout.data_dir.join(UPDATER_SETTINGS)).unwrap();
        assert!(settings.contains("[General]") && settings.contains("auto_check=false"));

        // Data the user made, and an old file in the program folder that the new zip no longer has.
        fs::create_dir_all(layout.data_dir.join("instances/pack")).unwrap();
        fs::write(layout.data_dir.join("accounts.json"), "ACCOUNTS").unwrap();
        fs::write(layout.data_dir.join("instances/pack/instance.cfg"), "CFG").unwrap();
        fs::write(layout.program_dir.join("stale.dll"), "x").unwrap();

        let v2 = make_zip(&[("PrismLauncher/prismlauncher.exe", "v2"), ("PrismLauncher/new.dll", "n")]);
        install(&root, "11.2.0", &v2, None).unwrap();
        assert_eq!(fs::read_to_string(layout.program()).unwrap(), "v2");
        assert!(layout.program_dir.join("new.dll").is_file(), "a shared top folder is dropped");
        assert!(!layout.program_dir.join("stale.dll").exists());
        assert_eq!(installed_version(&root).as_deref(), Some("11.2.0"));
        assert_eq!(fs::read_to_string(layout.data_dir.join("accounts.json")).unwrap(), "ACCOUNTS");
        assert_eq!(fs::read_to_string(layout.data_dir.join("instances/pack/instance.cfg")).unwrap(), "CFG");
        assert!(!root.join("prism.new").exists() && !root.join("prism.old").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_bad_zip_leaves_the_previous_program() {
        let root = scratch("bad");
        install(&root, "1", &make_zip(&[(PROGRAM, "v1")]), None).unwrap();
        assert!(install(&root, "2", &make_zip(&[("readme.txt", "no program")]), None).is_err());
        assert!(install(&root, "2", b"not a zip", None).is_err());
        assert!(install(&root, "2", &make_zip(&[(PROGRAM, "v2")]), Some("00")).is_err());
        assert_eq!(fs::read_to_string(Layout::new(&root).program()).unwrap(), "v1");
        assert_eq!(installed_version(&root).as_deref(), Some("1"));
        assert!(!root.join("prism.new").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_updater_setting_keeps_other_lines() {
        let before = "[General]\r\nupdate_interval=0\r\nauto_check=true\r\n\r\n[Other]\r\nauto_check=true\r\n";
        let after = set_ini_value(before, "General", "auto_check", "false");
        assert_eq!(after, "[General]\r\nupdate_interval=0\r\nauto_check=false\r\n\r\n[Other]\r\nauto_check=true\r\n");
        assert_eq!(set_ini_value("", "General", "auto_check", "false"), "[General]\r\nauto_check=false\r\n");
        assert_eq!(set_ini_value("[General]\r\nx=1", "General", "auto_check", "false"), "[General]\r\nauto_check=false\r\nx=1\r\n");
    }

    #[test]
    fn builds_the_command_lines() {
        let root = Path::new("R");
        let data: OsString = root.join("prism-data").into_os_string();
        let open = sign_in(root);
        assert_eq!(open.program, root.join("prism").join("prismlauncher.exe"));
        assert_eq!(open.args, vec![OsString::from("--dir"), data.clone()]);
        let play = launch(root, "Pack-1_a").unwrap();
        assert_eq!(play.args, vec![OsString::from("--dir"), data, "--launch".into(), "Pack-1_a".into()]);
        assert!(launch(root, "--import").is_err());
        assert!(launch(root, "").is_err());
        assert!(launch(root, "a/b").is_err());
    }

    // Reads GitHub's public API: run on purpose with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reads_the_real_releases() {
        let release = latest(&Http, Arch::X64).unwrap();
        assert!(release.size > 1_000_000);
        assert_eq!(pinned(&Http, PINNED_VERSION, Arch::X64).unwrap().version, PINNED_VERSION);
    }
}
