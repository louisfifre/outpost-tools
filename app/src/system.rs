// What the launcher reads from Windows: the processes running (Prism, the game, the dev server) and whether one
// of them shows a window yet, and how it starts programs, with or without a console of their own.

use std::{
    fs,
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

// True for the first launcher. A second one brings the first one's window to the front and gives up: Windows
// keeps a named lock while the first runs, and the first wrote where its window is. The name is the same for both
// launchers (the page's and the native one): they never run together.
pub fn only_one(data: &Path) -> bool {
    use windows_sys::Win32::{
        Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
        System::Threading::CreateMutexW,
        UI::WindowsAndMessaging::{IsIconic, IsWindow, SW_RESTORE, SetForegroundWindow, ShowWindow},
    };
    let name: Vec<u16> = "Local\\outpost-tools\0".encode_utf16().collect();
    // The lock is never released by hand: Windows lets it go when this process ends.
    let lock = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if lock.is_null() || unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return true;
    }
    if let Some(hwnd) = fs::read_to_string(data.join("hwnd.txt")).ok().and_then(|t| t.trim().parse::<isize>().ok()) {
        let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
        unsafe {
            if IsWindow(hwnd) != 0 {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                SetForegroundWindow(hwnd);
            }
        }
    }
    false
}

// The local calendar day, as days since 1970-01-01, of a time in milliseconds since 1970: what the page's
// `new Date(t).setHours(0, 0, 0, 0)` sees. Windows converts the instant to the zone's wall clock; the day count is
// then plain calendar arithmetic (Howard Hinnant's days_from_civil). A time Windows cannot convert counts as UTC.
pub fn local_day(ms: i64) -> i64 {
    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    };
    // FILETIME counts 100 ns from 1601-01-01; the gap to 1970 is 11,644,473,600 s.
    let ticks = (ms.max(0) as u64) * 10_000 + 116_444_736_000_000_000;
    let file = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let (mut utc, mut local) = (unsafe { std::mem::zeroed::<SYSTEMTIME>() }, unsafe { std::mem::zeroed::<SYSTEMTIME>() });
    let ok = unsafe { FileTimeToSystemTime(&file, &mut utc) != 0 && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0 };
    if !ok {
        return ms.div_euclid(86_400_000);
    }
    days_from_civil(local.wYear as i64, local.wMonth as i64, local.wDay as i64)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_days_count_from_1970() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 11, 15), 20_772);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }

    #[test]
    fn the_local_day_is_within_a_day_of_the_utc_one() {
        let ms = 1_795_262_400_000;
        assert!((local_day(ms) - ms.div_euclid(86_400_000)).abs() <= 1);
    }
}