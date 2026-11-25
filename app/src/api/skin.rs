// The account's skin, read from Mojang with no sign-in: name -> UUID -> profile -> `textures` value -> skin PNG.
// Kept under Outpost Tools' folder (`<root>/cache/skins`), so a second read works with no connection. An account with
// no skin of its own gets the game's default skin for its UUID, read from the installed client jar, never embedded
// (Mojang's usage guidelines forbid redistributing the game's textures; docs/research/launcher.md in the pack, 8-9).
// The player's mesh is in `mesh`. Nothing here draws, signs in or reads a token.

pub mod mesh;

use std::{
    fs, io,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{Value, json};

#[derive(Debug, PartialEq)]
pub enum FetchError {
    /// The server answered that there is nothing there (404, or 204 with no body).
    NotFound,
    /// Anything else: no connection, a timeout, a refusal (rate limit included).
    Unreachable(String),
}

/// Where bytes come from. Tests give fixtures; the program gives `Http`.
pub trait Fetch {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}

pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    pub fn new() -> Http {
        let config = ureq::Agent::config_builder()
            .user_agent(concat!("outpost-tools/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(10)))
            .build();
        Http { agent: config.into() }
    }
}

impl Default for Http {
    fn default() -> Http {
        Http::new()
    }
}

impl Fetch for Http {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        match self.agent.get(url).call() {
            Ok(mut reply) => {
                let bytes = reply.body_mut().read_to_vec().map_err(|e| FetchError::Unreachable(e.to_string()))?;
                if bytes.is_empty() { Err(FetchError::NotFound) } else { Ok(bytes) }
            }
            Err(ureq::Error::StatusCode(404)) => Err(FetchError::NotFound),
            Err(e) => Err(FetchError::Unreachable(e.to_string())),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SkinError {
    InvalidName,
    NoSuchAccount,
    /// Mojang could not be reached and nothing is kept for this name.
    Offline(String),
    BadData(String),
    Io(String),
    /// The account has no skin of its own and the game's client jar is not known or lacks the default skin.
    NoDefaultSkin(String),
}

impl std::fmt::Display for SkinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkinError::InvalidName => write!(f, "not a Minecraft account name"),
            SkinError::NoSuchAccount => write!(f, "no account has this name"),
            SkinError::Offline(why) => write!(f, "Mojang could not be reached: {why}"),
            SkinError::BadData(why) => write!(f, "unexpected answer: {why}"),
            SkinError::Io(why) => write!(f, "cannot read or write the skin cache: {why}"),
            SkinError::NoDefaultSkin(why) => write!(f, "no default skin: {why}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub name: String,
    /// 32 lowercase hex digits, no dashes, as Mojang writes it.
    pub uuid: String,
    pub slim: bool,
    /// The PNG file as is: 64x64, or the old 64x32.
    pub png: Vec<u8>,
    /// True when the account has no skin of its own and `png` is the game's default one.
    pub is_default: bool,
}

/// Mojang's names are 3 to 16 of A-Z a-z 0-9 _ ; the check also keeps the name safe as a file name.
fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Reads the skin from Mojang and keeps it. If Mojang cannot be reached, the kept one is returned; if none is
/// kept either, `Offline`. `client_jar` is the game's client jar, only used for an account with no skin of its own.
pub fn load(root: &Path, name: &str, fetch: &dyn Fetch, client_jar: Option<&Path>) -> Result<Skin, SkinError> {
    if !valid_name(name) {
        return Err(SkinError::InvalidName);
    }
    // Any "unreachable" answer, the skin's own download included, falls back to the kept copy.
    match fetch_profile(name, fetch).and_then(|profile| {
        let png = match &profile.skin_url {
            Some(url) => Some(fetch.get(url).map_err(|e| match e {
                FetchError::NotFound => SkinError::BadData("the skin's address leads nowhere".into()),
                FetchError::Unreachable(why) => SkinError::Offline(why),
            })?),
            None => None,
        };
        if let Some(png) = &png {
            png_size(png)?;
        }
        Ok((profile, png))
    }) {
        Ok((profile, png)) => {
            store(root, &profile, png.as_deref())?;
            assemble(profile, png, client_jar)
        }
        Err(SkinError::Offline(why)) => cached(root, name, client_jar).ok_or(SkinError::Offline(why)),
        Err(e) => Err(e),
    }
}

/// What a previous `load` kept for this name, with no network. None if nothing is kept.
pub fn cached(root: &Path, name: &str, client_jar: Option<&Path>) -> Option<Skin> {
    if !valid_name(name) {
        return None;
    }
    let meta: Value = serde_json::from_slice(&fs::read(cache_file(root, name, "json")).ok()?).ok()?;
    let profile = Profile {
        name: meta["name"].as_str()?.to_string(),
        uuid: meta["uuid"].as_str()?.to_string(),
        slim: meta["slim"].as_bool()?,
        skin_url: None,
    };
    let png = if meta["own_skin"].as_bool()? {
        let png = fs::read(cache_file(root, name, "png")).ok()?;
        // A pair torn by a crash between the two renames does not match: nothing is kept, rather than a wrong skin.
        if meta["png_sum"].as_str()? != checksum(&png) {
            return None;
        }
        Some(png)
    } else {
        None
    };
    assemble(profile, png, client_jar).ok()
}

struct Profile {
    name: String,
    uuid: String,
    slim: bool,
    skin_url: Option<String>,
}

fn assemble(profile: Profile, png: Option<Vec<u8>>, client_jar: Option<&Path>) -> Result<Skin, SkinError> {
    let is_default = png.is_none();
    let (png, slim) = match png {
        Some(png) => (png, profile.slim),
        None => {
            let jar = client_jar.ok_or_else(|| SkinError::NoDefaultSkin("the game's client jar is not known".into()))?;
            read_default_skin(jar, &profile.uuid)?
        }
    };
    Ok(Skin { name: profile.name, uuid: profile.uuid, slim, png, is_default })
}

fn fetch_profile(name: &str, fetch: &dyn Fetch) -> Result<Profile, SkinError> {
    let by_name = fetch_json(fetch, &format!("https://api.mojang.com/users/profiles/minecraft/{name}"))?;
    let uuid = by_name["id"].as_str().ok_or_else(|| SkinError::BadData("no id in the name's answer".into()))?;
    let uuid = normalize_uuid(uuid).ok_or_else(|| SkinError::BadData("the id is not a UUID".into()))?;
    let profile = fetch_json(fetch, &format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"))?;
    parse_profile(&profile, uuid)
}

fn fetch_json(fetch: &dyn Fetch, url: &str) -> Result<Value, SkinError> {
    let bytes = fetch.get(url).map_err(|e| match e {
        FetchError::NotFound => SkinError::NoSuchAccount,
        FetchError::Unreachable(why) => SkinError::Offline(why),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| SkinError::BadData(e.to_string()))
}

/// The session server's profile: its `textures` property is base64 of a JSON that holds the skin's URL and, for
/// slim arms, `"metadata": {"model": "slim"}` (https://minecraft.wiki/w/Mojang_API, read 2026-11-25).
fn parse_profile(profile: &Value, uuid: String) -> Result<Profile, SkinError> {
    let name = profile["name"].as_str().ok_or_else(|| SkinError::BadData("no name in the profile".into()))?;
    let textures = profile["properties"]
        .as_array()
        .and_then(|props| props.iter().find(|p| p["name"] == "textures"))
        .and_then(|p| p["value"].as_str());
    let (skin_url, slim) = match textures {
        None => (None, false),
        Some(value) => {
            let decoded = base64_decode(value).ok_or_else(|| SkinError::BadData("textures is not base64".into()))?;
            let json: Value = serde_json::from_slice(&decoded).map_err(|e| SkinError::BadData(e.to_string()))?;
            let skin = &json["textures"]["SKIN"];
            // The texture server answers on https as well; the profile writes http.
            let url = skin["url"].as_str().map(|u| u.replacen("http://", "https://", 1));
            (url, skin["metadata"]["model"] == "slim")
        }
    };
    Ok(Profile { name: name.to_string(), uuid, slim, skin_url })
}

fn cache_file(root: &Path, name: &str, ext: &str) -> PathBuf {
    root.join("cache").join("skins").join(format!("{}.{ext}", name.to_ascii_lowercase()))
}

/// FNV-1a, enough to tell a PNG from the one the metadata was written for.
fn checksum(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash = (hash ^ *b as u64).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Both files are written under temporary names, then renamed in; the metadata holds the PNG's checksum, so a crash
/// between the two renames is noticed by `cached` and never gives a skin with the wrong arms.
fn store(root: &Path, profile: &Profile, png: Option<&[u8]>) -> Result<(), SkinError> {
    let io = |e: io::Error| SkinError::Io(e.to_string());
    fs::create_dir_all(root.join("cache").join("skins")).map_err(io)?;
    let png_file = cache_file(root, &profile.name, "png");
    let json_file = cache_file(root, &profile.name, "json");
    let png_tmp = png_file.with_extension("png.tmp");
    let json_tmp = json_file.with_extension("json.tmp");
    let meta = json!({
        "name": profile.name, "uuid": profile.uuid, "slim": profile.slim, "own_skin": png.is_some(),
        "png_sum": png.map(checksum),
    });
    let result = (|| {
        if let Some(bytes) = png {
            fs::write(&png_tmp, bytes)?;
        }
        fs::write(&json_tmp, meta.to_string())?;
        match png {
            Some(_) => fs::rename(&png_tmp, &png_file)?,
            None => {
                let _ = fs::remove_file(&png_file);
            }
        }
        fs::rename(&json_tmp, &json_file)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&png_tmp);
        let _ = fs::remove_file(&json_tmp);
    }
    result.map_err(io)
}

/// (width, height) of a PNG, read from its header alone. A skin is 64x64 or the old 64x32.
pub fn png_size(png: &[u8]) -> Result<(u32, u32), SkinError> {
    if png.len() < 24 || png[..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] || &png[12..16] != b"IHDR" {
        return Err(SkinError::BadData("not a PNG".into()));
    }
    let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
    match (w, h) {
        (64, 64) | (64, 32) => Ok((w, h)),
        _ => Err(SkinError::BadData(format!("a skin is 64x64 or 64x32, this is {w}x{h}"))),
    }
}

// ---- default skins ----

/// The nine default skins' file names, alphabetical.
const DEFAULT_NAMES: [&str; 9] = ["alex", "ari", "efe", "kai", "makena", "noor", "steve", "sunny", "zuri"];

/// The UUID's 32 hex digits, lowercase, from the form with or without dashes.
pub fn normalize_uuid(text: &str) -> Option<String> {
    let hex = text.replace('-', "").to_ascii_lowercase();
    (hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// Which default skin an account with no skin of its own gets: (file name, slim arms).
/// TO VALIDATE: the rule is `floorMod(uuid.hashCode(), 18)` into the game's list of 18 (slim ones first, then wide
/// ones, each in the order of DEFAULT_NAMES), as remembered from the game's code (DefaultPlayerSkin, 1.20.2 and
/// after). No source was read for it: neither minecraft.wiki's Skin page (read 2026-11-25) nor this session's
/// tools gave it. Check against the decompiled client, or an account known to get a given default skin.
pub fn default_skin_for(uuid: &str) -> Option<(&'static str, bool)> {
    let hex = normalize_uuid(uuid)?;
    let msb = u64::from_str_radix(&hex[..16], 16).ok()?;
    let lsb = u64::from_str_radix(&hex[16..], 16).ok()?;
    // java.util.UUID.hashCode: the two halves xored, then the two 32-bit halves of that xored.
    let hilo = msb ^ lsb;
    let hash = ((hilo >> 32) as u32 ^ hilo as u32) as i32;
    let index = hash.rem_euclid(18) as usize;
    Some((DEFAULT_NAMES[index % 9], index < 9))
}

/// The path of a default skin inside the client jar.
fn default_skin_entry(name: &str, slim: bool) -> String {
    format!("assets/minecraft/textures/entity/player/{}/{name}.png", if slim { "slim" } else { "wide" })
}

/// The default skin for this UUID and its arms, read from the client jar.
pub fn read_default_skin(client_jar: &Path, uuid: &str) -> Result<(Vec<u8>, bool), SkinError> {
    let (name, slim) = default_skin_for(uuid).ok_or_else(|| SkinError::BadData("the id is not a UUID".into()))?;
    let no = |why: String| SkinError::NoDefaultSkin(why);
    let file = fs::File::open(client_jar).map_err(|e| no(format!("{}: {e}", client_jar.display())))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| no(e.to_string()))?;
    let entry = default_skin_entry(name, slim);
    let mut member = zip.by_name(&entry).map_err(|e| no(format!("{entry}: {e}")))?;
    let mut png = Vec::new();
    member.read_to_end(&mut png).map_err(|e| no(e.to_string()))?;
    png_size(&png)?;
    Ok((png, slim))
}

/// The newest client jar in a Prism `libraries` folder (`com/mojang/minecraft/<v>/minecraft-<v>-client.jar`).
/// Versions are compared as dotted numbers.
pub fn find_client_jar(libraries: &Path) -> Option<PathBuf> {
    let key = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let base = libraries.join("com").join("mojang").join("minecraft");
    let mut best: Option<(Vec<u64>, PathBuf)> = None;
    for dir in fs::read_dir(base).ok()?.flatten() {
        let version = dir.file_name().to_string_lossy().into_owned();
        let jar = dir.path().join(format!("minecraft-{version}-client.jar"));
        if jar.is_file() && best.as_ref().is_none_or(|(k, _)| key(&version) > *k) {
            best = Some((key(&version), jar));
        }
    }
    best.map(|(_, jar)| jar)
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for b in text.trim_end_matches('=').bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'\r' | b'\n' => continue,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
