// The Java the game, the world preview and the servers run on (Java 25 for Minecraft 26.x). Two jobs, no interface:
// find a Java 25 already on the PC and say which one and where, and, when there is none, say what Temurin's vendor API
// offers (archive, size, checksum) and put a downloaded zip into Outpost Tools' own folder, `<root>/java/`, after
// checking it. Nothing is installed in Windows and java.exe is never run: a Java home is read through its `release`
// file. Network code sits behind `Fetch`, so tests run on saved answers.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde_json::Value;
use sha2::{Digest, Sha256};

// The game's Java: Minecraft 26.x needs 25 or newer.
pub const MIN_MAJOR: u32 = 25;
const JAVA_EXE: &str = "java.exe";
// How deep below a scanned folder a home may sit: the official launcher keeps
// runtime\java-runtime-delta\windows-x64\java-runtime-delta\bin.
const SCAN_DEPTH: u32 = 4;
// The folder an extraction works in before its result takes its final name, so a failed one leaves no half Java.
const STAGING: &str = ".partial";

// ---------------------------------------------------------------------------------------------------------------
// Finding a Java on the PC

// Where a Java was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaSource {
    JavaHome,
    Path,
    OfficialLauncher,
    Prism,
    OutpostTools,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstall {
    pub home: PathBuf,
    pub exe: PathBuf,
    // JAVA_VERSION of the release file, such as "25.0.5".
    pub version: String,
    pub major: u32,
    // "JRE" or "JDK" when the release file says.
    pub image_type: Option<String>,
    pub source: JavaSource,
}

// The places looked in, given as data so a test can name its own. They are tried in the order JAVA_HOME, PATH, then
// `scanned`, which is the order of preference.
#[derive(Debug, Clone, Default)]
pub struct Locations {
    pub java_home: Vec<PathBuf>,
    pub path_dirs: Vec<PathBuf>,
    // Folders that hold Java homes somewhere below them.
    pub scanned: Vec<(JavaSource, PathBuf)>,
}

impl Locations {
    // The PC's own: JAVA_HOME, PATH, the official launcher's runtime folders, Prism's java folders, and the root's
    // java/. The user-level JAVA_HOME of the registry is not read: a Windows that has just set it shows it to the
    // programs opened afterwards.
    pub fn from_env(root: &Path) -> Locations {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty());
        let join = |base: Option<PathBuf>, rest: &[&str]| base.map(|b| rest.iter().fold(b, |acc, part| acc.join(part)));
        let java_home =
            var("JAVA_HOME").map(|p| PathBuf::from(p.to_string_lossy().trim_matches('"'))).into_iter().collect();
        let path_dirs = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        let mut scanned = Vec::new();
        let mut add = |source, dir: Option<PathBuf>| scanned.extend(dir.map(|d| (source, d)));
        // The Store build of the launcher, the classic one, and the older install.
        add(
            JavaSource::OfficialLauncher,
            join(
                var("LOCALAPPDATA"),
                &["Packages", "Microsoft.4297127D64EC6_8wekyb3d8bbwe", "LocalCache", "Local", "runtime"],
            ),
        );
        add(JavaSource::OfficialLauncher, join(var("APPDATA"), &[".minecraft", "runtime"]));
        add(JavaSource::OfficialLauncher, join(var("ProgramFiles(x86)"), &["Minecraft Launcher", "runtime"]));
        // A Prism installed by the user, then the one Outpost Tools keeps.
        add(JavaSource::Prism, join(var("APPDATA"), &["PrismLauncher", "java"]));
        add(JavaSource::Prism, Some(root.join("prism-data").join("java")));
        add(JavaSource::Prism, Some(root.join("prism").join("java")));
        add(JavaSource::OutpostTools, Some(java_dir(root)));
        Locations { java_home, path_dirs, scanned }
    }
}

// Where Outpost Tools puts the Java it downloads.
pub fn java_dir(root: &Path) -> PathBuf {
    root.join("java")
}

// Every Java the locations hold that a release file describes, once each, in order of preference.
pub fn find_all(locations: &Locations) -> Vec<JavaInstall> {
    let mut found: Vec<JavaInstall> = Vec::new();
    let mut add = |home: &Path, source: JavaSource| {
        let Some(install) = read_home(home, source) else { return };
        let key = |p: &Path| p.to_string_lossy().to_lowercase();
        if !found.iter().any(|f| key(&f.home) == key(&install.home)) {
            found.push(install);
        }
    };
    for home in &locations.java_home {
        add(home, JavaSource::JavaHome);
    }
    for dir in &locations.path_dirs {
        // A java.exe on the PATH is often a link into its home: follow it.
        let Some(real) = dir.join(JAVA_EXE).canonicalize().ok().map(strip_verbatim) else { continue };
        if let Some(home) = real.parent().and_then(Path::parent) {
            add(home, JavaSource::Path);
        }
    }
    for (source, dir) in &locations.scanned {
        let mut homes = Vec::new();
        scan_homes(dir, SCAN_DEPTH, &mut homes);
        for home in homes {
            add(&home, *source);
        }
    }
    found
}

// The Java to use: the first in the order of preference that is at least `min_major`.
pub fn find_java(locations: &Locations, min_major: u32) -> Option<JavaInstall> {
    find_all(locations).into_iter().find(|j| j.major >= min_major)
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(plain) => PathBuf::from(plain),
        None => path,
    }
}

// The homes below `dir`: folders holding bin\java.exe. A home is not searched inside.
fn scan_homes(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if dir.join("bin").join(JAVA_EXE).is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut subdirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subdirs.sort();
    for sub in subdirs {
        scan_homes(&sub, depth - 1, out);
    }
}

// A Java home read by its files alone: bin\java.exe must exist, and the release file gives the version. A home with
// no readable release file is skipped, not run to ask.
pub fn read_home(home: &Path, source: JavaSource) -> Option<JavaInstall> {
    let exe = home.join("bin").join(JAVA_EXE);
    if !exe.is_file() {
        return None;
    }
    let text = fs::read_to_string(home.join("release")).ok()?;
    let release = parse_release(&text)?;
    Some(JavaInstall {
        home: home.to_path_buf(),
        exe,
        version: release.version,
        major: release.major,
        image_type: release.image_type,
        source,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub major: u32,
    pub image_type: Option<String>,
}

// The `release` file: KEY="value" lines. JAVA_VERSION gives the version.
pub fn parse_release(text: &str) -> Option<Release> {
    let mut version = None;
    let mut image_type = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "JAVA_VERSION" => version = Some(value),
            "IMAGE_TYPE" => image_type = Some(value),
            _ => {}
        }
    }
    let version = version?;
    let major = major_of(&version)?;
    Some(Release { version, major, image_type })
}

// "25.0.5" and "25-ea" are 25; the old "1.8.0_401" is 8.
fn major_of(version: &str) -> Option<u32> {
    let number = |part: &str| {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    };
    let mut parts = version.split('.');
    let first = number(parts.next()?)?;
    if first == 1 { number(parts.next()?) } else { Some(first) }
}

// ---------------------------------------------------------------------------------------------------------------
// What to download

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Jre,
    Jdk,
}

impl ImageType {
    fn api_name(self) -> &'static str {
        match self {
            ImageType::Jre => "jre",
            ImageType::Jdk => "jdk",
        }
    }
}

// The newest Temurin build of a major version for Windows x64, as the vendor's API (Adoptium) lists it.
pub fn assets_url(major: u32, image: ImageType) -> String {
    format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type={}&os=windows&vendor=eclipse",
        image.api_name()
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaPackage {
    pub image: ImageType,
    // Such as "jdk-25.0.5.1+2".
    pub release_name: String,
    pub file_name: String,
    pub url: String,
    pub size: u64,
    // Lower-case hex SHA-256 of the zip.
    pub sha256: String,
}

// The package of an Adoptium answer: the zip of its first (newest) release. Ok(None) when the answer lists none.
pub fn parse_assets(json: &[u8], image: ImageType) -> Result<Option<JavaPackage>, String> {
    let value: Value = serde_json::from_slice(json).map_err(|e| format!("Adoptium's answer is not JSON: {e}"))?;
    let Some(first) = value.as_array().ok_or("Adoptium's answer is not a list")?.first() else { return Ok(None) };
    let package = &first["binary"]["package"];
    let text =
        |v: &Value, what: &str| v.as_str().map(str::to_string).ok_or_else(|| format!("Adoptium's answer has no {what}"));
    let file_name = text(&package["name"], "file name")?;
    if !file_name.ends_with(".zip") {
        return Err(format!("Adoptium's package is not a zip: {file_name}"));
    }
    Ok(Some(JavaPackage {
        image,
        release_name: text(&first["release_name"], "release name")?,
        file_name,
        url: text(&package["link"], "link")?,
        size: package["size"].as_u64().ok_or("Adoptium's answer has no size")?,
        sha256: text(&package["checksum"], "checksum")?.to_lowercase(),
    }))
}

#[derive(Debug)]
pub struct FetchError {
    pub status: Option<u16>,
    pub message: String,
}

// Where the bytes of a public page come from. The program's is `Http`; tests give saved answers.
pub trait Fetch {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}

pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    // Names the program and its version, and nothing of the PC or its user.
    pub fn new() -> Http {
        let agent =
            ureq::Agent::config_builder().user_agent(concat!("outpost-tools/", env!("CARGO_PKG_VERSION"))).build().into();
        Http { agent }
    }
}

impl Default for Http {
    fn default() -> Self {
        Http::new()
    }
}

fn fetch_error(error: ureq::Error) -> FetchError {
    let status = match &error {
        ureq::Error::StatusCode(code) => Some(*code),
        _ => None,
    };
    FetchError { status, message: error.to_string() }
}

impl Fetch for Http {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let mut response = self.agent.get(url).call().map_err(fetch_error)?;
        response.body_mut().read_to_vec().map_err(fetch_error)
    }
}

// What Outpost Tools would download for `major`: the JRE when the vendor has one, else the JDK. A JRE is a third of
// the size and is meant to hold what a game and a Fabric server run on (to validate by running the game on it); the
// JDK is the fallback. Only "no such package" falls back: a network error is reported.
pub fn plan_download(fetch: &dyn Fetch, major: u32) -> Result<JavaPackage, String> {
    for image in [ImageType::Jre, ImageType::Jdk] {
        match fetch.get(&assets_url(major, image)) {
            Ok(bytes) => {
                if let Some(package) = parse_assets(&bytes, image)? {
                    return Ok(package);
                }
            }
            Err(FetchError { status: Some(404), .. }) => {}
            Err(e) => return Err(format!("Adoptium could not be reached: {}", e.message)),
        }
    }
    Err(format!("Adoptium has no Windows x64 Java {major}"))
}

// ---------------------------------------------------------------------------------------------------------------
// Downloading, checking and extracting

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Downloading,
    Verifying,
    Extracting,
}

// How far a stage is, in bytes. Called at every chunk: the interface throttles what it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub done: u64,
    pub total: u64,
}

pub type OnProgress<'a> = &'a mut dyn FnMut(Progress);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// Copies a stream into a writer, counting the bytes, and returns the SHA-256 of what went through.
fn copy_hashed(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    stage: Stage,
    total: u64,
    on: OnProgress,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buffer).map_err(|e| format!("Reading failed: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        writer.write_all(&buffer[..n]).map_err(|e| format!("Writing failed: {e}"))?;
        done += n as u64;
        on(Progress { stage, done, total });
    }
    Ok(hex(hasher.finalize().as_slice()))
}

// Saves `package`'s zip to `dest`. This is the one call that downloads an archive, so no test runs it. The file is
// checked by `install_zip`, not here.
pub fn download(package: &JavaPackage, dest: &Path, on: OnProgress) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("The download folder could not be made: {e}"))?;
    }
    let mut response = Http::new().agent.get(&package.url).call().map_err(|e| format!("The download failed: {e}"))?;
    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(dest).map_err(|e| format!("The download file could not be made: {e}"))?;
    copy_hashed(&mut reader, &mut file, Stage::Downloading, package.size, on).map(|_| ())
}

fn sha256_of_file(path: &Path, on: OnProgress) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("The archive could not be opened: {e}"))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    copy_hashed(&mut file, &mut std::io::sink(), Stage::Verifying, total, on)
}

// Checks `zip_path` against `sha256`, extracts it into `<root>/java/` and returns the Java it holds. The zip's
// own top folder (Temurin's is "jdk-25.0.5.1+2-jre") becomes the Java's folder, replacing one of the same name.
// The work happens in `<root>/java/.partial`, removed at the end whatever happened.
pub fn install_zip(root: &Path, zip_path: &Path, sha256: &str, on: OnProgress) -> Result<JavaInstall, String> {
    let actual = sha256_of_file(zip_path, on)?;
    if !actual.eq_ignore_ascii_case(sha256) {
        return Err("The downloaded file does not match its checksum. It was not used.".to_string());
    }
    let java = java_dir(root);
    let staging = java.join(STAGING);
    let result = extract_into(&java, &staging, zip_path, on);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn extract_into(java: &Path, staging: &Path, zip_path: &Path, on: OnProgress) -> Result<JavaInstall, String> {
    let _ = fs::remove_dir_all(staging);
    fs::create_dir_all(staging).map_err(|e| format!("Outpost Tools' Java folder could not be made: {e}"))?;
    let file = File::open(zip_path).map_err(|e| format!("The archive could not be opened: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("The archive could not be read: {e}"))?;
    let mut total = 0u64;
    for i in 0..archive.len() {
        total += archive.by_index_raw(i).map_err(|e| format!("The archive could not be read: {e}"))?.size();
    }
    let mut done = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("The archive could not be read: {e}"))?;
        // enclosed_name refuses a path that climbs out of the folder or is absolute.
        let relative =
            entry.enclosed_name().ok_or_else(|| format!("The archive holds an unsafe path: {}", entry.name().map(|n| n.into_owned()).unwrap_or_default()))?;
        let target = staging.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("Extraction failed: {e}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Extraction failed: {e}"))?;
        }
        let mut out = File::create(&target).map_err(|e| format!("Extraction failed: {e}"))?;
        loop {
            let n = entry.read(&mut buffer).map_err(|e| format!("Extraction failed: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buffer[..n]).map_err(|e| format!("Extraction failed: {e}"))?;
            done += n as u64;
            on(Progress { stage: Stage::Extracting, done, total });
        }
    }
    let mut homes = Vec::new();
    scan_homes(staging, SCAN_DEPTH, &mut homes);
    let home = homes.first().ok_or("The archive holds no Java (no bin\\java.exe).")?;
    let name = match home.file_name() {
        Some(name) if home != staging => name.to_os_string(),
        _ => zip_path.file_stem().unwrap_or_default().to_os_string(),
    };
    let final_home = java.join(name);
    if final_home.exists() {
        fs::remove_dir_all(&final_home).map_err(|e| format!("The previous Java could not be replaced: {e}"))?;
    }
    fs::rename(home, &final_home).map_err(|e| format!("The Java could not be put in place: {e}"))?;
    read_home(&final_home, JavaSource::OutpostTools)
        .ok_or_else(|| "The archive's Java has no readable release file.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const JRE_JSON: &str = include_str!("../../tests/fixtures/adoptium-jre.json");
    const JDK_JSON: &str = include_str!("../../tests/fixtures/adoptium-jdk.json");

    // A scratch folder under the system's, removed when dropped.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Scratch {
            static COUNT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "outpost-java-test-{}-{}-{name}",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn release(version: &str) -> String {
        format!("IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"{version}\"\nIMAGE_TYPE=\"JRE\"\n")
    }

    // A fake home: bin\java.exe (not a program, just a file) and a release file.
    fn fake_home(dir: &Path, version: &str) -> PathBuf {
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin").join(JAVA_EXE), b"not a program").unwrap();
        fs::write(dir.join("release"), release(version)).unwrap();
        dir.to_path_buf()
    }

    // The zip a Temurin archive would be, tiny: a top folder with bin\java.exe, release and a file in lib.
    fn tiny_zip(path: &Path, top: &str, extra: Option<&str>) {
        let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer.add_directory(format!("{top}/"), options).unwrap();
        writer.start_file(format!("{top}/bin/java.exe"), options).unwrap();
        writer.write_all(b"not a program").unwrap();
        writer.start_file(format!("{top}/release"), options).unwrap();
        writer.write_all(release("25.0.5.1").as_bytes()).unwrap();
        writer.start_file(format!("{top}/lib/modules"), options).unwrap();
        writer.write_all(&vec![7u8; 200_000]).unwrap();
        if let Some(name) = extra {
            writer.start_file(name, options).unwrap();
            writer.write_all(b"outside").unwrap();
        }
        writer.finish().unwrap();
    }

    fn sha_of(path: &Path) -> String {
        sha256_of_file(path, &mut |_| {}).unwrap()
    }

    #[test]
    fn release_file_is_read() {
        let real = "IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"25.0.5\"\nJAVA_RUNTIME_VERSION=\"25.0.5+8-LTS\"\nIMAGE_TYPE=\"JDK\"\n";
        let parsed = parse_release(real).unwrap();
        assert_eq!((parsed.version.as_str(), parsed.major), ("25.0.5", 25));
        assert_eq!(parsed.image_type.as_deref(), Some("JDK"));
        assert_eq!(parse_release("JAVA_VERSION=\"1.8.0_401\"\n").unwrap().major, 8);
        assert_eq!(parse_release("JAVA_VERSION=\"26\"\n").unwrap().major, 26);
        assert_eq!(parse_release("JAVA_VERSION=\"27-ea\"\n").unwrap().major, 27);
        assert!(parse_release("IMPLEMENTOR=\"x\"\n").is_none());
        assert!(parse_release("").is_none());
    }

    #[test]
    fn a_home_needs_its_exe_and_its_release_file() {
        let s = Scratch::new("home");
        let home = fake_home(&s.0.join("ok"), "25.0.5");
        let install = read_home(&home, JavaSource::Prism).unwrap();
        assert_eq!((install.major, install.source), (25, JavaSource::Prism));
        assert_eq!(install.exe, home.join("bin").join(JAVA_EXE));
        fs::remove_file(home.join("release")).unwrap();
        assert!(read_home(&home, JavaSource::Prism).is_none());
        assert!(read_home(&s.0.join("missing"), JavaSource::Prism).is_none());
    }

    #[test]
    fn finds_the_first_java_25_in_order_and_says_where() {
        let s = Scratch::new("find");
        let old = fake_home(&s.0.join("jh-old"), "21.0.4");
        let official =
            fake_home(&s.0.join("official/runtime/java-runtime-delta/windows-x64/java-runtime-delta"), "25.0.1");
        let prism = fake_home(&s.0.join("prism-data/java/java-runtime-epsilon"), "25.0.3");
        let own = fake_home(&s.0.join("root/java/jdk-25.0.5.1+2-jre"), "25.0.5.1");
        let locations = Locations {
            java_home: vec![old.clone()],
            path_dirs: vec![s.0.join("nowhere")],
            scanned: vec![
                (JavaSource::OfficialLauncher, s.0.join("official/runtime")),
                (JavaSource::Prism, s.0.join("prism-data/java")),
                (JavaSource::OutpostTools, s.0.join("root/java")),
            ],
        };
        let sources: Vec<JavaSource> = find_all(&locations).iter().map(|j| j.source).collect();
        assert_eq!(
            sources,
            [JavaSource::JavaHome, JavaSource::OfficialLauncher, JavaSource::Prism, JavaSource::OutpostTools]
        );
        let best = find_java(&locations, MIN_MAJOR).unwrap();
        assert_eq!((best.source, best.home), (JavaSource::OfficialLauncher, official));
        // Without the launcher's, the next in order.
        let mut next = locations.clone();
        next.scanned.remove(0);
        assert_eq!(find_java(&next, MIN_MAJOR).unwrap().home, prism);
        // Only the old one: none, but find_all still lists it.
        let only_old = Locations { java_home: vec![old], ..Locations::default() };
        assert!(find_java(&only_old, MIN_MAJOR).is_none());
        assert_eq!(find_all(&only_old).len(), 1);
        // The root's own.
        let own_only =
            Locations { scanned: vec![(JavaSource::OutpostTools, s.0.join("root/java"))], ..Locations::default() };
        assert_eq!(find_java(&own_only, MIN_MAJOR).unwrap().home, own);
    }

    #[test]
    fn the_same_home_is_listed_once() {
        let s = Scratch::new("dup");
        let home = fake_home(&s.0.join("jdk"), "25.0.5");
        let locations = Locations {
            java_home: vec![home.clone()],
            path_dirs: vec![home.join("bin")],
            scanned: vec![(JavaSource::Prism, s.0.clone())],
        };
        let all = find_all(&locations);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].source, JavaSource::JavaHome);
    }

    #[test]
    fn from_env_names_the_roots_folders() {
        let locations = Locations::from_env(Path::new("R"));
        let folders = |source| -> Vec<PathBuf> {
            locations.scanned.iter().filter(|(s, _)| *s == source).map(|(_, p)| p.clone()).collect()
        };
        assert_eq!(folders(JavaSource::OutpostTools), [Path::new("R").join("java")]);
        assert!(folders(JavaSource::Prism).contains(&Path::new("R").join("prism-data").join("java")));
    }

    #[test]
    fn adoptium_answers_are_read() {
        let jre = parse_assets(JRE_JSON.as_bytes(), ImageType::Jre).unwrap().unwrap();
        assert_eq!(jre.release_name, "jdk-25.0.5.1+2");
        assert_eq!(jre.file_name, "OpenJDK25U-jre_x64_windows_hotspot_25.0.5.1_2.zip");
        assert!(jre.url.ends_with(&jre.file_name) && jre.url.starts_with("https://"));
        assert_eq!(jre.size, 58_599_439);
        assert_eq!(jre.sha256, "3260a1b9899f30ac80d737daafc053042c525249f52d066101fbf81d78d73f25");
        let jdk = parse_assets(JDK_JSON.as_bytes(), ImageType::Jdk).unwrap().unwrap();
        assert!(jdk.file_name.contains("-jdk_") && jdk.size > jre.size);
        assert_eq!(parse_assets(b"[]", ImageType::Jre).unwrap(), None);
        assert!(parse_assets(b"{}", ImageType::Jre).is_err());
        assert!(parse_assets(b"nope", ImageType::Jre).is_err());
    }

    #[test]
    fn assets_url_asks_for_windows_x64_temurin() {
        let url = assets_url(25, ImageType::Jre);
        for part in ["/latest/25/hotspot", "architecture=x64", "image_type=jre", "os=windows", "vendor=eclipse"] {
            assert!(url.contains(part), "{url}");
        }
    }

    struct Saved(Vec<(&'static str, Result<&'static str, u16>)>);
    impl Fetch for Saved {
        fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            let (_, answer) = self.0.iter().find(|(part, _)| url.contains(part)).expect("an unexpected request");
            match answer {
                Ok(body) => Ok(body.as_bytes().to_vec()),
                Err(status) => Err(FetchError { status: Some(*status), message: format!("status {status}") }),
            }
        }
    }

    #[test]
    fn the_jre_is_preferred_and_the_jdk_is_the_fallback() {
        let both = Saved(vec![("image_type=jre", Ok(JRE_JSON)), ("image_type=jdk", Ok(JDK_JSON))]);
        assert_eq!(plan_download(&both, 25).unwrap().image, ImageType::Jre);
        let no_jre = Saved(vec![("image_type=jre", Ok("[]")), ("image_type=jdk", Ok(JDK_JSON))]);
        assert_eq!(plan_download(&no_jre, 25).unwrap().image, ImageType::Jdk);
        let none = Saved(vec![("image_type=jre", Ok("[]")), ("image_type=jdk", Err(404))]);
        assert!(plan_download(&none, 25).is_err());
        // A server error is not "no JRE": it is reported, not worked around.
        let down = Saved(vec![("image_type=jre", Err(503))]);
        assert!(plan_download(&down, 25).unwrap_err().contains("503"));
    }

    #[test]
    fn a_zip_is_verified_extracted_and_reported() {
        let s = Scratch::new("zip");
        let zip_path = s.0.join("temurin.zip");
        tiny_zip(&zip_path, "jdk-25.0.5.1+2-jre", None);
        let root = s.0.join("root");
        let mut events: Vec<Progress> = Vec::new();
        let install = install_zip(&root, &zip_path, &sha_of(&zip_path), &mut |p| events.push(p)).unwrap();
        assert_eq!(install.home, root.join("java").join("jdk-25.0.5.1+2-jre"));
        assert_eq!((install.major, install.source), (25, JavaSource::OutpostTools));
        assert!(install.exe.is_file());
        assert!(root.join("java/jdk-25.0.5.1+2-jre/lib/modules").is_file());
        assert!(!root.join("java").join(STAGING).exists());
        // Verifying then extracting, each ending at its total.
        let last = |stage| events.iter().rev().find(|p| p.stage == stage).copied().unwrap();
        assert_eq!(last(Stage::Verifying).done, last(Stage::Verifying).total);
        assert_eq!(last(Stage::Extracting).done, last(Stage::Extracting).total);
        assert!(last(Stage::Extracting).total >= 200_000);
        // The root's java/ is now found by detection.
        let own = Locations { scanned: vec![(JavaSource::OutpostTools, java_dir(&root))], ..Locations::default() };
        assert_eq!(find_java(&own, 25).unwrap().home, install.home);
        // Extracting again replaces it.
        install_zip(&root, &zip_path, &sha_of(&zip_path), &mut |_| {}).unwrap();
    }

    #[test]
    fn a_wrong_checksum_extracts_nothing() {
        let s = Scratch::new("badsum");
        let zip_path = s.0.join("temurin.zip");
        tiny_zip(&zip_path, "jdk-x", None);
        let root = s.0.join("root");
        let error = install_zip(&root, &zip_path, &"0".repeat(64), &mut |_| {}).unwrap_err();
        assert!(error.contains("checksum"));
        assert!(!root.join("java").exists());
    }

    #[test]
    fn a_path_climbing_out_is_refused_and_leaves_nothing() {
        let s = Scratch::new("slip");
        let zip_path = s.0.join("evil.zip");
        tiny_zip(&zip_path, "jdk-x", Some("../evil.txt"));
        let root = s.0.join("root");
        let error = install_zip(&root, &zip_path, &sha_of(&zip_path), &mut |_| {}).unwrap_err();
        assert!(error.contains("unsafe"), "{error}");
        assert!(!root.join("java").join("evil.txt").exists() && !root.join("evil.txt").exists());
        assert!(!root.join("java").join(STAGING).exists());
        assert!(!root.join("java").join("jdk-x").exists());
    }

    #[test]
    fn a_zip_with_no_java_is_refused() {
        let s = Scratch::new("nojava");
        let zip_path = s.0.join("other.zip");
        let mut writer = zip::ZipWriter::new(File::create(&zip_path).unwrap());
        writer.start_file("readme.txt", zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(b"hello").unwrap();
        writer.finish().unwrap();
        let root = s.0.join("root");
        let error = install_zip(&root, &zip_path, &sha_of(&zip_path), &mut |_| {}).unwrap_err();
        assert!(error.contains("no Java"));
        assert!(!root.join("java").join(STAGING).exists());
    }

    #[test]
    fn a_stream_is_copied_counted_and_hashed() {
        let data = vec![1u8; 150_000];
        let mut out = Vec::new();
        let mut last = None;
        let hash = copy_hashed(&mut &data[..], &mut out, Stage::Downloading, 150_000, &mut |p| last = Some(p)).unwrap();
        assert_eq!(out, data);
        assert_eq!(last.unwrap(), Progress { stage: Stage::Downloading, done: 150_000, total: 150_000 });
        assert_eq!(hash.len(), 64);
        let mut again = Vec::new();
        assert_eq!(copy_hashed(&mut &data[..], &mut again, Stage::Downloading, 0, &mut |_| {}).unwrap(), hash);
    }

    // The real vendor answer, to see its shape has not moved. Network: run with --ignored.
    #[test]
    #[ignore]
    fn adoptium_still_answers_as_saved() {
        let package = plan_download(&Http::new(), 25).unwrap();
        assert_eq!(package.image, ImageType::Jre);
        assert!(package.size > 1_000_000 && package.sha256.len() == 64);
    }
}
