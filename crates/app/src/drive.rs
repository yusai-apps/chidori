//! A test run of the real app that nobody has to click (`--drive <dir>`, in builds with the
//! `drive` feature; docs/DRIVE.md). The window stays out of sight and takes its input from
//! `<dir>/in.jsonl`, one JSON command a line, answering each with a line in `<dir>/out.jsonl`.
//! Clicks, keys and typing go into GPUI as its own input events, so they pass through the same
//! hit testing and handlers as a hand's; the window draws itself to a PNG on request; and a
//! recording hears sound files in place of the devices. Its settings, sessions and logs live in
//! `<dir>/profile`, and the tray, the global hotkeys, the update check and the single-instance
//! rule are left out, so it runs beside an installed Chidori without touching it.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chidori_core::audio::StandIn;
use chidori_core::host::{AppHandle, Paths};
use chidori_core::models::catalog;
use chidori_core::ops;
use gpui_kit::{
    AnyWindowHandle, AsyncApp, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    PlatformInput, Point, ScrollDelta, ScrollEvent, Window, point, px, size,
};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::app::{Message, WindowEvent};
use crate::bridge::{self, UiEvent};
use crate::native::menu;
use crate::ui::{self, Cx};

const IN: &str = "in.jsonl";
const OUT: &str = "out.jsonl";

fn flag(name: &str) -> bool {
    std::env::args().skip(1).any(|a| a == name)
}

/// The folder of `--drive <dir>`.
pub fn dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            if a == "--drive" {
                return args.next().and_then(|d| std::path::absolute(d).ok());
            }
        }
        None
    })
    .as_deref()
}

/// Whether this run is driven.
pub fn on() -> bool {
    dir().is_some()
}

/// Whether the window shows: always, unless driven, when only `--drive-visible` shows it (without
/// taking the focus), for watching a run. On Linux a driven window always shows: X draws nothing
/// for a hidden one, so `shot` reads the shown window back (`x11_shot`). Runs on CI and in the
/// cloud put it on a virtual display (Xvfb), where nobody sees it.
pub fn visible() -> bool {
    static VISIBLE: OnceLock<bool> = OnceLock::new();
    *VISIBLE.get_or_init(|| !on() || flag("--drive-visible") || cfg!(target_os = "linux"))
}

/// The profile was made by this run, so it has yet to be set up.
static FRESH: AtomicBool = AtomicBool::new(false);

/// The app's folders in the driven profile. A new profile gets links to the models already on
/// this PC, so no test downloads one.
pub fn paths() -> Option<Paths> {
    let base = dir()?.join("profile");
    let system = Paths::system();
    if !base.exists() {
        FRESH.store(true, Ordering::SeqCst);
        let models = base.join("local").join("models");
        if let Err(e) = std::fs::create_dir_all(&models) {
            eprintln!("drive: {}: {e}", models.display());
        }
        for entry in std::fs::read_dir(system.local_data.join("models")).into_iter().flatten().flatten() {
            let from = entry.path();
            if from.extension().is_some_and(|e| e == "bin") {
                let to = models.join(entry.file_name());
                // A link, so deleting a model in a test leaves the user's own file.
                if std::fs::hard_link(&from, &to).is_err() {
                    let _ = std::fs::copy(&from, &to);
                }
            }
        }
    }
    Some(Paths {
        config: base.join("config"),
        local_data: base.join("local"),
        logs: base.join("logs"),
        resources: system.resources,
        documents: base.join("documents"),
    })
}

/// Before the app starts: a new profile is past the setup wizard on the smallest model there is
/// (tests want speed, not accuracy), and recordings hear silence until a `sound` command.
pub fn seed(core: &AppHandle) {
    if !on() {
        return;
    }
    let st = core.state();
    *st.stand_in.lock() = Some(StandIn::default());
    if FRESH.load(Ordering::SeqCst) {
        let smallest =
            catalog::all().iter().filter(|e| e.path_in(&st.models_dir).exists()).min_by_key(|e| e.size_bytes);
        if let Some(model) = smallest {
            ops::set_settings(core, &json!({ "modelId": model.id }));
            ops::complete_setup(core);
        }
    }
}

/// The popup menu the app last opened. A driven run shows no native menu (it would open on the
/// real screen and wait for a real click); `state` lists its items and `menu` picks one.
static POPUP: Mutex<Vec<menu::Item>> = Mutex::new(Vec::new());

/// Keeps a popup menu for `state` to list; `false` when the run is not driven.
pub fn popup(items: &[menu::Item]) -> bool {
    if on() {
        *POPUP.lock() = items.to_vec();
    }
    on()
}

fn menu_json(items: &[menu::Item]) -> Value {
    items
        .iter()
        .map(|item| match item {
            menu::Item::Action { id, label, enabled } => json!({ "id": id, "label": label, "enabled": enabled }),
            menu::Item::Check { id, label, checked, enabled } => {
                json!({ "id": id, "label": label, "enabled": enabled, "checked": checked })
            }
            menu::Item::Separator => json!("-"),
            menu::Item::Submenu { label, items, enabled } => {
                json!({ "label": label, "enabled": enabled, "items": menu_json(items) })
            }
        })
        .collect()
}

/// Reads commands until the app quits.
pub async fn serve(window: AnyWindowHandle, cx: &mut AsyncApp) {
    let Some(dir) = dir() else { return };
    let _ = std::fs::remove_file(dir.join(OUT));
    let (tx, mut rx) = unbounded_channel();
    let commands = dir.join(IN);
    if let Err(e) = std::thread::Builder::new().name("drive".into()).spawn(move || tail(&commands, &tx)) {
        tracing::error!("drive: {e}");
        return;
    }
    if !visible() {
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                if cx.update(|cx| window.update(cx, |_, window, cx| frame(window, cx))).is_err() {
                    break;
                }
            }
        })
        .detach();
    }
    answer(dir, &json!({ "n": 0, "ready": true, "pid": std::process::id() }));
    let mut n = 0u64;
    while let Some(line) = rx.recv().await {
        n += 1;
        let mut out = match serde_json::from_str::<Value>(&line) {
            Ok(cmd) => step(&cmd, window, cx).await.unwrap_or_else(|e| json!({ "ok": false, "error": e })),
            Err(e) => json!({ "ok": false, "error": format!("not JSON: {e}") }),
        };
        out["n"] = n.into();
        answer(dir, &out);
        // What the command set off (queued messages, a core call's quick answer) lands before the
        // next one.
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
}

fn answer(dir: &Path, line: &Value) {
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(OUT))
        .and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = written {
        tracing::error!("drive: {OUT}: {e}");
    }
}

/// Sends each whole line appended to the command file, from its start (so a script written
/// before the launch runs). A file that shrank was written anew.
fn tail(path: &Path, tx: &UnboundedSender<String>) {
    let mut offset = 0u64;
    let mut pending = Vec::new();
    loop {
        if let Ok(mut f) = std::fs::File::open(path) {
            let len = f.metadata().map_or(0, |m| m.len());
            if len < offset {
                offset = 0;
                pending.clear();
            }
            if len > offset && f.seek(SeekFrom::Start(offset)).is_ok() {
                let mut fresh = Vec::new();
                if let Ok(read) = f.read_to_end(&mut fresh) {
                    offset += read as u64;
                    pending.extend(fresh);
                }
            }
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = pending.drain(..=end).collect();
                let line = String::from_utf8_lossy(&line).trim().to_string();
                if !line.is_empty() && tx.send(line).is_err() {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

async fn step(cmd: &Value, window: AnyWindowHandle, cx: &mut AsyncApp) -> Result<Value, String> {
    let name = cmd["do"].as_str().ok_or("a command needs \"do\"")?;
    if name == "wait" {
        let ms = cmd["ms"].as_u64().ok_or("wait needs \"ms\"")?;
        cx.background_executor().timer(Duration::from_millis(ms)).await;
        return Ok(json!({ "ok": true }));
    }
    cx.update(|cx| window.update(cx, |_, window, cx| act(name, cmd, window, cx))).map_err(|e| e.to_string())?
}

/// How often a hidden window draws: the system asks a window for frames only while it shows, and
/// the app's motion advances once a frame, so without these it would stand still.
const FRAME: Duration = Duration::from_millis(16);

/// One frame of a hidden window, as the system would ask for it: what waits for the next frame
/// runs, then the window lays out and paints (to nowhere).
fn frame(window: &mut Window, cx: &mut Cx) {
    window.simulate_next_frame(cx);
    window.draw(cx).clear(cx);
}

/// Lays out and paints the window as it now is, so input lands on what a screenshot would show.
/// A visible window is then marked for drawing again: this frame was not shown, and its own next
/// frame must not take it for shown.
fn draw(window: &mut Window, cx: &mut Cx) {
    window.draw(cx).clear(cx);
    if visible() {
        window.refresh();
    }
}

fn number(cmd: &Value, key: &str) -> Result<f32, String> {
    cmd[key].as_f64().map(|v| v as f32).ok_or_else(|| format!("needs a number \"{key}\""))
}

fn button(cmd: &Value) -> MouseButton {
    match cmd["button"].as_str() {
        Some("right") => MouseButton::Right,
        Some("middle") => MouseButton::Middle,
        _ => MouseButton::Left,
    }
}

fn mouse_move(at: Point<Pixels>, pressed: Option<MouseButton>, window: &mut Window, cx: &mut Cx) {
    let event = MouseMoveEvent { position: at, pressed_button: pressed, ..Default::default() };
    window.dispatch_event(PlatformInput::MouseMove(event), cx);
}

fn mouse_down(at: Point<Pixels>, button: MouseButton, count: usize, window: &mut Window, cx: &mut Cx) {
    let event = MouseDownEvent { button, position: at, click_count: count, ..Default::default() };
    window.dispatch_event(PlatformInput::MouseDown(event), cx);
}

fn mouse_up(at: Point<Pixels>, button: MouseButton, count: usize, window: &mut Window, cx: &mut Cx) {
    let event = MouseUpEvent { button, position: at, click_count: count, ..Default::default() };
    window.dispatch_event(PlatformInput::MouseUp(event), cx);
}

fn act(name: &str, cmd: &Value, window: &mut Window, cx: &mut Cx) -> Result<Value, String> {
    let dir = dir().ok_or("not driven")?;
    // Positions are pixels of a screenshot; GPUI's are logical.
    let scale = window.scale_factor();
    let at = |x: &str, y: &str| Ok::<_, String>(point(px(number(cmd, x)? / scale), px(number(cmd, y)? / scale)));
    let ok = || Ok(json!({ "ok": true }));
    let core = ui::read_app(cx, |app| app.core.clone()).ok_or("the app is gone")?;
    match name {
        "shot" => {
            draw(window, cx);
            #[cfg(not(target_os = "linux"))]
            let image = window.render_to_image().map_err(|e| format!("{e:#}"))?;
            // GPUI draws to an image only on Windows and macOS.
            #[cfg(target_os = "linux")]
            let image = x11_shot(window)?;
            let file = dir.join(cmd["file"].as_str().unwrap_or("shot.png"));
            image.save(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            Ok(json!({ "ok": true, "file": file, "width": image.width(), "height": image.height(), "scale": scale }))
        }
        "state" => {
            draw(window, cx);
            let state = ui::read_app(cx, state).ok_or("the app is gone")?;
            Ok(json!({ "ok": true, "state": state }))
        }
        "click" => {
            draw(window, cx);
            let (p, b) = (at("x", "y")?, button(cmd));
            mouse_move(p, None, window, cx);
            // A double click is two presses in a row, the second one counted as such.
            for count in 1..=cmd["count"].as_u64().unwrap_or(1).max(1) as usize {
                mouse_down(p, b, count, window, cx);
                mouse_up(p, b, count, window, cx);
            }
            ok()
        }
        "move" => {
            draw(window, cx);
            mouse_move(at("x", "y")?, None, window, cx);
            ok()
        }
        "drag" => {
            draw(window, cx);
            let (from, to, b) = (at("x", "y")?, at("to_x", "to_y")?, button(cmd));
            mouse_move(from, None, window, cx);
            mouse_down(from, b, 1, window, cx);
            const STEPS: usize = 8;
            for i in 1..=STEPS {
                let t = i as f32 / STEPS as f32;
                mouse_move(from + (to - from) * t, Some(b), window, cx);
            }
            mouse_up(to, b, 1, window, cx);
            ok()
        }
        "scroll" => {
            draw(window, cx);
            let by = point(px(cmd["dx"].as_f64().unwrap_or(0.0) as f32 / scale), px(number(cmd, "dy")? / scale));
            let event = ScrollEvent { position: at("x", "y")?, delta: ScrollDelta::Pixels(by), ..Default::default() };
            window.dispatch_event(PlatformInput::Scroll(event), cx);
            ok()
        }
        "key" => {
            draw(window, cx);
            let os = cx.platform_info().os;
            for key in cmd["keys"].as_str().ok_or("key needs \"keys\"")?.split_whitespace() {
                let stroke = Keystroke::parse_with_os(key, os).map_err(|e| format!("{key}: {e}"))?;
                window.dispatch_keystroke(stroke, cx);
            }
            ok()
        }
        "type" => {
            draw(window, cx);
            for c in cmd["text"].as_str().ok_or("type needs \"text\"")?.chars() {
                let stroke = Keystroke { modifiers: Modifiers::default(), key: c.into(), key_char: Some(c.into()) };
                window.dispatch_keystroke(stroke, cx);
            }
            ok()
        }
        "menu" => {
            bridge::send(UiEvent::Menu(cmd["id"].as_str().ok_or("menu needs \"id\"")?.into()));
            POPUP.lock().clear();
            ok()
        }
        "tray" => {
            bridge::send(UiEvent::TrayClick);
            ok()
        }
        "hotkey" => {
            let name = cmd["name"].as_str().ok_or("hotkey needs \"name\"")?;
            for _ in 0..cmd["times"].as_u64().unwrap_or(1) {
                if !crate::native::hotkeys::press(name, &core) {
                    return Err(format!("no hotkey {name}"));
                }
            }
            ok()
        }
        "window" => {
            let event = match cmd["event"].as_str() {
                Some("hidden") => WindowEvent::Hidden(true),
                Some("shown") => WindowEvent::Hidden(false),
                Some("close") => WindowEvent::CloseRequested,
                Some("focused") => WindowEvent::Focused,
                Some("drop") => WindowEvent::FileDropped(cmd["path"].as_str().ok_or("drop needs \"path\"")?.into()),
                _ => return Err("window needs \"event\": hidden, shown, close, focused or drop".into()),
            };
            ui::send(Message::WindowEvent(event), cx);
            ok()
        }
        "resize" => {
            window.resize(size(px(number(cmd, "w")?), px(number(cmd, "h")?)));
            ok()
        }
        "settings" => {
            ui::send(Message::UpdateSettings(cmd["set"].clone()), cx);
            ok()
        }
        "sound" => {
            let file = |key: &str| cmd[key].as_str().map(PathBuf::from);
            let files = StandIn { others: file("others"), me: file("me") };
            if let Some(missing) = [&files.others, &files.me].into_iter().flatten().find(|p| !p.exists()) {
                return Err(format!("no file {}", missing.display()));
            }
            *core.state().stand_in.lock() = Some(files);
            ok()
        }
        "quit" => {
            cx.quit();
            ok()
        }
        other => Err(format!("no command {other}")),
    }
}

/// What the app shows, in words: for a script to wait on and check, where a screenshot would
/// need eyes.
fn state(app: &crate::app::App) -> Value {
    let live = &app.live;
    let items = &app.main.list.items;
    let partials: Vec<Value> =
        live.partials.iter().map(|(source, line)| json!({ "source": source, "text": line.text })).collect();
    json!({
        "view": format!("{:?}", app.view),
        "wizard": app.needs_wizard(),
        "compact": app.compact(),
        "hidden": app.hidden,
        "recording": {
            "state": app.rec.state,
            "sessionId": app.rec.session_id,
            "elapsedMs": app.rec.elapsed(std::time::Instant::now()),
            "deferred": app.rec.deferred,
            "engine": app.rec.engine,
            "lastSavedFolder": app.rec.last_saved_folder,
        },
        "live": {
            "sessionId": live.session_id,
            "title": live.title,
            "folder": live.folder,
            "durationMs": live.duration_ms,
            "items": live.items,
            "partials": partials,
            "scroll": {
                "following": items.is_following_tail(),
                "top": -f32::from(items.scroll_px_offset_for_scrollbar().y),
                "max": f32::from(items.max_offset_for_scrollbar().y),
                "dragging": app.main.list.bar.dragging(),
            },
        },
        "scroll": crate::scrollbar::areas()
            .into_iter()
            .map(|(key, top, max, dragging)| (key.to_string(), json!({ "top": top, "max": max, "dragging": dragging })))
            .collect::<serde_json::Map<_, _>>(),
        "viewing": app.viewing_session,
        "banners": app.banners.iter().map(|b| json!({ "id": b.id, "notice": b.notice })).collect::<Vec<_>>(),
        "toasts": app.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>(),
        "dialogs": format!("{:?}", app.dialogs),
        "popup": menu_json(&POPUP.lock()),
        "settings": app.settings,
    })
}

/// The shown window as X has it, read back from the server: GPUI's Linux backend has no
/// `render_to_image`. X11 only (a Wayland session runs the app through XWayland when `DISPLAY` is
/// set, which `xtask drive` does not change).
#[cfg(target_os = "linux")]
fn x11_shot(window: &Window) -> Result<image::RgbaImage, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let id = match handle.as_raw() {
        RawWindowHandle::Xcb(h) => h.window.get(),
        RawWindowHandle::Xlib(h) => h.window as u32,
        _ => return Err("shot needs an X11 window (run with DISPLAY set, e.g. on Xvfb)".into()),
    };
    let (conn, _) = x11rb::connect(None).map_err(|e| format!("X11: {e}"))?;
    let geometry = conn.get_geometry(id).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
    let (w, h) = (geometry.width, geometry.height);
    let reply = conn
        .get_image(ImageFormat::Z_PIXMAP, id, 0, 0, w, h, !0)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| format!("X11 GetImage: {e}"))?;
    let setup = conn.setup();
    let bpp = setup.pixmap_formats.iter().find(|f| f.depth == reply.depth).map_or(32, |f| f.bits_per_pixel);
    if bpp != 32 {
        return Err(format!("X11 GetImage: {bpp} bits a pixel, expected 32"));
    }
    // Little-endian 24/32-bit TrueColor, as Xvfb and every desktop serve it: B, G, R, pad.
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for px in reply.data.as_chunks::<4>().0 {
        rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    image::RgbaImage::from_raw(w.into(), h.into(), rgba).ok_or_else(|| "X11 GetImage: short image".into())
}
