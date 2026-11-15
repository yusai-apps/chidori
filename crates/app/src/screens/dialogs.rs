//! Dialogs over everything: recovery at launch (flow D), quitting while recording (FR-93),
//! 書き出し, renaming a row (FR-63), moving sessions to the Recycle Bin (FR-62), and a project's
//! name and colour (FR-64). Each is a white sheet with its main action on top; while one is open it
//! holds the view's one primary.

use chidori_core::error::AppError;
use chidori_core::events::UiState;
use chidori_core::ops;
use chidori_core::session::projects::Project;
use chidori_core::session::recovery::SessionSummary;
use gpui_kit::{InteractiveElement, IntoElement, ParentElement, Styled, div, px};

use crate::actions::{self, Action, ExportKind};
use crate::app::{App, Level, Message, call};
use crate::i18n::{t, tf};
use crate::icons::{Icon, icon};
use crate::model::format::{date_time_of, hms};
use crate::task::{self, Task};
use crate::theme::{self, Ink, PROJECT_COLORS, R_PANEL, R_ROW, g, line, p, project_color, tokens};
use crate::ui::{self, El, Field, KeyPress, Ui, clickable};
use crate::widgets::{self, Variant, col, dialog, dialog_button, row, with_tip};

/// The text boxes a dialog can hold, focused when it opens. Each dialog has its own, so one
/// doesn't keep the other's placeholder.
const RENAME_INPUT: &str = "dialog-rename";
const PROJECT_INPUT: &str = "dialog-project";

/// A project's name is kept short enough for a menu and a title.
const NAME_MAX: usize = 40;

#[derive(Debug, Default)]
pub struct State {
    open: Option<Dialog>,
    /// Sessions that ended in a crash, offered one at a time at launch (flow D); 後で puts off all
    /// of them until the next launch.
    recoverable: Vec<SessionSummary>,
    recovering: bool,
    /// Markdown comes as one ZIP: remembered while the app runs.
    zip: bool,
}

#[derive(Debug, Clone)]
enum Dialog {
    ConfirmQuit,
    Rename {
        id: String,
        title: String,
    },
    Delete {
        ids: Vec<String>,
        title: Option<String>,
        /// From 履歴's 選択: selection ends once they are gone.
        from_selection: bool,
    },
    Export {
        id: String,
        busy: Option<ExportKind>,
    },
    Project(ProjectForm),
    DeleteProject(Project),
}

/// A project's name and colour: a new one (`editing` None) for these sessions, or one being changed.
#[derive(Debug, Clone)]
struct ProjectForm {
    editing: Option<Project>,
    session_ids: Vec<String>,
    /// Asked from 履歴's filter menu: the new project's page opens.
    open_page: bool,
    name: String,
    color: String,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Close,
    Escape,
    Quit,
    QuitAnswer(Result<bool, AppError>),
    Title(String),
    SaveTitle,
    ConfirmDelete,
    ExportAs(ExportKind),
    Zip(bool),
    Exported,
    ProjectName(String),
    ProjectColor(&'static str),
    SaveProject,
    /// A project was made (and its sessions put in it); `bool`: its page opens.
    ProjectMade(Result<Project, AppError>, Vec<String>, bool),
    DeleteProject,
    /// Sessions were put in a project, or in none.
    Assigned(Vec<String>, Option<String>, Result<(), AppError>),
    Recoverable(Vec<SessionSummary>),
    Recover,
    Recovered(Result<(), AppError>),
    RecoverLater,
}

fn msg(m: Msg) -> Message {
    Message::Dialog(m)
}

/// Whether a dialog is over the window, so the view under it leaves keys alone.
pub fn is_open(app: &App) -> bool {
    app.dialogs.open.is_some() || !app.dialogs.recoverable.is_empty()
}

/// An export is running, with its spinner (the app ticks to turn it).
pub fn spinning(app: &App) -> bool {
    matches!(app.dialogs.open, Some(Dialog::Export { busy: Some(_), .. }))
}

fn open(app: &mut App, d: Dialog) {
    match d {
        Dialog::Rename { .. } => ui::focus_soon(RENAME_INPUT),
        Dialog::Project(_) => ui::focus_soon(PROJECT_INPUT),
        _ => {}
    }
    app.dialogs.open = Some(d);
}

pub fn update(app: &mut App, m: Msg) -> Task {
    let st = &mut app.dialogs;
    match m {
        Msg::Close => {
            // An export on its way finishes first.
            if !matches!(st.open, Some(Dialog::Export { busy: Some(_), .. })) {
                st.open = None;
            }
            Task::none()
        }
        Msg::Escape => {
            if st.open.is_some() {
                update(app, Msg::Close)
            } else {
                update(app, Msg::RecoverLater)
            }
        }
        Msg::Quit => {
            st.open = None;
            call(app, ops::request_quit, |r| msg(Msg::QuitAnswer(r)))
        }
        // Quitting while recording stops first; the core ends the app once the session is saved.
        Msg::QuitAnswer(Ok(true)) => task::exit(),
        Msg::QuitAnswer(Ok(false)) => Task::none(),
        Msg::QuitAnswer(Err(e)) => {
            actions::report_error(app, &e, "failed");
            Task::none()
        }
        Msg::Title(s) => {
            if let Some(Dialog::Rename { title, .. }) = &mut st.open {
                *title = s;
            }
            Task::none()
        }
        Msg::SaveTitle => {
            let Some(Dialog::Rename { id, title }) = &st.open else { return Task::none() };
            let next = title.trim().to_string();
            if next.is_empty() {
                return Task::none();
            }
            let id = id.clone();
            st.open = None;
            let renamed = actions::run(app, Action::Rename(id, next));
            renamed.chain(super::history::reload(app))
        }
        Msg::ConfirmDelete => {
            let Some(Dialog::Delete { ids, from_selection, .. }) = st.open.take() else { return Task::none() };
            if from_selection {
                super::history::stop_selecting(app);
            }
            actions::run(app, Action::Delete(ids))
        }
        Msg::ExportAs(kind) => {
            let zip = st.zip;
            let Some(Dialog::Export { id, busy }) = &mut st.open else { return Task::none() };
            if busy.is_some() {
                return Task::none();
            }
            *busy = Some(kind);
            let id = id.clone();
            actions::run(app, Action::Export(kind, Some(id), zip)).chain(Task::done(msg(Msg::Exported)))
        }
        Msg::Exported => {
            if matches!(st.open, Some(Dialog::Export { .. })) {
                st.open = None;
            }
            Task::none()
        }
        Msg::Zip(on) => {
            st.zip = on;
            Task::none()
        }
        Msg::ProjectName(s) => {
            if let Some(Dialog::Project(f)) = &mut st.open {
                f.name = s.chars().take(NAME_MAX).collect();
            }
            Task::none()
        }
        Msg::ProjectColor(c) => {
            if let Some(Dialog::Project(f)) = &mut st.open {
                f.color = c.to_string();
            }
            Task::none()
        }
        Msg::SaveProject => {
            let Some(Dialog::Project(f)) = &st.open else { return Task::none() };
            let name = f.name.trim().to_string();
            if name.is_empty() {
                return Task::none();
            }
            let Some(Dialog::Project(f)) = st.open.take() else { return Task::none() };
            match f.editing {
                Some(project) => {
                    let changed = Project { name, color: f.color, ..project };
                    call(
                        app,
                        move |c| ops::update_project(c, &changed),
                        |r| match r {
                            Ok(list) => Message::Projects(list),
                            Err(e) => Message::Failed(e, "saveFailed"),
                        },
                    )
                }
                None => {
                    let (ids, color, open_page) = (f.session_ids, f.color, f.open_page);
                    let ids2 = ids.clone();
                    call(
                        app,
                        move |c| {
                            let made = ops::create_project(c, &name, &color)?;
                            if !ids2.is_empty() {
                                ops::set_project(c, &ids2, Some(made.id.clone()))?;
                            }
                            Ok(made)
                        },
                        move |r| msg(Msg::ProjectMade(r, ids, open_page)),
                    )
                }
            }
        }
        Msg::ProjectMade(r, ids, open_page) => {
            match r {
                Ok(made) => {
                    follow_project(app, &ids, Some(made.id.clone()));
                    if open_page {
                        super::history::show_project(app, made.id);
                    }
                }
                Err(e) => actions::report_error(app, &e, "saveFailed"),
            }
            // Even a half-done save may have made the project.
            Task::done(Message::ProjectsChanged)
        }
        Msg::DeleteProject => {
            let Some(Dialog::DeleteProject(project)) = st.open.take() else { return Task::none() };
            call(
                app,
                move |c| ops::delete_project(c, &project.id),
                |r| match r {
                    // Its sessions now show as in no project.
                    Ok(_) => Message::ProjectsChanged,
                    Err(e) => Message::Failed(e, "deleteFailed"),
                },
            )
        }
        Msg::Assigned(ids, project, r) => match r {
            Ok(()) => {
                follow_project(app, &ids, project);
                Task::done(Message::ProjectsChanged)
            }
            Err(e) => {
                actions::report_error(app, &e, "saveFailed");
                Task::none()
            }
        },
        Msg::Recoverable(list) => {
            st.recoverable = list;
            Task::none()
        }
        Msg::Recover => {
            if st.recovering {
                return Task::none();
            }
            let Some(id) = st.recoverable.first().map(|s| s.id.clone()) else { return Task::none() };
            st.recovering = true;
            call(app, move |c| ops::recover_session(c, &id).map(|_| ()), |r| msg(Msg::Recovered(r)))
        }
        Msg::Recovered(r) => {
            st.recovering = false;
            if !st.recoverable.is_empty() {
                st.recoverable.remove(0);
            }
            match r {
                Ok(()) => {
                    app.toast(t("recovered"), Level::Info);
                    super::history::reload(app)
                }
                Err(e) => {
                    actions::report_error(app, &e, "saveFailed");
                    Task::none()
                }
            }
        }
        Msg::RecoverLater => {
            // All of them, not just the card on screen: the next one looks the same, so putting
            // them off one by one read as a button that did nothing (#17). While one is being
            // recovered it stays until that finishes.
            st.recoverable.truncate(usize::from(st.recovering));
            Task::none()
        }
    }
}

/// The transcript on screen follows a change to its project.
fn follow_project(app: &mut App, ids: &[String], project: Option<String>) {
    if app.live.session_id.as_ref().is_some_and(|s| ids.contains(s)) {
        app.live.project = project;
    }
}

/// A pick in a menu this module's items came from (`dialog:` ids): 書き出し…, a project, or a new one.
pub fn on_menu(app: &mut App, rest: &str) -> Task {
    let (verb, arg) = rest.split_once(':').unwrap_or((rest, ""));
    let ids = |s: &str| s.split(',').filter(|i| !i.is_empty()).map(str::to_string).collect::<Vec<_>>();
    match verb {
        "export" if !arg.is_empty() => {
            ask_export(app, arg.to_string());
            Task::none()
        }
        "assign" => {
            let Some((project, sessions)) = arg.split_once(':') else { return Task::none() };
            let sessions = ids(sessions);
            if sessions.is_empty() {
                return Task::none();
            }
            let project = (project != "-").then(|| project.to_string());
            let (ids2, project2) = (sessions.clone(), project.clone());
            call(app, move |c| ops::set_project(c, &ids2, project2), move |r| msg(Msg::Assigned(sessions, project, r)))
        }
        "new-project" => {
            ask_new_project(app, ids(arg));
            Task::none()
        }
        _ => Task::none(),
    }
}

/// Offers to recover sessions that ended in a crash, once per launch, when nothing records.
pub fn on_launch(app: &App) -> Task {
    if app.rec.state != UiState::Ready {
        return Task::none();
    }
    call(app, ops::list_recoverable, |list| msg(Msg::Recoverable(list)))
}

pub fn ask_confirm_quit(app: &mut App) {
    open(app, Dialog::ConfirmQuit);
}

/// Asks before moving sessions to the Recycle Bin; one is named by its title.
pub fn ask_delete(app: &mut App, ids: Vec<String>, title: Option<String>) {
    let title = title.or_else(|| if ids.len() == 1 { title_of(app, &ids[0]) } else { None });
    open(app, Dialog::Delete { ids, title, from_selection: false });
}

/// 削除 under 履歴's 選択: once they are gone, selection ends.
pub fn ask_delete_selected(app: &mut App, ids: Vec<String>) {
    open(app, Dialog::Delete { ids, title: None, from_selection: true });
}

/// Renames a session shown as a row; an open session renames in its header instead.
pub fn ask_rename(app: &mut App, id: String) {
    let title = title_of(app, &id).unwrap_or_default();
    open(app, Dialog::Rename { id, title });
}

/// A session's title, from 履歴's list or the transcript on screen.
fn title_of(app: &App, id: &str) -> Option<String> {
    super::history::title_of(app, id)
        .or_else(|| (app.live.session_id.as_deref() == Some(id)).then(|| app.live.title.clone()).flatten())
}

/// Escape closes the dialog on top.
pub fn on_key(app: &App, key: &KeyPress) -> Option<Message> {
    (is_open(app) && key.code == "Escape").then_some(msg(Msg::Escape))
}

/// 書き出し: the dialog to pick Markdown, PDF or Typst for a session.
pub fn ask_export(app: &mut App, session_id: String) {
    open(app, Dialog::Export { id: session_id, busy: None });
}

/// 新しいプロジェクト…: makes a project and puts these sessions in it (FR-64).
pub fn ask_new_project(app: &mut App, session_ids: Vec<String>) {
    new_project(app, session_ids, false);
}

/// 新しいプロジェクト… in 履歴's filter menu: the new, empty project's page opens.
pub fn ask_new_project_page(app: &mut App) {
    new_project(app, Vec::new(), true);
}

fn new_project(app: &mut App, session_ids: Vec<String>, open_page: bool) {
    let color = fresh_color(&app.projects).to_string();
    open(app, Dialog::Project(ProjectForm { editing: None, session_ids, open_page, name: String::new(), color }));
}

/// Changes a project's name and colour, from its page in 履歴.
pub fn ask_edit_project(app: &mut App, project: Project) {
    let (name, color) = (project.name.clone(), project.color.clone());
    open(
        app,
        Dialog::Project(ProjectForm { editing: Some(project), session_ids: Vec::new(), open_page: false, name, color }),
    );
}

/// Asks before deleting a project; its sessions stay, in no project.
pub fn ask_delete_project(app: &mut App, project: Project) {
    open(app, Dialog::DeleteProject(project));
}

/// A colour the other projects don't use yet, so a new one stands apart; grey last.
pub fn fresh_color(projects: &[Project]) -> &'static str {
    PROJECT_COLORS.iter().copied().find(|c| *c != "slate" && !projects.iter().any(|p| p.color == *c)).unwrap_or("slate")
}

/// The i18n key naming a colour: `colorBlue`.
fn color_key(c: &str) -> String {
    let mut chars = c.chars();
    match chars.next() {
        Some(first) => format!("color{}{}", first.to_uppercase(), chars.as_str()),
        None => "color".into(),
    }
}

/// A project's colour as a round dot; a hollow ring for none.
pub fn project_mark(color: Option<&str>, size: f32) -> El {
    let d = div().flex_none().size(px(size)).rounded(px(size / 2.0));
    match color.map(project_color) {
        Some(c) => d.bg(c),
        None => d.border_2().border_color(p().line),
    }
    .into_any_element()
}

fn body_text(s: String) -> El {
    theme::t(s, tokens::BODY, p().ink).into_any_element()
}

/// The open dialog over everything, or none.
pub fn view(app: &App, ui: &mut Ui) -> Option<El> {
    let st = &app.dialogs;
    let cancel = || dialog_button(t("cancel"), Variant::Secondary, Some(msg(Msg::Close)));
    let Some(d) = &st.open else { return recovery(app) };
    Some(match d {
        Dialog::ConfirmQuit => dialog(
            t("confirmQuitTitle"),
            Some(body_text(t("confirmQuitBody"))),
            vec![dialog_button(t("quitAndSave"), Variant::Danger, Some(msg(Msg::Quit))), cancel()],
        ),
        Dialog::Rename { title, .. } => {
            let ok = !title.trim().is_empty();
            let field =
                ui.input(Field::new(RENAME_INPUT, title, |s| msg(Msg::Title(s))).on_submit(msg(Msg::SaveTitle)));
            dialog(
                t("rename"),
                Some(field),
                vec![dialog_button(t("save"), Variant::Primary, ok.then(|| msg(Msg::SaveTitle))), cancel()],
            )
        }
        Dialog::Delete { ids, title, .. } => {
            let heading = match (ids.len(), title) {
                (1, Some(title)) => tf("confirmDelete", &[("title", title)]),
                (n, _) => tf("confirmDeleteMany", &[("n", &n)]),
            };
            dialog(
                heading,
                None,
                vec![dialog_button(t("delete"), Variant::Danger, Some(msg(Msg::ConfirmDelete))), cancel()],
            )
        }
        Dialog::Export { busy, .. } => {
            let mut rows = col().gap(px(6.0)).w_full();
            for (kind, glyph, key) in [
                (ExportKind::Markdown, Icon::FileText, "formatMarkdown"),
                (ExportKind::Pdf, Icon::FileType, "formatPdf"),
                (ExportKind::Typst, Icon::FileCode, "formatTypst"),
            ] {
                rows = rows.child(format_row(app, kind, glyph, key, *busy));
            }
            dialog(
                t("export"),
                Some(rows.into_any_element()),
                vec![dialog_button(t("cancel"), Variant::Secondary, busy.is_none().then(|| msg(Msg::Close)))],
            )
        }
        Dialog::Project(f) => {
            let ok = !f.name.trim().is_empty();
            let field = ui.input(
                Field::new(PROJECT_INPUT, &f.name, |s| msg(Msg::ProjectName(s)))
                    .placeholder(t("projectName"))
                    .on_submit(msg(Msg::SaveProject)),
            );
            let body = col().w_full().gap(g(2.0)).child(field).child(color_choice(&f.color));
            let (title, save) =
                if f.editing.is_some() { ("editProject", "save") } else { ("newProject", "createProject") };
            dialog(
                t(title),
                Some(body.into_any_element()),
                vec![dialog_button(t(save), Variant::Primary, ok.then(|| msg(Msg::SaveProject))), cancel()],
            )
        }
        Dialog::DeleteProject(project) => dialog(
            tf("confirmDeleteProject", &[("name", &project.name)]),
            Some(body_text(t("deleteProjectNote"))),
            vec![dialog_button(t("delete"), Variant::Danger, Some(msg(Msg::DeleteProject))), cancel()],
        ),
    })
}

/// Flow D: offers to recover a session that has no end marker, one at a time.
fn recovery(app: &App) -> Option<El> {
    let st = &app.dialogs;
    let s = st.recoverable.first()?;
    let meta = format!(
        "{} · {} · {}",
        date_time_of(&s.started_at),
        hms(s.duration_ms),
        tf("segmentsCount", &[("n", &s.segments)])
    );
    let card = col()
        .w_full()
        .gap(px(2.0))
        .py(px(10.0))
        .px(g(2.0))
        .bg(p().hover_panel)
        .rounded(px(R_PANEL))
        .child(line(s.title.clone(), tokens::LABEL, p().ink))
        .child(line(meta, tokens::SMALL, p().muted));
    let more = st.recoverable.len() - 1;
    let body = if more == 0 {
        card.into_any_element()
    } else {
        col()
            .w_full()
            .gap(g(1.0))
            .child(card)
            .child(line(tf("recoverMore", &[("n", &more)]), tokens::SMALL, p().muted))
            .into_any_element()
    };
    Some(dialog(
        t("recoverTitle"),
        Some(body),
        vec![
            dialog_button(t("recover"), Variant::Primary, (!st.recovering).then(|| msg(Msg::Recover))),
            dialog_button(t("recoverLater"), Variant::Secondary, Some(msg(Msg::RecoverLater))),
        ],
    ))
}

/// One of 書き出し's formats; Markdown carries its ZIP switch.
fn format_row(app: &App, kind: ExportKind, glyph: Icon, key: &str, busy: Option<ExportKind>) -> El {
    let idle = busy.is_none();
    let mut content = row()
        .w_full()
        .gap(g(2.0))
        .child(icon(glyph, 18.0, p().muted))
        .child(line(t(key), tokens::LABEL, p().ink).flex_1().min_w_0());
    if busy == Some(kind) {
        content = content.child(widgets::spinner(16.0, app.now));
    }
    if kind == ExportKind::Markdown {
        let zip = app.dialogs.zip;
        let toggle = idle.then(|| msg(Msg::Zip(!zip)));
        let label = row()
            .gap(g(1.0))
            .child(line("ZIP", tokens::SMALL, p().muted).bold())
            .child(widgets::switch(zip, toggle.clone()));
        content = content.child(with_tip(widgets::bare(label, toggle, false), t("zipToggle")));
    }
    let pal = p();
    let mut b = clickable(idle.then(|| msg(Msg::ExportAs(kind))), ("format", key))
        .flex()
        .items_center()
        .w_full()
        .h(px(46.0))
        .px(g(2.0))
        .bg(pal.surface)
        .border_1()
        .border_color(pal.line)
        .rounded(px(R_PANEL))
        .text_color(pal.ink)
        .child(content);
    if idle {
        let (sunk, strong) = (pal.hover_panel, pal.line);
        b = b.hover(move |s| s.bg(sunk).border_color(strong));
    }
    b.into_any_element()
}

/// The colour choice: one square per colour, the chosen one ringed.
fn color_choice(chosen: &str) -> El {
    let mut r = row().gap(px(6.0));
    for c in PROJECT_COLORS {
        let on = c == chosen;
        let strong = p().line;
        let mut b = clickable(Some(msg(Msg::ProjectColor(c))), ("color", c))
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.0))
            .border_1()
            .rounded(px(R_ROW))
            .border_color(if on { p().ink } else { theme::Color::TRANSPARENT })
            .child(project_mark(Some(c), 16.0));
        // Empty on the chosen one, so GPUI keeps tracking the pointer (see `screens::titlebar::tabs`).
        b = b.hover(move |s| if on { s } else { s.border_color(strong) });
        r = r.child(with_tip(b, t(&color_key(c))));
    }
    r.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, color: &str) -> Project {
        Project { id: id.into(), name: id.into(), color: color.into() }
    }

    #[test]
    fn a_new_project_gets_a_colour_no_other_uses_grey_last() {
        assert_eq!(fresh_color(&[]), "blue");
        assert_eq!(fresh_color(&[p("a", "blue"), p("b", "teal")]), "green");
        let all: Vec<_> = PROJECT_COLORS.iter().filter(|c| **c != "slate").map(|c| p(c, c)).collect();
        assert_eq!(fresh_color(&all), "slate");
    }

    fn crashed(id: &str) -> SessionSummary {
        SessionSummary {
            id: id.into(),
            title: "システム全体".into(),
            started_at: "2026-11-10T09:00:00+09:00".into(),
            duration_ms: 60_000,
            folder: String::new(),
            segments: 3,
            screenshots: 0,
            recoverable: true,
            sources: vec![],
            preview: String::new(),
            activity: chidori_core::session::recovery::Activity { others: vec![], me: vec![], shots: vec![] },
            project: None,
        }
    }

    fn offered(app: &mut App, ids: &[&str]) {
        let _ = update(app, Msg::Recoverable(ids.iter().map(|i| crashed(i)).collect()));
    }

    #[test]
    fn later_puts_off_every_interrupted_session() {
        let (mut app, _dir) = crate::preview::app();
        offered(&mut app, &["a", "b", "c"]);
        assert!(is_open(&app));
        let _ = update(&mut app, Msg::RecoverLater);
        assert!(!is_open(&app));
    }

    #[test]
    fn later_during_a_recovery_keeps_the_one_being_recovered() {
        let (mut app, _dir) = crate::preview::app();
        offered(&mut app, &["a", "b", "c"]);
        app.dialogs.recovering = true;
        let _ = update(&mut app, Msg::RecoverLater);
        assert_eq!(app.dialogs.recoverable.len(), 1);
        assert_eq!(app.dialogs.recoverable[0].id, "a");
    }

    #[test]
    fn recovering_one_offers_the_next() {
        let (mut app, _dir) = crate::preview::app();
        offered(&mut app, &["a", "b"]);
        let _ = update(&mut app, Msg::Recovered(Ok(())));
        assert_eq!(app.dialogs.recoverable.len(), 1);
        assert!(is_open(&app));
    }

    #[test]
    fn colours_have_names() {
        crate::i18n::set_locale(chidori_core::settings::Locale::Ja);
        assert_eq!(color_key("blue"), "colorBlue");
        for c in PROJECT_COLORS {
            assert_ne!(t(&color_key(c)), color_key(c), "{c} has no name");
        }
    }
}
