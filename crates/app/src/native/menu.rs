//! A menu's items, and the native popup that shows them where the app's own drawn menu
//! (`screens::popup`) can't fit the window: the compact window (380 × 170) is too small for a
//! long one, and a native menu may reach past the window. A pick arrives through the bridge as
//! `UiEvent::Menu(id)`.

#[cfg(any(windows, target_os = "macos"))]
use tray_icon::menu::ContextMenu;
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use crate::bridge::{self, UiEvent};

#[derive(Debug, Clone)]
pub enum Item {
    Action {
        id: String,
        label: String,
        enabled: bool,
    },
    /// An item with a check mark, shown checked or not.
    Check {
        id: String,
        label: String,
        checked: bool,
        enabled: bool,
    },
    Separator,
    Submenu {
        label: String,
        items: Vec<Item>,
        enabled: bool,
    },
}

impl Item {
    pub fn action(id: impl Into<String>, label: impl Into<String>) -> Self {
        Item::Action { id: id.into(), label: label.into(), enabled: true }
    }

    pub fn check(id: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        Item::Check { id: id.into(), label: label.into(), checked, enabled: true }
    }

    pub fn submenu(label: impl Into<String>, items: Vec<Item>) -> Self {
        Item::Submenu { label: label.into(), items, enabled: true }
    }

    /// The same item, greyed out unless `enabled`.
    pub fn enabled(mut self, on: bool) -> Self {
        match &mut self {
            Item::Action { enabled, .. } | Item::Check { enabled, .. } | Item::Submenu { enabled, .. } => *enabled = on,
            Item::Separator => {}
        }
        self
    }
}

/// Sends menu picks into the bridge. Call once at startup.
pub fn init() {
    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        bridge::send(UiEvent::Menu(event.id.0));
    }));
}

fn build(items: &[Item]) -> Vec<Box<dyn IsMenuItem>> {
    items
        .iter()
        .map(|item| -> Box<dyn IsMenuItem> {
            match item {
                Item::Action { id, label, enabled } => Box::new(MenuItem::with_id(id.as_str(), label, *enabled, None)),
                Item::Check { id, label, checked, enabled } => {
                    Box::new(CheckMenuItem::with_id(id.as_str(), label, *enabled, *checked, None))
                }
                Item::Separator => Box::new(PredefinedMenuItem::separator()),
                Item::Submenu { label, items, enabled } => {
                    let children = build(items);
                    let refs: Vec<&dyn IsMenuItem> = children.iter().map(|c| c.as_ref()).collect();
                    let sub = Submenu::new(label, *enabled);
                    if let Err(e) = sub.append_items(&refs) {
                        tracing::warn!("submenu: {e}");
                    }
                    Box::new(sub)
                }
            }
        })
        .collect()
}

/// Shows a menu at a point in the window (logical pixels), or at the cursor. Blocks until the
/// menu closes; the pick, if any, follows as an event.
///
/// On macOS the menu runs its own event loop, and AppKit keeps calling GPUI from inside it (the
/// keyboard layout, redraws). Shown from within GPUI's update, those calls find the app already
/// borrowed and panic, so there it opens on the main queue once the update is over.
pub fn popup(hwnd: u64, items: &[Item], at: Option<crate::paint::Point>) {
    #[cfg(target_os = "macos")]
    {
        let items = items.to_vec();
        mac::after_this_event(Box::new(move || show(hwnd, &items, at)));
    }
    #[cfg(not(target_os = "macos"))]
    show(hwnd, items, at);
}

fn show(hwnd: u64, items: &[Item], at: Option<crate::paint::Point>) {
    let menu = Menu::new();
    let children = build(items);
    let refs: Vec<&dyn IsMenuItem> = children.iter().map(|c| c.as_ref()).collect();
    if let Err(e) = menu.append_items(&refs) {
        tracing::warn!("popup menu: {e}");
        return;
    }
    let position =
        at.map(|p| tray_icon::dpi::Position::Logical(tray_icon::dpi::LogicalPosition::new(p.x as f64, p.y as f64)));
    #[cfg(windows)]
    // SAFETY: `hwnd` is the app's own live window, from GPUI.
    unsafe {
        let picked = menu.show_context_menu_for_hwnd(hwnd as isize, position);
        tracing::debug!("popup menu closed, picked: {picked}");
    }
    #[cfg(target_os = "macos")]
    // SAFETY: `hwnd` is the app's own live NSView, from GPUI.
    unsafe {
        let picked = menu.show_context_menu_for_nsview(hwnd as usize as *const std::ffi::c_void, position);
        tracing::debug!("popup menu closed, picked: {picked}");
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = (hwnd, position);
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    type Job = Box<dyn FnOnce()>;

    unsafe extern "C" {
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
    }

    extern "C" fn run(context: *mut c_void) {
        // SAFETY: `context` is the box `after_this_event` leaked, run once on the main queue.
        let job = unsafe { Box::from_raw(context.cast::<Job>()) };
        job();
    }

    /// Runs `job` on the main thread after the current event has been handled.
    pub fn after_this_event(job: Job) {
        let context = Box::into_raw(Box::new(job)).cast::<c_void>();
        // SAFETY: `_dispatch_main_q` is the main queue (what `dispatch_get_main_queue()` returns);
        // `run` takes back the box exactly once.
        unsafe { dispatch_async_f(&raw const _dispatch_main_q, context, run) };
    }
}
