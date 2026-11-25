// Files of key=value lines: Prism's instance.cfg for the profiles, the dev server's server.properties.

use std::{fs, path::Path};

// Prism's instance.cfg, one key=value per line. A value that holds quotes, commas or backslashes must be wrapped in
// quotes and escaped, or Prism reads it wrong (it silently drops bare quotes, which glues a command line together).
pub(super) struct Cfg {
    lines: Vec<String>,
}

impl Cfg {
    // An empty file: just the [General] section Prism keeps its settings in.
    pub(super) fn new() -> Cfg {
        Cfg { lines: vec!["[General]".to_string()] }
    }
    pub(super) fn read(file: &Path) -> Cfg {
        Cfg { lines: read_lines(file) }
    }
    pub(super) fn get(&self, key: &str) -> Option<String> {
        let raw = find(&self.lines, key)?;
        let raw = &self.lines[raw][key.len() + 1..];
        if raw.len() < 2 || !(raw.starts_with('"') && raw.ends_with('"')) {
            return Some(raw.to_string());
        }
        let mut out = String::new();
        let mut chars = raw[1..raw.len() - 1].chars();
        while let Some(c) = chars.next() {
            out.push(if c == '\\' { chars.next().unwrap_or('\\') } else { c });
        }
        Some(out)
    }
    pub(super) fn number(&self, key: &str) -> i64 {
        self.get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
    }
    pub(super) fn set(&mut self, key: &str, value: &str) {
        let plain = !value.contains(['"', ',', ';', '\\'])
            && !value.starts_with(char::is_whitespace)
            && !value.ends_with(char::is_whitespace);
        let value = if plain { value.to_string() } else { format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")) };
        let line = format!("{key}={value}");
        match find(&self.lines, key) {
            Some(at) => self.lines[at] = line,
            None => {
                let at = self.lines.iter().position(|l| l == "[General]").map(|i| i + 1).unwrap_or(0);
                self.lines.insert(at, line);
            }
        }
    }
    pub(super) fn write(&self, file: &Path) -> std::io::Result<()> {
        write_lines(file, &self.lines)
    }
}

// The dev server's server.properties: plain key=value lines.
#[derive(Default)]
pub(super) struct Props {
    lines: Vec<String>,
}

impl Props {
    pub(super) fn read(file: &Path) -> Option<Props> {
        file.exists().then(|| Props { lines: read_lines(file) })
    }
    pub(super) fn get(&self, key: &str) -> Option<String> {
        find(&self.lines, key).map(|at| self.lines[at][key.len() + 1..].to_string())
    }
    pub(super) fn set(&mut self, key: &str, value: &str) {
        let line = format!("{key}={value}");
        match find(&self.lines, key) {
            Some(at) => self.lines[at] = line,
            None => self.lines.push(line),
        }
    }
    pub(super) fn write(&self, file: &Path) -> std::io::Result<()> {
        write_lines(file, &self.lines)
    }
}

fn find(lines: &[String], key: &str) -> Option<usize> {
    let prefix = format!("{key}=");
    lines.iter().position(|l| l.starts_with(&prefix))
}

fn read_lines(file: &Path) -> Vec<String> {
    let bytes = fs::read(file).unwrap_or_default();
    String::from_utf8_lossy(strip_bom(&bytes)).lines().map(String::from).collect()
}

// Lines as Windows writes them, as the scripts did.
fn write_lines(file: &Path, lines: &[String]) -> std::io::Result<()> {
    let mut text = lines.join("\r\n");
    text.push_str("\r\n");
    fs::write(file, text)
}

pub(super) fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("key_value_{name}_{}", std::process::id()));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn cfg_reads_plain_and_quoted_values() {
        let path = file("read", b"\xEF\xBB\xBF[General]\r\nname=My pack\r\nJvmArgs=\"-Da=\\\"b c\\\" -Dd=e\\\\f\"\r\nMaxMemAlloc=4096\r\nempty=\r\n");
        let cfg = Cfg::read(&path);
        assert_eq!(cfg.get("name").as_deref(), Some("My pack"));
        assert_eq!(cfg.get("JvmArgs").as_deref(), Some(r#"-Da="b c" -Dd=e\f"#));
        assert_eq!(cfg.number("MaxMemAlloc"), 4096);
        assert_eq!(cfg.number("name"), 0);
        assert_eq!(cfg.get("empty").as_deref(), Some(""));
        assert_eq!(cfg.get("missing"), None);
        // A key is a whole key, not the start of a longer one.
        assert_eq!(cfg.get("Max"), None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn cfg_quotes_what_prism_would_read_wrong_and_reads_it_back() {
        let path = file("write", b"InstanceType=OneSix\r\n[General]\r\nname=Pack\r\n");
        let mut cfg = Cfg::read(&path);
        cfg.set("name", "Other");
        cfg.set("JvmArgs", r#"-Da="b,c" -Dd=e\f"#);
        cfg.set("Padded", " x ");
        cfg.write(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        // A new key goes right under [General], where Prism keeps its settings.
        assert_eq!(text, "InstanceType=OneSix\r\n[General]\r\nPadded=\" x \"\r\nJvmArgs=\"-Da=\\\"b,c\\\" -Dd=e\\\\f\"\r\nname=Other\r\n");
        let again = Cfg::read(&path);
        assert_eq!(again.get("JvmArgs").as_deref(), Some(r#"-Da="b,c" -Dd=e\f"#));
        assert_eq!(again.get("Padded").as_deref(), Some(" x "));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn props_are_plain_and_a_new_key_goes_last() {
        let path = file("props", b"#Minecraft server properties\nmotd=A \"quoted\" line\nserver-port=25565\n");
        let mut props = Props::read(&path).unwrap();
        assert_eq!(props.get("motd").as_deref(), Some("A \"quoted\" line"));
        props.set("server-port", "25566");
        props.set("white-list", "true");
        props.write(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "#Minecraft server properties\r\nmotd=A \"quoted\" line\r\nserver-port=25566\r\nwhite-list=true\r\n");
        let _ = fs::remove_file(&path);
        assert!(Props::read(&path).is_none());
    }
}
