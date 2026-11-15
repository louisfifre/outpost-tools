// The launcher's program as a library: the answers to the page's requests (`api`) and what they read from Windows
// (`system`). app's own binary puts a window of tao and wry in front of them; native puts its own native window in
// front of the same answers.

pub mod api;
pub mod icon;
pub mod system;

use std::path::{Path, PathBuf};

// The pack repository the launcher drives, when --repo does not name it: the first folder above the program with
// scripts\common.ps1. It holds in a prod copy, where Outpost Tools is built inside the pack's snapshot; anywhere
// else the pack's own scripts pass --repo.
pub fn find_repo() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|dir| dir.join("scripts").join("common.ps1").exists()).map(Path::to_path_buf)
}

// Outpost Tools' own folder: the first folder above the program that holds the page, index.html. The page is read
// from there at each load, so a change to it needs no rebuild.
pub fn find_home() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|dir| dir.join("index.html").exists()).map(Path::to_path_buf)
}
