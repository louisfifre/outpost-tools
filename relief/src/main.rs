//! relief <data-dir> [--shot out.png --size WxH --scale S]
//!
//! Without --shot, opens a window. With it, draws one frame offscreen, writes the PNG and exits: the same
//! frame the window would show.

mod text;
mod view;

use mere_base::log;
use std::path::PathBuf;

struct Args {
    dir: PathBuf,
    shot: Option<PathBuf>,
    size: (u32, u32),
    scale: f32,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut dir = None;
    let mut shot = None;
    let mut size = (1280, 800);
    let mut scale = 1.0;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |what: &str| it.next().ok_or_else(|| format!("{what} needs a value"));
        match a.as_str() {
            "--shot" => shot = Some(PathBuf::from(value("--shot")?)),
            "--size" => {
                let v = value("--size")?;
                let (w, h) = v.split_once('x').ok_or("--size is WxH, like 1280x800")?;
                size = (
                    w.parse().map_err(|_| "--size width is not a number")?,
                    h.parse().map_err(|_| "--size height is not a number")?,
                );
                if size.0 == 0 || size.1 == 0 {
                    return Err("--size must not be zero".into());
                }
            }
            "--scale" => {
                scale = value("--scale")?.parse().map_err(|_| "--scale is not a number")?;
                if !(scale > 0.0) {
                    return Err("--scale must be above zero".into());
                }
            }
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => dir = Some(PathBuf::from(other)),
        }
    }
    Ok(Args { dir: dir.ok_or("no data folder given")?, shot, size, scale })
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&args).map_err(|e| format!("{e}\n{}", text::usage()))?;
    let world = preview::world::World::load(&args.dir)?;
    mere_base::observe!(Info, "loaded {} points, seed {}", world.heights.len(), world.seed);
    let mut app = view::Relief::new(world);
    match args.shot {
        Some(path) => {
            let (w, h) = args.size;
            let rgba = mere_win32::shot(&mut app, w, h, args.scale).map_err(|e| e.to_string())?;
            std::fs::write(&path, mere_png::encode(w, h, &rgba)).map_err(|e| format!("{}: {e}", path.display()))?;
            mere_base::observe!(Info, "wrote {}", path.display());
            Ok(())
        }
        None => mere_win32::run(text::WINDOW_TITLE, &mut app).map_err(|e| e.to_string()),
    }
}

fn main() {
    log::add_sink(log::stderr_sink(log::Level::Info));
    if let Err(e) = run() {
        eprintln!("relief: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn reads_the_options() {
        let a = parse_args(&args(r"out --shot a.png --size 640x480 --scale 2")).unwrap();
        assert_eq!(a.size, (640, 480));
        assert_eq!(a.scale, 2.0);
        assert!(a.shot.is_some());
        let b = parse_args(&args("dir")).unwrap();
        assert_eq!((b.size, b.scale, b.shot.is_none()), ((1280, 800), 1.0, true));
    }

    #[test]
    fn refuses_what_it_cannot_use() {
        assert!(parse_args(&args("--shot a.png")).is_err());
        assert!(parse_args(&args("d --size 12")).is_err());
        assert!(parse_args(&args("d --scale 0")).is_err());
        assert!(parse_args(&args("d --what")).is_err());
    }
}
