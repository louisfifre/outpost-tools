// The launcher application: one window showing launcher/index.html, and everything the page asks for (the
// profiles, the dev server, the world preview), routed by api.rs to one file per part in api/.
//
// The window has no frame of its own: the page draws the whole bar, its title, its tools and the buttons to
// minimise, maximise and close. The page asks for what only the window can do through window.ipc.postMessage, one
// word per message, and hears back where the window stands through window.shell. The page reaches api.rs through
// a protocol of the window's own, http://launcher.localhost/, so no port is open: nothing else on the machine
// can ask anything.
//
// Usage:
//   launcher                  the window
//   launcher --serve <port>   no window: the page and its answers on http://localhost:<port>, for launcher/shots.ps1;
//                             it stops 90 s after the last request
//   launcher --devtools       the window, with the browser's tools on F12
#![windows_subsystem = "windows"]

use std::{
    borrow::Cow,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use launcher::{api::Api, find_repo, system};
use tao::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Icon, ResizeDirection, Window, WindowBuilder},
};
use wry::{
    WebContext, WebViewBuilder,
    http::{Response, header},
};

// The size under which the preview has no room: its map in the middle and a panel on each side.
const MIN_SIZE: (u32, u32) = (960, 600);
const PAGE_ORIGIN: &str = "http://launcher.localhost";

fn main() {
    let mut serve = None;
    let mut repo = None;
    let mut devtools = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--serve" => serve = args.next().and_then(|p| p.parse::<u16>().ok()),
            "--repo" => repo = args.next().map(PathBuf::from),
            "--devtools" => devtools = true,
            _ => {}
        }
    }
    let repo = repo.or_else(find_repo).unwrap_or_else(|| PathBuf::from("."));
    match serve {
        Some(port) => serve_http(Api::new(repo, format!("http://localhost:{port}")), port),
        None => open_window(Api::new(repo, PAGE_ORIGIN.into()), devtools),
    }
}

// ---------- the window ----------

// What the page asks, and what the window answers.
enum Ask {
    Page(String),
    // The page's title, which the taskbar shows: where a run of the preview stands.
    Title(String),
}

fn open_window(api: Api, devtools: bool) {
    let data = api.repo.join("run").join("app");
    let _ = fs::create_dir_all(&data);
    // Opening the launcher while it runs shows the window already open rather than a second one.
    if !system::only_one(&data) {
        return;
    }
    let place_file = data.join("window.txt");
    api.write_log("started");
    let api = Arc::new(api);

    let event_loop = EventLoopBuilder::<Ask>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let title_proxy = event_loop.create_proxy();

    // No frame, but the shadow and the rounded corners Windows gives a window: it still looks like one.
    let window = WindowBuilder::new()
        .with_title("Outpost Tools")
        .with_decorations(false)
        .with_undecorated_shadow(true)
        // In the page's own pixels, which a screen at 200 % doubles: the page's 960 is the screen's 1920 there.
        .with_min_inner_size(LogicalSize::new(MIN_SIZE.0, MIN_SIZE.1))
        .with_inner_size(LogicalSize::new(1280, 880))
        .with_window_icon(Some(cube_icon()))
        .with_visible(false)
        .build(&event_loop)
        .expect("the window could not be made");
    restore_place(&window, &place_file);
    let _ = fs::write(data.join("hwnd.txt"), (window.hwnd() as isize).to_string());
    window.set_visible(true);

    // The browser's profile (storage, cache) lives in the launcher's own folder.
    let mut context = WebContext::new(Some(data.join("webview")));
    let protocol_api = api.clone();
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(format!("{PAGE_ORIGIN}/"))
        .with_background_color((0, 0, 0, 255))
        .with_devtools(devtools)
        .with_asynchronous_custom_protocol("launcher".into(), move |_, request, responder| {
            // Each request on a thread of its own: reading the processes takes a moment, and the window must not wait.
            let api = protocol_api.clone();
            std::thread::spawn(move || {
                let origin = request.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok()).map(String::from);
                let reply = api.handle(request.method().as_str(), request.uri().path(), origin.as_deref(), request.body());
                let response = Response::builder()
                    .status(reply.status)
                    .header(header::CONTENT_TYPE, reply.kind)
                    // Profiles and launches change in place: never an old copy.
                    .header(header::CACHE_CONTROL, "no-store")
                    .body(Cow::Owned(reply.body))
                    .unwrap();
                responder.respond(response);
            });
        })
        .with_ipc_handler(move |request| {
            let _ = proxy.send_event(Ask::Page(request.body().clone()));
        })
        .with_document_title_changed_handler(move |title| {
            let _ = title_proxy.send_event(Ask::Title(title));
        })
        // A link that would open a window opens in the person's own browser instead.
        .with_new_window_req_handler(|url, _| {
            if url.starts_with("http://") || url.starts_with("https://") {
                let _ = system::start("explorer.exe", &[&url]);
            }
            wry::NewWindowResponse::Deny
        })
        .build(&window)
        .expect("the page could not be shown");

    let mut maximized = window.is_maximized();
    // A close the page was asked about: a second one within a few seconds closes anyway, so a page that no longer
    // answers never keeps the window open.
    let mut close_asked: Option<Instant> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Ask::Title(title)) => window.set_title(&title),
            Event::UserEvent(Ask::Page(word)) => match word.as_str() {
                "minimize" => window.set_minimized(true),
                "maximize" => window.set_maximized(!window.is_maximized()),
                "close" => {
                    save_place(&window, &place_file);
                    api.write_log("stopped: the window was closed");
                    *control_flow = ControlFlow::Exit;
                }
                "drag" => {
                    let _ = window.drag_window();
                }
                "ready" => tell(&webview, window.is_maximized()),
                other => {
                    if let Some(edge) = other.strip_prefix("resize:").and_then(direction) {
                        let _ = window.drag_resize_window(edge);
                    }
                }
            },
            // Alt+F4 and the taskbar's Close ask the page first: it may have a map being computed.
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if close_asked.is_some_and(|at| at.elapsed().as_secs() < 3) {
                    save_place(&window, &place_file);
                    api.write_log("stopped: the window was closed");
                    *control_flow = ControlFlow::Exit;
                } else {
                    close_asked = Some(Instant::now());
                    let _ = webview.evaluate_script("window.shell ? window.shell.closeRequested() : window.ipc.postMessage('close')");
                }
            }
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } => {
                let now = window.is_maximized();
                if now != maximized {
                    maximized = now;
                    tell(&webview, now);
                }
            }
            _ => {}
        }
    });
}

// Tells the page where the window stands; its maximise button turns into "restore" and back.
fn tell(webview: &wry::WebView, maximized: bool) {
    let _ = webview.evaluate_script(&format!("window.shell && window.shell.state({{ maximized: {maximized} }})"));
}

fn direction(edge: &str) -> Option<ResizeDirection> {
    Some(match edge {
        "n" => ResizeDirection::North,
        "s" => ResizeDirection::South,
        "e" => ResizeDirection::East,
        "w" => ResizeDirection::West,
        "ne" => ResizeDirection::NorthEast,
        "nw" => ResizeDirection::NorthWest,
        "se" => ResizeDirection::SouthEast,
        "sw" => ResizeDirection::SouthWest,
        _ => return None,
    })
}

// The window opens where it was left: its place and size, or maximised. Kept as one line of numbers.
fn save_place(window: &Window, file: &PathBuf) {
    if window.is_maximized() {
        // The size to come back to is the one before maximising: keep the line already there.
        if let Ok(line) = fs::read_to_string(file) {
            let mut parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 5 {
                parts[4] = "1";
                let _ = fs::write(file, parts.join(" "));
                return;
            }
        }
    }
    let (Ok(at), size) = (window.outer_position(), window.inner_size()) else { return };
    let _ = fs::write(file, format!("{} {} {} {} {}", at.x, at.y, size.width, size.height, window.is_maximized() as u8));
}

fn restore_place(window: &Window, file: &PathBuf) {
    let Ok(line) = fs::read_to_string(file) else { return };
    let n: Vec<i64> = line.split_whitespace().filter_map(|p| p.parse().ok()).collect();
    let [x, y, w, h, max] = n[..] else { return };
    // Only a place still on one of the screens: a screen unplugged since would leave the window out of sight.
    let on_screen = window.available_monitors().any(|m| {
        let (p, s) = (m.position(), m.size());
        x + 40 >= p.x as i64 && y >= p.y as i64 && x + 40 < p.x as i64 + s.width as i64 && y + 40 < p.y as i64 + s.height as i64
    });
    // The saved size is in the screen's pixels; the window's least size holds it back if it is too small.
    if w > 0 && h > 0 {
        window.set_inner_size(PhysicalSize::new(w as u32, h as u32));
    }
    if on_screen {
        window.set_outer_position(PhysicalPosition::new(x as i32, y as i32));
    }
    if max == 1 {
        window.set_maximized(true);
    }
}

// The page's own icon for the taskbar, from the pixels the library draws (the native launcher shows the same ones).
fn cube_icon() -> Icon {
    let (rgba, side) = launcher::icon::cube_rgba();
    Icon::from_rgba(rgba, side, side).expect("the icon could not be made")
}

// ---------- without a window, for captures ----------

// The same answers over plain HTTP on localhost, so a headless browser can take pictures of the page
// (launcher/shots.ps1). One request per connection, which is all a page needs here.
fn serve_http(api: Api, port: u16) {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("the port is taken");
    let api = Arc::new(api);
    let last = Arc::new(Mutex::new(Instant::now()));
    api.write_log(&format!("serving on port {port}"));
    {
        let last = last.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                *last.lock().unwrap() = Instant::now();
                let api = api.clone();
                std::thread::spawn(move || {
                    let _ = answer(&api, stream);
                });
            }
        });
    }
    // The page pings every 10 s: a server nobody asks anything stops by itself.
    while last.lock().unwrap().elapsed() < Duration::from_secs(90) {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn answer(api: &Api, stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("/").to_string());
    let (mut length, mut origin) = (0usize, None);
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "origin" => origin = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }
    let mut body = vec![0; length.min(1 << 20)];
    reader.read_exact(&mut body)?;
    let reply = api.handle(&method, &path, origin.as_deref(), &body);
    let mut out = stream;
    write!(
        out,
        "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.kind,
        reply.body.len()
    )?;
    out.write_all(&reply.body)
}
