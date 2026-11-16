//! The form's values as last typed, kept in `<runs>/draft.json` so that a value typed and not yet generated is still
//! there at the next start. Without it the form opens on the last run's values and what was typed since is lost.

use std::path::Path;

use crate::json::Json;
use crate::settings::SETTINGS;

/// The values as one JSON object, a number per key (a switch is 1 or 0), as `run.json` holds them.
pub fn to_text(values: &[f64; 43]) -> String {
    let fields: Vec<String> = SETTINGS.iter().zip(values).map(|(s, v)| format!("\"{}\":{}", s.key, if *v == 0.0 { 0.0 } else { *v })).collect();
    format!("{{{}}}", fields.join(","))
}

/// What `to_text` wrote. A key the file lacks (a setting added since) takes `or`'s value; a file that is not an
/// object of numbers reads None.
pub fn from_text(text: &str, or: &[f64; 43]) -> Option<[f64; 43]> {
    let data = Json::parse(text).ok()?;
    if !matches!(data, Json::Obj(_)) {
        return None;
    }
    let mut out = *or;
    for (i, s) in SETTINGS.iter().enumerate() {
        if let Some(v) = data.get(s.key) {
            out[i] = v.num()?;
        }
    }
    Some(out)
}

pub fn load(path: &Path, or: &[f64; 43]) -> Option<[f64; 43]> {
    from_text(&std::fs::read_to_string(path).ok()?, or)
}

pub fn save(path: &Path, values: &[f64; 43]) -> std::io::Result<()> {
    std::fs::write(path, to_text(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::pack_values;

    #[test]
    fn what_is_written_reads_back_the_same() {
        let mut v = pack_values();
        v[0] = 0.07;
        v[11] = 0.0;
        assert_eq!(from_text(&to_text(&v), &pack_values()), Some(v));
    }

    #[test]
    fn a_missing_key_takes_the_fallback_and_a_bad_file_reads_nothing() {
        let v = from_text("{\"temperature_scale\":0.2}", &pack_values()).unwrap();
        assert_eq!(v[0], 0.2);
        assert_eq!(v[1..], pack_values()[1..]);
        assert_eq!(from_text("[1,2]", &pack_values()), None);
        assert_eq!(from_text("{\"temperature_scale\":\"wide\"}", &pack_values()), None);
        assert_eq!(from_text("not json", &pack_values()), None);
    }
}
