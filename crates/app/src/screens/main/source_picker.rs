//! What to listen to (FR-10 to FR-13), on the start screen: 相手 is a white panel of the apps (those
//! playing first, each saying whether it is playing now), everything the PC plays, or the mic
//! alone; 自分 is whether it is a conversation, when the mic records your voice too and the
//! transcript names the lines 相手 and 自分, and which mic. While recording, and in the compact
//! window, the same choice is the native source menu (FR-17).

use chidori_core::audio::source::AudioApp;
use chidori_core::settings::SourceMode;
use gpui_kit::{InteractiveElement, IntoElement, ParentElement, Styled, div, px};

use super::Msg;
use crate::app::{App, Message, call};
use crate::i18n::t;
use crate::icons::{Icon, icon};
use crate::motion::{self, Look};
use crate::task::Task;
use crate::theme::{self, GRID, Ink, R_PANEL, R_ROW, alpha, g, line, p, tokens};
use crate::ui::{El, clickable};
use crate::widgets::{col, icon_button_sized, row, select, setting_row, spinner, text_button, toggle_row, with_tip};

#[derive(Debug, Clone)]
pub enum PickMsg {
    Pick(SourceMode, Option<u32>),
    Refresh,
    Refreshed,
    Conversation,
    Mic(Option<String>),
}

fn m(msg: PickMsg) -> Message {
    Message::Main(Msg::Pick(msg))
}

/// The apps whose sound is read: on the start screen every app with an audio session, so each
/// row can say whether it is playing; otherwise the chosen app's.
pub fn shown_pids(app: &App, start: bool) -> Vec<u32> {
    let s = &app.source;
    let mut pids = Vec::new();
    if s.mode == SourceMode::App
        && let Some(a) = &s.app
        && a.root_pid != 0
    {
        pids.push(a.root_pid);
    }
    if start {
        pids.extend(s.apps.iter().filter(|a| a.has_session && a.root_pid != 0).map(|a| a.root_pid));
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

pub fn update(app: &mut App, msg: PickMsg) -> Task {
    match msg {
        PickMsg::Pick(mode, pid) => {
            if let Some(pid) = pid
                && let Some(a) = app.source.apps.iter().find(|a| a.root_pid == pid).cloned()
            {
                app.source.set_app(&a);
            }
            app.source.mode = mode;
            super::source_changed(app)
        }
        PickMsg::Refresh => Task::batch([refresh(app), app.refresh_devices()]),
        PickMsg::Refreshed => Task::none(),
        PickMsg::Conversation => {
            app.source.include_mic = !app.source.include_mic;
            super::source_changed(app)
        }
        PickMsg::Mic(id) => {
            app.source.mic_device_id = id.clone();
            Task::batch([
                Task::done(Message::UpdateSettings(serde_json::json!({ "source": { "micDeviceId": id } }))),
                super::source_changed(app),
            ])
        }
    }
}

/// A fresh app list; the rows follow it.
pub fn refresh(app: &mut App) -> Task {
    app.refresh_apps().chain(Task::done(m(PickMsg::Refreshed)))
}

/// Tells the core which apps' sound to read when that changed.
pub fn sync_watch(app: &mut App, pids: Vec<u32>) -> Task {
    match app.app_levels.watch(pids, app.now) {
        Some(pids) => call(app, move |c| chidori_core::ops::watch_app_levels(c, pids), |_| Message::Noop),
        None => Task::none(),
    }
}

/// One choice of 相手: its name over what it is doing, and a check when chosen. 64 high.
fn option(chosen: bool, enabled: bool, name: String, status: Option<String>, on: Message) -> El {
    let fade = |c| if enabled { c } else { alpha(c, 0.45) };
    let name = line(name, tokens::BODY, fade(p().ink));
    let mut words = col().flex_1().min_w_0().overflow_hidden().child(if chosen { name.bold() } else { name });
    if let Some(s) = status {
        words = words.child(line(s, tokens::SMALL, fade(p().muted)));
    }
    let check: El = if chosen {
        icon(Icon::Check, 24.0, p().ink).into_any_element()
    } else {
        div().flex_none().size(g(3.0)).into_any_element()
    };
    let mut b = clickable(enabled.then_some(on), "source-option")
        .flex()
        .items_center()
        .gap(g(2.0))
        .w_full()
        .h(g(8.0))
        .px(g(2.0))
        .rounded(px(R_ROW))
        .child(words)
        .child(check);
    // The chosen row has the sliding pill under it already, so its hover is empty, but it has one
    // so GPUI keeps tracking the pointer over it (see `screens::titlebar::tabs`).
    if enabled {
        let hover = theme::wash();
        b = b.hover(move |s| if chosen { s } else { s.bg(hover) });
    }
    b.into_any_element()
}

/// 音が出ています while the app plays, 無音 when it has a session but is quiet now.
fn status_of(app: &App, a: &AudioApp) -> Option<String> {
    if !a.has_session {
        return None;
    }
    let playing = app.app_levels.tracks.get(&a.root_pid).is_some_and(|t| !t.quiet(app.now));
    Some(t(if playing { "appPlaying" } else { "appQuiet" }))
}

/// The choices of 相手 and which of them is chosen: the apps (those with sound first), then
/// システム全体 and マイクだけ (FR-11).
fn options(app: &App) -> (Vec<El>, Option<usize>) {
    let s = &app.source;
    let supported = app.info.app_loopback_supported;
    let mut list = Vec::new();
    let mut chosen = None;
    let playing = s.apps.iter().filter(|a| a.has_session);
    let others = s.apps.iter().filter(|a| !a.has_session);
    for a in playing.chain(others) {
        let here = s.mode == SourceMode::App && s.is_chosen(a);
        if here {
            chosen = Some(list.len());
        }
        let o = option(
            here,
            supported,
            a.name.clone(),
            status_of(app, a),
            m(PickMsg::Pick(SourceMode::App, Some(a.root_pid))),
        );
        list.push(if supported { o } else { with_tip(o, crate::i18n::app_capture_unavailable()).into_any_element() });
    }
    if s.mode == SourceMode::System {
        chosen = Some(list.len());
    }
    list.push(option(
        s.mode == SourceMode::System,
        true,
        t("srcSystem"),
        None,
        m(PickMsg::Pick(SourceMode::System, None)),
    ));
    if s.mode == SourceMode::Mic {
        chosen = Some(list.len());
    }
    list.push(option(s.mode == SourceMode::Mic, true, t("srcMicOnly"), None, m(PickMsg::Pick(SourceMode::Mic, None))));
    (list, chosen)
}

/// A mic in the list, by its ID (`None`: the system's default).
#[derive(Debug, Clone, PartialEq)]
struct MicOption {
    id: Option<String>,
    name: String,
}

impl std::fmt::Display for MicOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// The start screen's choice: 相手 and 自分, each a label over a white panel.
pub fn picker(app: &App) -> El {
    let s = &app.source;
    let refresh: El = if s.loading_apps {
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(g(3.0))
            .child(spinner(16.0, app.now))
            .into_any_element()
    } else {
        icon_button_sized(Icon::RefreshCw, t("refresh"), 3.0 * GRID, 3.0 * GRID, 16.0, Some(m(PickMsg::Refresh)), None)
    };
    let label = |text: String| div().flex_1().pl(g(2.0)).child(line(text, tokens::SMALL, p().muted));
    let them_label = row().h(g(3.0)).child(label(t("listenToThem"))).child(refresh);

    let (items, chosen) = options(app);
    let look = Look { color: p().hover_panel, side: p().side, radius: R_ROW };
    let list = motion::slide("source-rows", col().w_full(), items, chosen, look);
    let them = div().w_full().p(g(1.0)).bg(p().surface).rounded(px(R_PANEL)).child(list);

    let only = s.mode == SourceMode::Mic;
    let conversation = toggle_row(t("conversation"), only || s.include_mic, (!only).then(|| m(PickMsg::Conversation)));
    let mut mics = vec![MicOption { id: None, name: crate::i18n::default_device() }];
    mics.extend(s.devices.iter().map(|d| MicOption { id: Some(d.id.clone()), name: d.name.clone() }));
    let current = mics.iter().find(|o| o.id == s.mic_device_id).cloned().or_else(|| mics.first().cloned());
    let mic = setting_row(t("srcMic"), select("mic", mics, current, |o: MicOption| m(PickMsg::Mic(o.id))));
    let me = div()
        .w_full()
        .p(g(1.0))
        .bg(p().surface)
        .rounded(px(R_PANEL))
        .child(col().w_full().child(conversation).child(mic));

    col()
        .w_full()
        .gap(g(1.0))
        .child(them_label)
        .child(them)
        .child(div().pt(g(1.0)).child(label(t("me"))))
        .child(me)
        .into_any_element()
}

/// 「ファイルから文字起こし」: a recording made elsewhere, transcribed as fast as the PC can (FR-18).
pub fn file_button(app: &App) -> El {
    let enabled = app.rec.state == chidori_core::events::UiState::Ready;
    text_button(
        Some(Icon::FileText),
        t("transcribeFile"),
        enabled.then_some(Message::Action(crate::actions::Action::TranscribeFile)),
    )
}

/// The chosen source in a few words, `Zoom · 会話`, for the compact window.
pub fn summary_text(app: &App) -> String {
    let s = &app.source;
    let name = match s.mode {
        SourceMode::App => s.app.as_ref().map_or_else(|| t("chooseApp"), |a| a.name.clone()),
        SourceMode::System => t("srcSystem"),
        SourceMode::Mic => t("srcMicOnly"),
    };
    if s.mode != SourceMode::Mic && s.include_mic { format!("{name} · {}", t("conversationShort")) } else { name }
}
