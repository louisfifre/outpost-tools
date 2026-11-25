use std::{cell::RefCell, collections::HashMap, io::Write, path::PathBuf};

use super::*;

const JEB_ID: &str = "853c80ef3c3749fdaa49938b674adae6";
const JEB_PROFILE: &str = include_str!("../../../tests/fixtures/profile_jeb.json");
const JEB_SKIN_URL: &str =
    "https://textures.minecraft.net/texture/7fd9ba42a7c81eeea22f1524271ae85a8e045ce0af5a6ae16c6406ae917e68b5";

/// A PNG header with the given size: enough for `png_size`; no image data.
fn fake_png(w: u32, h: u32, tag: u8) -> Vec<u8> {
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    png.extend(w.to_be_bytes());
    png.extend(h.to_be_bytes());
    png.push(tag);
    png
}

/// Answers from a table, counts the requests, and can be switched off like a lost connection.
struct Fake {
    answers: HashMap<String, Vec<u8>>,
    online: RefCell<bool>,
    asked: RefCell<Vec<String>>,
    /// Requests to this one address fail like a lost connection, the others answer.
    down_url: RefCell<Option<String>>,
}

impl Fake {
    fn jeb() -> Fake {
        let mut answers = HashMap::new();
        answers.insert(
            "https://api.mojang.com/users/profiles/minecraft/jeb_".into(),
            format!(r#"{{"id":"{JEB_ID}","name":"jeb_"}}"#).into_bytes(),
        );
        answers.insert(
            format!("https://sessionserver.mojang.com/session/minecraft/profile/{JEB_ID}"),
            JEB_PROFILE.as_bytes().to_vec(),
        );
        answers.insert(JEB_SKIN_URL.into(), fake_png(64, 64, 1));
        Fake { answers, online: RefCell::new(true), asked: RefCell::new(Vec::new()), down_url: RefCell::new(None) }
    }
}

impl Fetch for Fake {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        self.asked.borrow_mut().push(url.to_string());
        if !*self.online.borrow() || self.down_url.borrow().as_deref() == Some(url) {
            return Err(FetchError::Unreachable("offline".into()));
        }
        self.answers.get(url).cloned().ok_or(FetchError::NotFound)
    }
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("outpost-skin-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A jar with the 18 default skins, each PNG tagged by its entry name's length and arms so they differ.
fn fake_jar(dir: &Path) -> PathBuf {
    let path = dir.join("minecraft-test-client.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for slim in [false, true] {
        for (i, name) in DEFAULT_NAMES.iter().enumerate() {
            zip.start_file(default_skin_entry(name, slim), options).unwrap();
            zip.write_all(&fake_png(64, 64, i as u8 + if slim { 100 } else { 0 })).unwrap();
        }
    }
    zip.finish().unwrap();
    path
}

#[test]
fn reads_the_skin_by_name_with_no_sign_in_and_keeps_it() {
    let root = temp("read");
    let fake = Fake::jeb();
    let skin = load(&root, "jeb_", &fake, None).unwrap();
    assert_eq!((skin.name.as_str(), skin.uuid.as_str()), ("jeb_", JEB_ID));
    assert_eq!(skin.png, fake_png(64, 64, 1));
    assert!(!skin.slim && !skin.is_default);
    // Three requests, all to public endpoints, none carrying anything but the name or UUID asked.
    assert_eq!(fake.asked.borrow().len(), 3);
    assert!(root.join("cache/skins/jeb_.png").is_file() && root.join("cache/skins/jeb_.json").is_file());
}

#[test]
fn a_second_read_works_offline() {
    let root = temp("offline");
    let fake = Fake::jeb();
    let first = load(&root, "jeb_", &fake, None).unwrap();
    *fake.online.borrow_mut() = false;
    assert_eq!(load(&root, "JEB_", &fake, None).unwrap(), first);
    assert_eq!(cached(&root, "jeb_", None).unwrap(), first);
}

#[test]
fn a_failed_png_download_falls_back_to_the_kept_copy() {
    let root = temp("png-down");
    let fake = Fake::jeb();
    let first = load(&root, "jeb_", &fake, None).unwrap();
    *fake.down_url.borrow_mut() = Some(JEB_SKIN_URL.into());
    assert_eq!(load(&root, "jeb_", &fake, None).unwrap(), first);
    let empty = temp("png-down-empty");
    assert!(matches!(load(&empty, "jeb_", &fake, None), Err(SkinError::Offline(_))));
}

#[test]
fn a_torn_pair_is_not_kept_and_no_temporary_file_stays() {
    let root = temp("torn");
    let fake = Fake::jeb();
    load(&root, "jeb_", &fake, None).unwrap();
    let dir = root.join("cache").join("skins");
    let leftovers = fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "tmp"));
    assert_eq!(leftovers.count(), 0);
    // A crash between the two renames: a new PNG next to the old metadata.
    fs::write(dir.join("jeb_.png"), fake_png(64, 64, 9)).unwrap();
    assert_eq!(cached(&root, "jeb_", None), None);
}

#[test]
fn offline_with_nothing_kept_says_offline() {
    let root = temp("none");
    let fake = Fake::jeb();
    *fake.online.borrow_mut() = false;
    assert!(matches!(load(&root, "jeb_", &fake, None), Err(SkinError::Offline(_))));
    assert_eq!(cached(&root, "jeb_", None), None);
}

#[test]
fn unknown_name_and_unsafe_names() {
    let root = temp("names");
    let fake = Fake::jeb();
    assert_eq!(load(&root, "nobody", &fake, None), Err(SkinError::NoSuchAccount));
    for bad in ["", "../x", "a b", "seventeen_chars_xx", "jeb_/.."] {
        assert_eq!(load(&root, bad, &fake, None), Err(SkinError::InvalidName), "{bad}");
    }
    assert!(!root.join("cache").exists());
}

#[test]
fn slim_arms_come_from_the_profile_metadata() {
    let value = br#"{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc","metadata":{"model":"slim"}}}}"#;
    let encode = |bytes: &[u8]| {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
            for k in 0..=chunk.len() {
                out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
            }
            out.extend(std::iter::repeat_n('=', 3 - chunk.len()));
        }
        out
    };
    let profile = json!({"name": "x", "properties": [{"name": "textures", "value": encode(value)}]});
    let p = parse_profile(&profile, JEB_ID.into()).unwrap();
    assert!(p.slim);
    assert_eq!(p.skin_url.as_deref(), Some("https://textures.minecraft.net/texture/abc"));
    assert!(!parse_profile(&serde_json::from_str(JEB_PROFILE).unwrap(), JEB_ID.into()).unwrap().slim);
}

#[test]
fn base64_round_trips_the_real_fixture() {
    let profile: Value = serde_json::from_str(JEB_PROFILE).unwrap();
    let decoded = base64_decode(profile["properties"][0]["value"].as_str().unwrap()).unwrap();
    let json: Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(json["profileName"], "jeb_");
}

#[test]
fn rejects_a_skin_that_is_not_a_png_of_the_right_size() {
    assert!(png_size(b"nope").is_err());
    assert!(png_size(&fake_png(128, 128, 0)).is_err());
    assert_eq!(png_size(&fake_png(64, 32, 0)), Ok((64, 32)));
}

#[test]
fn default_skin_index_follows_the_uuid_hash() {
    // UUID.hashCode of all zeros is 0; with lsb = 1 it is 1; low half all ones is -1, floorMod 18 = 17.
    assert_eq!(default_skin_for("00000000-0000-0000-0000-000000000000"), Some(("alex", true)));
    assert_eq!(default_skin_for("00000000000000000000000000000001"), Some(("ari", true)));
    assert_eq!(default_skin_for("000000000000000000000000ffffffff"), Some(("zuri", false)));
    // The halves are xored: equal halves hash to 0.
    assert_eq!(default_skin_for("12345678123456781234567812345678"), Some(("alex", true)));
    assert_eq!(default_skin_for("not a uuid"), None);
}

#[test]
fn an_account_with_no_skin_gets_the_default_skin_from_the_game_jar() {
    let root = temp("default");
    let jar = fake_jar(&root);
    let id = "00000000000000000000000000000001"; // ari, slim
    let mut fake = Fake::jeb();
    fake.answers.insert("https://api.mojang.com/users/profiles/minecraft/plain".into(), format!(r#"{{"id":"{id}"}}"#).into_bytes());
    fake.answers.insert(
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}"),
        format!(r#"{{"id":"{id}","name":"plain","properties":[]}}"#).into_bytes(),
    );
    let skin = load(&root, "plain", &fake, Some(&jar)).unwrap();
    assert!(skin.is_default && skin.slim);
    assert_eq!(skin.png, fake_png(64, 64, 1 + 100));
    // Offline, from the kept uuid and the jar.
    *fake.online.borrow_mut() = false;
    assert_eq!(load(&root, "plain", &fake, Some(&jar)).unwrap(), skin);
    // Without the jar: an error that says so, not a made-up skin.
    assert!(matches!(cached(&root, "plain", None), None));
    assert!(matches!(load(&root, "plain", &{ *fake.online.borrow_mut() = true; fake }, None), Err(SkinError::NoDefaultSkin(_))));
}

#[test]
fn finds_the_newest_client_jar() {
    let root = temp("libs");
    for v in ["1.9", "26.3", "26.10"] {
        let dir = root.join("com/mojang/minecraft").join(v);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("minecraft-{v}-client.jar")), b"x").unwrap();
    }
    fs::create_dir_all(root.join("com/mojang/minecraft/99.0")).unwrap(); // no jar inside
    assert!(find_client_jar(&root).unwrap().ends_with("26.10/minecraft-26.10-client.jar"));
    assert_eq!(find_client_jar(&root.join("nowhere")), None);
}

/// Live check of the real endpoints and the shape of the answer; not part of the normal run.
#[test]
#[ignore = "needs the network"]
fn live_jeb() {
    let root = temp("live");
    let skin = load(&root, "jeb_", &Http::new(), None).unwrap();
    assert_eq!(skin.uuid, JEB_ID);
    assert!(png_size(&skin.png).is_ok());
}
