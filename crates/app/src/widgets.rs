//! The app's parts in GPUI (DECISIONS → UI): pill buttons (ink for the one primary action, white
//! for the second), icon buttons with no ground of their own, switches whose thumb slides with the
//! jello, white panels of rows on the grey ground, chips, the dialog, and the select. Every size is
//! on the 8 px grid; colours are read from the palette when the view is built.

use std::cell::RefCell;
use std::time::Instant;

use gpui_kit::{
    Div, InteractiveElement, IntoElement, ParentElement, Stateful, StatefulInteractiveElement, Styled, anchored,
    deferred, div, px,
};

use crate::app::Message;
use crate::icons::{Icon, icon, turned};
use crate::motion::{self, Look};
use crate::theme::{Color, GRID, Ink, R_PANEL, R_ROW, alpha, g, hover_of, line, p, plain, t, tokens, wash};
use crate::ui::{self, El, clickable};

pub use crate::ui::with_tip;

/// A row: children side by side, centred across.
pub fn row() -> Div {
    div().flex().flex_row().items_center()
}

/// A column: children one under another.
pub fn col() -> Div {
    div().flex().flex_col()
}

/// Space that takes what is left in a row or column.
pub fn fill() -> Div {
    div().flex_1()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// The one main action of a view: an ink pill (開始, 停止, コピー, 保存).
    Primary,
    /// A destructive confirmation: the primary's shape; the dialog's words say what it does.
    Danger,
    /// The second action: a white pill on the ground.
    Secondary,
    /// A quiet one: text only, with the ground under the pointer.
    Ghost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Sm,
    Md,
    Lg,
}

impl Size {
    fn height(self) -> f32 {
        match self {
            Size::Sm => 4.0 * GRID,
            Size::Md => 5.0 * GRID,
            Size::Lg => 6.0 * GRID,
        }
    }

    fn pad_x(self) -> f32 {
        match self {
            Size::Lg => 3.0 * GRID,
            _ => 2.0 * GRID,
        }
    }
}

/// A variant's ground, hovered ground and text.
fn colors(variant: Variant) -> (Color, Color, Color) {
    let p = p();
    match variant {
        Variant::Primary | Variant::Danger => (p.ink, hover_of(p.ink), p.on_ink),
        Variant::Secondary => (p.surface, hover_of(p.surface), p.ink),
        Variant::Ghost => (Color::TRANSPARENT, wash(), p.ink),
    }
}

/// The text colour on a variant.
pub fn on_color(variant: Variant) -> Color {
    colors(variant).2
}

/// A pill in a variant's look around any content.
fn button_box(
    content: impl IntoElement,
    variant: Variant,
    height: f32,
    pad_x: f32,
    on: Option<Message>,
) -> Stateful<Div> {
    let (bg, hover, fg) = colors(variant);
    let enabled = on.is_some();
    let seed = ("btn", variant as u8);
    let mut b = clickable(on, seed).flex().flex_none().h(px(height)).rounded(px(height / 2.0)).bg(bg).text_color(fg);
    if enabled {
        b = b.hover(move |s| s.bg(hover));
    } else {
        b = b.opacity(0.45);
    }
    b.child(row().flex_1().justify_center().gap(g(1.0)).px(px(pad_x)).child(content))
}

/// A labelled button. `on` is `None` for a disabled one.
pub fn btn(label: impl Into<String>, variant: Variant, size: Size, on: Option<Message>) -> Stateful<Div> {
    btn_with(plain(label.into(), tokens::LABEL).bold(), variant, size, on)
}

/// A button with an icon before its label.
pub fn btn_icon(
    which: Icon,
    label: impl Into<String>,
    variant: Variant,
    size: Size,
    on: Option<Message>,
) -> Stateful<Div> {
    let fg = on_color(variant);
    btn_with(
        row().gap(g(1.0)).child(icon(which, 16.0, fg)).child(plain(label.into(), tokens::LABEL).bold()),
        variant,
        size,
        on,
    )
}

/// A button around any content.
pub fn btn_with(content: impl IntoElement, variant: Variant, size: Size, on: Option<Message>) -> Stateful<Div> {
    button_box(content, variant, size.height(), size.pad_x(), on)
}

/// A round button holding only an icon (the card's ▶): as wide as it is high, with no side
/// padding, which would push the icon off centre.
pub fn round_button(content: impl IntoElement, variant: Variant, size: Size, on: Option<Message>) -> Stateful<Div> {
    let side = size.height();
    button_box(content, variant, side, 0.0, on).w(px(side))
}

/// An icon-only button: a muted icon with no ground of its own, its name as a tooltip. `Md` is
/// 40 × 40 (the title bar's), `Lg` 48 (the bottom bar's), `Sm` 32.
pub fn icon_button(which: Icon, label: impl Into<String>, size: Size, on: Option<Message>) -> El {
    icon_button_tinted(which, label, size, on, None)
}

/// An icon button whose icon has a colour of its own while `tint` is set (a pressed pin, ...).
pub fn icon_button_tinted(
    which: Icon,
    label: impl Into<String>,
    size: Size,
    on: Option<Message>,
    tint: Option<Color>,
) -> El {
    let side = size.height();
    let glyph = if size == Size::Lg { 24.0 } else { 16.0 };
    icon_button_sized(which, label, side, side, glyph, on, tint)
}

/// An icon button of any size on the grid.
pub fn icon_button_sized(
    which: Icon,
    label: impl Into<String>,
    w: f32,
    h: f32,
    glyph: f32,
    on: Option<Message>,
    tint: Option<Color>,
) -> El {
    let enabled = on.is_some();
    let group = ui::uid(("icon-btn-group", which as u8));
    let group = format!("{group:?}");
    let mut svg = icon(which, glyph, tint.unwrap_or(p().muted));
    if tint.is_none() {
        let ink = p().ink;
        svg = svg.group_hover(group.clone(), move |s| s.text_color(ink));
    }
    let mut b = clickable(on, ("icon-btn", which as u8))
        .group(group)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(w))
        .h(px(h))
        .rounded(px(h.min(w) / 2.0))
        .child(svg);
    if enabled {
        let hover = wash();
        b = b.hover(move |s| s.bg(hover));
    } else {
        b = b.opacity(0.4);
    }
    with_tip(b, label.into()).into_any_element()
}

/// A sheet that floats over content (a select's list, the jump button): a white panel with the
/// `line` ring, since a white sheet on a white panel would have no edge, and no shadow.
pub fn floating<E: Styled>(d: E) -> E {
    let p = p();
    d.bg(p.surface).text_color(p.ink).border_2().border_color(p.line).rounded(px(R_ROW))
}

/// A switch's track and thumb: 48 × 32, ink when on, `line` when off; the thumb slides.
fn switch_track(key: &str, on: bool, enabled: bool) -> Div {
    // A switch that can't be changed fades as a whole.
    let a = if enabled { 1.0 } else { 0.5 };
    let x = if on { 20.0 } else { 4.0 };
    div()
        .relative()
        .flex_none()
        .w(g(6.0))
        .h(g(4.0))
        .rounded(g(2.0))
        .bg(alpha(if on { p().ink } else { p().line }, a))
        .child(motion::thumb(format!("thumb-{key}"), x, 3.0 * GRID, alpha(p().surface, a), p().thumb_side))
}

/// The switch on its own.
pub fn switch(on: bool, msg: Option<Message>) -> El {
    let enabled = msg.is_some();
    let key = format!("{msg:?}");
    clickable(msg, ("switch", on)).child(switch_track(&key, on, enabled)).into_any_element()
}

/// A row that toggles: its label, then the switch (a 56-high row of a panel).
pub fn toggle_row(label: impl Into<String>, on: bool, msg: Option<Message>) -> El {
    let enabled = msg.is_some();
    let color = if enabled { p().ink } else { alpha(p().ink, 0.5) };
    let label: String = label.into();
    let key = format!("{msg:?}");
    clickable(msg, ("toggle", &label))
        .flex()
        .items_center()
        .gap(g(2.0))
        .min_h(g(7.0))
        .px(g(2.0))
        .child(t(label, tokens::BODY, color).flex_1())
        .child(switch_track(&key, on, enabled))
        .into_any_element()
}

/// A labelled setting: the name over its control, as a row of a panel.
pub fn field(label: impl Into<String>, content: impl IntoElement) -> El {
    col()
        .gap(g(1.0))
        .py(g(1.0))
        .px(g(2.0))
        .child(line(label.into(), tokens::SMALL, p().muted))
        .child(content)
        .into_any_element()
}

/// A setting's row: its name, then its control at the end (a select, a value), 56 high.
pub fn setting_row(label: impl Into<String>, control: impl IntoElement) -> El {
    row()
        .w_full()
        .min_h(g(7.0))
        .gap(g(2.0))
        .px(g(2.0))
        .child(t(label.into(), tokens::BODY, p().ink).flex_1().min_w_0())
        .child(div().flex_none().child(control))
        .into_any_element()
}

thread_local! {
    static OPEN_SELECT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// A dropdown of choices: the chosen one in a small ground-coloured pill that opens a list under
/// it. `key` names it, so only one is open at a time.
pub fn select<T>(key: &str, options: Vec<T>, selected: Option<T>, on_select: impl Fn(T) -> Message) -> El
where
    T: ToString + PartialEq + Clone + 'static,
{
    let key = key.to_string();
    let open = OPEN_SELECT.with(|o| o.borrow().as_deref() == Some(key.as_str()));
    let label = selected.as_ref().map(|s| s.to_string()).unwrap_or_default();
    let p_ = p();
    let k = key.clone();
    let hover = p_.line;
    let head = div()
        .id(ui::uid(("select", &key)))
        .flex()
        .items_center()
        .gap(g(1.0))
        .h(g(4.0))
        .max_w(g(28.0))
        .pl(g(2.0))
        .pr(g(1.0))
        .rounded(g(2.0))
        .bg(if open { p_.line } else { p_.ground })
        .hover(move |s| s.bg(hover))
        .cursor_pointer()
        .child(line(label, tokens::LABEL, p_.ink).min_w_0())
        .child(turned(Icon::ChevronDown, 16.0, p_.muted, if open { 0.5 } else { 0.0 }))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            OPEN_SELECT.with(|o| {
                let mut o = o.borrow_mut();
                *o = if o.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) };
            });
            window.refresh();
        });
    let mut root = div().relative().flex_none().child(head);
    if open {
        let mut list = floating(col()).min_w(g(20.0)).max_h(g(35.0)).p(g(1.0));
        for o in options {
            let chosen = selected.as_ref() == Some(&o);
            let msg = on_select(o.clone());
            let mut item = row()
                .id(ui::uid(("select-opt", &key, o.to_string())))
                .gap(g(1.0))
                .h(g(5.0))
                .px(g(2.0))
                .rounded(g(2.0))
                .cursor_pointer()
                .hover(move |s| s.bg(p_.hover_panel))
                .child(line(o.to_string(), tokens::BODY, p_.ink).flex_1());
            item = if chosen {
                item.child(icon(Icon::Check, 16.0, p_.ink))
            } else {
                item.child(div().flex_none().size(px(16.0)))
            };
            let item = item.on_click(move |_, window, cx| {
                cx.stop_propagation();
                OPEN_SELECT.with(|o| *o.borrow_mut() = None);
                window.refresh();
                ui::send(msg.clone(), cx);
            });
            list = list.child(item);
        }
        let (handle, bar) = crate::scrollbar::handle_bar(&format!("select-{key}"));
        let list = list
            .id(ui::uid(("select-list", &key)))
            .track_scroll(&handle)
            .overflow_y_scroll()
            .occlude()
            .on_mouse_down_out(|_, window, _| {
                OPEN_SELECT.with(|o| *o.borrow_mut() = None);
                window.refresh();
            });
        // The bar goes over the list's right edge, inside its sheet.
        let list = div().relative().child(list).child(crate::scrollbar::bar(&handle, &bar, 0.0));
        let sheet = motion::enter(
            format!("select-sheet-{key}"),
            list,
            Some(motion::From { dx: 0.0, dy: -GRID }),
            false,
            Look { color: p_.surface, side: p_.side, radius: R_ROW },
        );
        root = root.child(deferred(anchored().child(div().mt(g(1.0)).child(sheet))).with_priority(2));
    }
    root.into_any_element()
}

/// A thin progress bar in ink.
pub fn progress(value: f32, max: f32) -> El {
    let frac = if max > 0.0 { (value / max).clamp(0.0, 1.0) } else { 0.0 };
    div()
        .w_full()
        .h(px(4.0))
        .rounded(px(2.0))
        .bg(p().line)
        .child(div().h_full().w(gpui_kit::relative(frac)).rounded(px(2.0)).bg(p().ink))
        .into_any_element()
}

/// A 1 px line across, for the few places a list needs one.
pub fn hairline() -> El {
    div().flex_none().w_full().h(px(1.0)).bg(p().line).into_any_element()
}

/// A white panel: rows on the ground, 24 radius, 8 padding.
pub fn panel(rows: Vec<El>) -> El {
    col().w_full().p(g(1.0)).bg(p().surface).rounded(px(R_PANEL)).children(rows).into_any_element()
}

/// A group's label above its panel: small and muted, in line with the rows' text.
pub fn group_label(label: impl Into<String>) -> El {
    div().pl(g(2.0)).child(line(label.into(), tokens::SMALL, p().muted)).into_any_element()
}

/// A labelled group: the label, then its rows on a white panel.
pub fn section(title: Option<String>, rows: Vec<El>) -> El {
    let mut c = col().w_full().gap(g(1.0));
    if let Some(title) = title {
        c = c.child(group_label(title));
    }
    c.child(panel(rows)).into_any_element()
}

/// A dialog: a white panel over the dimmed window, its title, its words and its actions as
/// full-width pills, the main one on top.
pub fn dialog(title: impl Into<String>, body: Option<El>, actions: Vec<El>) -> El {
    let mut sheet = col().w_full().gap(g(2.0)).child(t(title.into(), tokens::HEADING, p().ink));
    if let Some(b) = body {
        sheet = sheet.child(b);
    }
    sheet = sheet.child(col().w_full().gap(g(1.0)).pt(g(1.0)).children(actions));
    let panel = div().w_full().max_w(g(48.0)).p(g(3.0)).bg(p().surface).rounded(px(R_PANEL)).child(sheet);
    let panel = motion::enter(
        "dialog",
        panel,
        Some(motion::From { dx: 0.0, dy: 2.0 * GRID }),
        false,
        Look { color: p().surface, side: p().side, radius: R_PANEL },
    );
    div()
        .id("dialog-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .p(g(2.0))
        .bg(p().scrim)
        .occlude()
        .child(div().w_full().max_w(g(48.0)).child(panel))
        .into_any_element()
}

/// A dialog's full-width action button, its label centred.
pub fn dialog_button(label: impl Into<String>, variant: Variant, on: Option<Message>) -> El {
    button_box(plain(label.into(), tokens::LABEL).bold(), variant, Size::Lg.height(), 3.0 * GRID, on)
        .w_full()
        .into_any_element()
}

/// The loader icon, turning with the clock (the app ticks while one is shown).
pub fn spinner(size: f32, now: Instant) -> El {
    let turn = (now.duration_since(*START).as_millis() % 900) as f32 / 900.0;
    turned(Icon::LoaderCircle, size, p().muted, turn).into_any_element()
}

static START: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

/// A project's colour (FR-64) as a dot.
pub fn swatch(color: Color, size: f32) -> El {
    div().flex_none().size(px(size)).rounded(px(size / 2.0)).bg(color).into_any_element()
}

/// A plain clickable area with no look of its own beyond the ground under the pointer.
pub fn bare(content: impl IntoElement, on: Option<Message>, hover_bg: bool) -> Stateful<Div> {
    let mut b = clickable(on, "bare").rounded(px(R_ROW)).child(content);
    if hover_bg {
        let hover = wash();
        b = b.hover(move |s| s.bg(hover));
    }
    b
}

/// A chip: 32 high, muted text (the chosen one's pill comes from `chips`).
pub fn chip(label: impl Into<String>, dot: Option<Color>, chosen: bool, on: Option<Message>) -> El {
    let label: String = label.into();
    let mut content = row().gap(g(1.0));
    if let Some(c) = dot {
        content = content.child(swatch(c, GRID));
    }
    // The label takes the chip's colour, so the hover's ink reaches it.
    let text = plain(label.clone(), tokens::LABEL).overflow_hidden().text_ellipsis();
    content = content.child(if chosen { text.bold() } else { text });
    let ink = p().ink;
    let b = clickable(on, ("chip", &label))
        .flex()
        .flex_none()
        .items_center()
        .h(g(4.0))
        .px(g(2.0))
        .text_color(if chosen { p().ink } else { p().muted })
        // On the chosen chip too (it is ink already): see `screens::titlebar::tabs`.
        .hover(move |s| s.text_color(ink))
        .child(content);
    b.into_any_element()
}

/// A row of chips with the chosen one's white pill sliding between them (設定's sections, 履歴's
/// projects).
pub fn chips(key: &str, items: Vec<El>, chosen: Option<usize>) -> Div {
    motion::slide(format!("chips-{key}"), row().flex_none().h(g(5.0)), items, chosen, chip_look())
}

/// Chips that wrap onto further lines when they don't fit (設定's sections), keeping the 40 pitch
/// of a single row; the chosen one's pill slides between lines too.
pub fn chips_wrapped(key: &str, items: Vec<El>, chosen: Option<usize>) -> Div {
    let container = div().flex().flex_wrap().items_center().w_full().gap_y(g(1.0)).py(px(4.0));
    motion::slide(format!("chips-{key}"), container, items, chosen, chip_look())
}

fn chip_look() -> Look {
    Look { color: p().surface, side: p().side, radius: R_ROW }
}

/// A quiet text button: muted, ink when pointed at.
pub fn text_button(glyph: Option<Icon>, label: impl Into<String>, on: Option<Message>) -> El {
    let label: String = label.into();
    let enabled = on.is_some();
    let ink = p().ink;
    let tone = if enabled { p().muted } else { alpha(p().muted, 0.45) };
    let mut content = row().gap(g(1.0));
    if let Some(i) = glyph {
        content = content.child(icon(i, 16.0, tone).group_hover("text-btn", move |s| s.text_color(ink)));
    }
    content = content.child(plain(label.clone(), tokens::SMALL));
    let mut b = clickable(on, ("text-btn", &label))
        .group("text-btn")
        .flex()
        .flex_none()
        .items_center()
        .h(g(4.0))
        .px(g(2.0))
        .rounded(g(2.0))
        .text_color(tone)
        .child(content);
    if enabled {
        b = b.hover(move |s| s.text_color(ink));
    }
    b.into_any_element()
}

/// Nothing (an empty slot in a row).
pub fn nothing() -> El {
    div().into_any_element()
}
