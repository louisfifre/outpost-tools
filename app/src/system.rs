// What the launcher reads from Windows: the processes running (Prism, the game, the dev server) and whether one
// of them shows a window yet, and how it starts programs, with or without a console of their own.

use std::{
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, Command},
};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible},
};

// A program started by the launcher shows no console unless it is meant to be read.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub struct Proc {
    pub pid: u32,
    pub parent: Option<u32>,
    // Lower case, as Windows compares names and paths.
    pub name: String,
    pub cmd: String,
    // Seconds since 1970.
    pub started: u64,
}

// Every process with its command line: one look at the machine, taken each time a question needs it.
pub fn processes(system: &mut System) -> Vec<Proc> {
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always));
    system
        .processes()
        .values()
        .map(|p| Proc {
            pid: p.pid().as_u32(),
            parent: p.parent().map(|q| q.as_u32()),
            name: p.name().to_string_lossy().to_lowercase(),
            cmd: p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ").to_lowercase(),
            started: p.start_time(),
        })
        .collect()
}

// Whether a process shows a window of its own on screen: what .NET calls its main window, a visible window that
// belongs to no other. The game has one once it has loaded.
pub fn has_window(pid: u32) -> bool {
    struct Search {
        pid: u32,
        found: bool,
    }
    unsafe extern "system" fn each(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut owner_pid = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut owner_pid) };
        if owner_pid == search.pid
            && unsafe { IsWindowVisible(window) } != 0
            && unsafe { GetWindow(window, GW_OWNER) }.is_null()
        {
            search.found = true;
            return 0;
        }
        1
    }
    let mut search = Search { pid, found: false };
    unsafe { EnumWindows(Some(each), &mut search as *mut Search as LPARAM) };
    search.found
}

// A program with no window of its own (the preview's server, a command sent to the dev server).
pub fn hidden(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

// A program in a console window of its own, where its log scrolls and its commands are typed: the dev server.
pub fn in_console(program: &str, args: &[&str], dir: &Path) -> std::io::Result<Child> {
    Command::new(program).args(args).current_dir(dir).creation_flags(CREATE_NEW_CONSOLE).spawn()
}

// A program shown as the person would start it: a folder in the Explorer, the game through Prism.
pub fn start(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> std::io::Result<()> {
    Command::new(program).args(args).spawn().map(|_| ())
}
