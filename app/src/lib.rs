// The launcher's program as a library: the answers to the page's requests (`api`) and what they read from Windows
// (`system`). launcher/app's own binary puts a window of tao and wry in front of them; launcher/native puts its own
// native window in front of the same answers.

pub mod api;
pub mod icon;
pub mod system;

use std::path::{Path, PathBuf};

// The repository the program was built in: the first folder above it with scripts\common.ps1.
pub fn find_repo() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|dir| dir.join("scripts").join("common.ps1").exists()).map(Path::to_path_buf)
}
