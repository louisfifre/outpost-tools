//! The product's name: the program, the window's title and the log lines say Outpost Tools. No process is started:
//! the program's file name is the one cargo gives it at build time.

mod common;

use std::path::Path;

#[test]
fn the_program_is_outpost_tools() {
    let exe = Path::new(env!("CARGO_BIN_EXE_outpost-tools"));
    assert_eq!(exe.file_stem().and_then(|s| s.to_str()), Some("outpost-tools"), "{exe:?}");
    assert_eq!(env!("CARGO_PKG_NAME"), "outpost-tools");
}

#[test]
fn the_window_and_the_taskbar_say_outpost_tools() {
    assert_eq!(preview::text::BAR_TITLE, "Outpost Tools");
    // Before any run of the preview, the title the window is given is the bar's: the name alone.
    let rig = common::Rig::with_api(std::sync::Arc::new(outpost_tools::fake::fixture()), 1400, 940, true, false);
    assert_eq!(preview::bar::window_title(rig.app.state()), "Outpost Tools");
}

#[test]
fn the_log_lines_say_outpost_tools() {
    // main.rs writes these three lines; the source is read so a line cannot go back to another name unseen.
    let main = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs")).unwrap();
    for line in ["Outpost Tools started (native)", "start refused: Outpost Tools already runs", "Outpost Tools stopped: the window was closed"] {
        assert!(main.contains(line), "{line}");
    }
    assert!(!main.contains("launcher-native"));
}
