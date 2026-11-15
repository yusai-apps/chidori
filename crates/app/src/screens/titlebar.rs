//! The title bar over every expanded screen (DECISIONS → UI, "One title bar"): the name, the three
//! places 録音 · 履歴 · 設定 as tabs with a white pill under the current one that slides between
//! them, and the window buttons. 録音 carries the red dot while a recording runs elsewhere. There
//! is no bottom navigation. Dragging the bar moves the window. The compact window has a header of
//! its own (`main_view`), and the source menu it opens lives here.

use chidori_core::settings::{Layout, SourceMode};
use gpui_kit::{FontWeight, InteractiveElement, IntoElement, ParentElement, Styled, WindowControlArea, div, px};

use crate::app::{App, Message, Place, View, WinAction};
use crate::i18n::t;
use crate::icons::Icon;
use crate::motion::{self, Look};
use crate::native::menu::Item;
use crate::screens::popup::Menu;
use crate::task::Task;
use crate::theme::{GRID, Ink, R_ROW, SANS, TypeStyle, g, p, plain, tokens};
use crate::ui::{self, El, Ui, clickable};
use crate::widgets::{icon_button_sized, row, with_tip};

#[derive(Debug, Clone)]
pub enum Msg {
    /// The compact window's source: a native menu to switch it (FR-17).
    SourceMenu,
    /// The app list is fresh: the menu opens.
    SourceMenuReady,
}

pub fn update(app: &mut App, msg: Msg) -> Task {
    match msg {
        Msg::SourceMenu => {
            let refresh = app.refresh_apps();
            refresh.chain(Task::done(Message::Titlebar(Msg::SourceMenuReady)))
        }
        Msg::SourceMenuReady => Task::done(Message::Popup(source_menu(app), None)),
    }
}

/// The source menu, once the app list is fresh.
fn source_menu(app: &App) -> Vec<Item> {
    let s = &app.source;
    let supported = app.info.app_loopback_supported;
    let mut items = vec![
        Item::check("titlebar:src-system", t("srcSystem"), s.mode == SourceMode::System),
        Item::check("titlebar:src-mic", t("srcMicOnly"), s.mode == SourceMode::Mic),
        Item::Separator,
    ];
    let playing = s.apps.iter().filter(|a| a.has_session);
    let others = s.apps.iter().filter(|a| !a.has_session);
    for a in playing.chain(others) {
        items.push(
            Item::check(format!("titlebar:src-app:{}", a.root_pid), a.name.clone(), s.is_chosen(a)).enabled(supported),
        );
    }
    if !s.apps.is_empty() {
        items.push(Item::Separator);
    }
    items.push(
        Item::check("titlebar:src-conversation", t("conversation"), s.mode != SourceMode::Mic && s.include_mic)
            .enabled(s.mode != SourceMode::Mic),
    );
    items
}

/// A pick in the source menu changes the choice; the main view turns it into a switch.
pub fn on_menu(app: &mut App, rest: &str) -> Task {
    let s = &mut app.source;
    match rest.split_once(':') {
        Some(("src-app", pid)) => {
            if let Some(a) = s.apps.iter().find(|a| a.root_pid.to_string() == pid).cloned() {
                s.set_app(&a);
                s.mode = SourceMode::App;
            }
        }
        _ => match rest {
            "src-system" => s.mode = SourceMode::System,
            "src-mic" => s.mode = SourceMode::Mic,
            "src-conversation" => s.include_mic = !s.include_mic,
            _ => return Task::none(),
        },
    }
    super::main_view::source_changed(app)
}

/// The places' labels, in the order the tabs show them.
fn places() -> [(Place, String, View); 3] {
    [
        (Place::Main, t("tabRecord"), View::Main),
        (Place::History, t("history"), View::History),
        (Place::Settings, t("settings"), View::Settings),
    ]
}

/// How much of the bar fits the window: everything, the tabs without the name, or tighter tabs.
#[derive(Clone, Copy, PartialEq)]
enum Fit {
    Full,
    NoName,
    Tight,
}

impl Fit {
    /// A tab's side padding and the gap between tabs.
    fn tab_spacing(self) -> (f32, f32) {
        if self == Fit::Tight { (GRID, 0.0) } else { (2.0 * GRID, GRID) }
    }
}

/// The width a line of text takes, as the bar would draw it.
fn text_width(ui: &Ui, s: &str, style: TypeStyle, bold: bool) -> f32 {
    let mut ts = ui.window.text_style();
    ts.font_family = SANS.into();
    if bold {
        ts.font_weight = FontWeight::BOLD;
    }
    let run = ts.to_run(s.len());
    f32::from(ui.window.text_system().layout_line(s, px(style.size), &[run], None).width)
}

/// Measured rather than a fixed breakpoint: English's labels are wider than Japanese's, and the
/// window buttons must never run off the right edge.
fn fit(app: &App, ui: &Ui, buttons: f32) -> Fit {
    let avail = f32::from(ui.window.viewport_size().width);
    let current = app.view.place();
    let labels: f32 = places().iter().map(|(pl, l, _)| text_width(ui, l, tokens::LABEL, *pl == current)).sum();
    let dot = if app.rec.busy() { 2.0 * GRID } else { 0.0 };
    let tabs = |f: Fit| {
        let (pad, gap) = f.tab_spacing();
        labels + 6.0 * pad + 2.0 * gap + dot
    };
    // Left and right padding, the gaps either side of the spacer, the window buttons.
    let edges = 2.0 * GRID + GRID + 2.0 * GRID + buttons;
    let name = text_width(ui, "Chidori", tokens::BODY, true) + 2.0 * GRID;
    if edges + name + tabs(Fit::Full) <= avail {
        Fit::Full
    } else if edges + tabs(Fit::NoName) <= avail {
        Fit::NoName
    } else {
        Fit::Tight
    }
}

/// 録音 · 履歴 · 設定, the current one bold on its white pill.
fn tabs(app: &App, fit: Fit) -> El {
    let busy = app.rec.busy();
    let current = app.view.place();
    let (pad, gap) = fit.tab_spacing();
    let entries = places();
    let mut items = Vec::new();
    let mut chosen = None;
    for (i, (place, label, view)) in entries.into_iter().enumerate() {
        let here = place == current;
        if here {
            chosen = Some(i);
        }
        let mut content = row().gap(g(1.0));
        if place == Place::Main && busy && !here {
            content = content.child(div().flex_none().size(px(GRID)).rounded(px(GRID / 2.0)).bg(p().rec));
        }
        // The label takes the tab's colour, so the hover's ink reaches it.
        let text = plain(label, tokens::LABEL);
        content = content.child(if here { text.bold() } else { text });
        let ink = p().ink;
        // Every tab keeps the hover, the current one too (where it changes nothing): GPUI only
        // tracks the pointer for an element with a hover style, so a tab left without one keeps
        // the "hovered" it had when it was picked and stays white after the next pick until the
        // pointer moves.
        let b = clickable(Some(Message::Go(view)), "place")
            .flex()
            .items_center()
            .h(g(4.0))
            .px(px(pad))
            .text_color(if here { p().ink } else { p().muted })
            .hover(move |s| s.text_color(ink))
            .child(content);
        let tip = if place == Place::Main && busy && !here { t("backToLive") } else { String::new() };
        items.push(with_tip(b, tip).into_any_element());
    }
    let look = Look { color: p().surface, side: p().side, radius: R_ROW };
    motion::slide("tabs", row().flex_none().gap(px(gap)), items, chosen, look).into_any_element()
}

/// Always on top, shrink, minimize, close: muted icons, 32 wide so four fit beside the tabs.
fn window_buttons(app: &App, layout_toggle: bool) -> El {
    let on_top = app.settings.window.always_on_top;
    let compact = app.view == View::Main && app.layout() == Layout::Compact;
    let (w, h) = (4.0 * GRID, 5.0 * GRID);
    let button = |glyph: Icon, label: String, msg: WinAction, tint| {
        icon_button_sized(glyph, label, w, h, 16.0, Some(Message::Win(msg)), tint)
    };
    let mut r =
        row().flex_none().child(button(Icon::Pin, t("alwaysOnTop"), WinAction::ToggleOnTop, on_top.then(|| p().ink)));
    if layout_toggle {
        let (glyph, label) = if compact { (Icon::Maximize, t("expand")) } else { (Icon::Minimize, t("shrink")) };
        r = r.child(button(glyph, label, WinAction::ToggleLayout, None));
    }
    r.child(button(Icon::Minus, t("minimize"), WinAction::Minimize, None))
        .child(button(Icon::X, t("close"), WinAction::Close, None))
        .into_any_element()
}

pub fn view(app: &App, ui: &Ui) -> El {
    let setup = app.needs_wizard();
    let fit = if setup { Fit::Full } else { fit(app, ui, 4.0 * 4.0 * GRID) };
    let mut bar = row().flex_none().gap(g(1.0)).h(g(6.0)).pl(g(2.0)).pr(g(1.0));
    if fit == Fit::Full {
        bar = bar.child(div().flex_none().mr(g(1.0)).child(plain("Chidori", tokens::BODY).bold().text_color(p().ink)));
    }
    if !setup {
        bar = bar.child(solid(tabs(app, fit)));
    }
    bar = bar.child(div().flex_1()).child(solid(window_buttons(app, !setup)));
    // Where nothing else takes the pointer, the bar moves the window (Windows' own caption drag).
    with_window_menu(app, bar.id("titlebar").window_control_area(WindowControlArea::Drag)).into_any_element()
}

/// A right click on the bar (or the compact window's header) opens the app's window menu in
/// place of Windows' own: its release is taken too, since Windows opens the system menu on it.
pub fn with_window_menu<E: InteractiveElement>(app: &App, el: E) -> E {
    let compact = app.view == View::Main && app.layout() == Layout::Compact;
    let mut menu =
        Menu::new().check(t("alwaysOnTop"), app.settings.window.always_on_top, Message::Win(WinAction::ToggleOnTop));
    if !app.needs_wizard() {
        let label = if compact { t("expand") } else { t("shrink") };
        menu = menu.act(label, Some(Message::Win(WinAction::ToggleLayout)));
    }
    let menu = menu
        .separator()
        .act(t("minimize"), Some(Message::Win(WinAction::Minimize)))
        .act(t("close"), Some(Message::Win(WinAction::Close)))
        .open();
    ui::on_right(el, menu).on_mouse_up(gpui_kit::MouseButton::Right, |_, _, cx| cx.stop_propagation())
}

/// The compact window's window buttons: expand and close.
pub fn compact_buttons() -> El {
    let (w, h) = (5.0 * GRID, 5.0 * GRID);
    solid(
        row()
            .flex_none()
            .child(icon_button_sized(
                Icon::Maximize,
                t("expand"),
                w,
                h,
                16.0,
                Some(Message::Win(WinAction::ToggleLayout)),
                None,
            ))
            .child(icon_button_sized(Icon::X, t("close"), w, h, 16.0, Some(Message::Win(WinAction::Close)), None))
            .into_any_element(),
    )
}

/// Keeps the bar's drag area from under what can be clicked: Windows asks GPUI what the pointer
/// is over, and any drag area still under it makes the click a window drag.
pub fn solid(content: El) -> El {
    div().id(crate::ui::uid("solid")).flex().flex_none().occlude().child(content).into_any_element()
}
