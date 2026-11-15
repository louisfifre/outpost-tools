// Files of key=value lines: Prism's instance.cfg for the profiles, the dev server's server.properties.

use std::{fs, path::Path};

// Prism's instance.cfg, one key=value per line. A value that holds quotes, commas or backslashes must be wrapped in
// quotes and escaped, or Prism reads it wrong (it silently drops bare quotes, which glues a command line together).
pub(super) struct Cfg {
    lines: Vec<String>,
}

impl Cfg {
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
