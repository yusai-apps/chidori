//! What the UI asks of the core (section 14), as plain blocking calls. The UI runs them off its
//! own thread; file dialogs are the UI's, so exports take the path the user picked.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::asr::worker::{EngineState, EngineStatus};
use crate::audio::source::{AudioApp, Device};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::events::{self, StatePayload};
use crate::export::{archive, html, markdown, plaintext, saving, typst_pdf};
use crate::host::AppHandle;
use crate::models::{benchmark, manager};
use crate::recorder::{self, Phase, ShotResponse, SourceConfig, StartResponse};
use crate::session::log::{LogEvent, SessionLog};
use crate::session::model::{Segment, Session, TimelineItem};
use crate::session::projects::{self, Project};
use crate::session::recovery::{self, SessionSummary};
use crate::settings::{Layout, Settings};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub os_build: u32,
    pub app_loopback_supported: bool,
    /// "gpu" | "cpu" | "loading"
    pub device: &'static str,
    pub device_name: Option<String>,
    pub engine: EngineStatus,
    pub gpu_compiled: bool,
    pub hotkey_errors: std::collections::BTreeMap<String, String>,
    pub state: StatePayload,
    pub models_dir: PathBuf,
    pub log_dir: PathBuf,
    pub recommended_model: String,
    pub total_ram_bytes: u64,
    pub setup_done: bool,
    pub gpu_failed: bool,
}

pub fn app_info(app: &AppHandle) -> AppInfo {
    let st = app.state();
    let engine = st.worker.status();
    let device = match engine.state {
        EngineState::Ready if engine.gpu => "gpu",
        EngineState::Ready => "cpu",
        _ => "loading",
    };
    let internal = st.internal.lock().clone();
    AppInfo {
        version: app.version().to_string(),
        os_build: crate::platform::os_build(),
        app_loopback_supported: crate::platform::app_loopback_supported(),
        device,
        device_name: engine.device_name.clone(),
        engine,
        gpu_compiled: crate::asr::engine::gpu_compiled(),
        hotkey_errors: st.hotkey_errors.lock().clone(),
        state: recorder::state_payload(st),
        models_dir: st.models_dir.clone(),
        log_dir: app.paths().logs.clone(),
        recommended_model: crate::models::catalog::recommend(crate::platform::total_memory()).to_string(),
        total_ram_bytes: crate::platform::total_memory(),
        setup_done: internal.setup_done,
        gpu_failed: internal.gpu_failure.is_some(),
    }
}

pub fn settings(app: &AppHandle) -> Settings {
    app.state().settings.read().clone()
}

/// Merges a partial settings object (validated as on load) and saves it. Returns the settings
/// before and after, so the app can re-register hotkeys or restyle its window when they changed.
pub fn set_settings(app: &AppHandle, partial: &Value) -> (Settings, Settings) {
    let st = app.state();
    // Read and written under one lock, so a change made meanwhile (the source a recording
    // remembers) isn't overwritten with the settings as they were.
    let (old, new) = {
        let mut settings = st.settings.write();
        let old = settings.clone();
        let new = crate::settings::merge_partial(&old, partial, &st.default_output_root);
        *settings = new.clone();
        (old, new)
    };
    st.persist_settings(app);
    if (new.model_id != old.model_id || new.use_gpu != old.use_gpu) && matches!(&*st.recorder.lock(), Phase::Idle) {
        st.load_selected_model();
    }
    if new.locale != old.locale {
        recorder::emit_state(app, st);
    }
    (old, new)
}

pub fn list_audio_apps() -> AppResult<Vec<AudioApp>> {
    #[cfg(windows)]
    {
        use crate::audio::source::AudioAppLister;
        crate::audio::win::sessions::WinAudioAppLister.list().map_err(AppError::from)
    }
    #[cfg(not(windows))]
    Ok(Vec::new())
}

pub fn list_mic_devices() -> AppResult<Vec<Device>> {
    crate::audio::list_mic_devices().map_err(AppError::from)
}

/// The apps whose loudness the source picker shows, by root PID; none stops reading their meters.
pub fn watch_app_levels(app: &AppHandle, root_pids: Vec<u32>) {
    #[cfg(windows)]
    app.state().meters.watch(app.events(), root_pids);
    #[cfg(not(windows))]
    let _ = (app, root_pids);
}

pub fn start_recording(app: &AppHandle, source: SourceConfig, title: Option<String>) -> AppResult<StartResponse> {
    recorder::start(app, app.state(), source, title)
}

/// Records more onto a saved session (FR-08).
pub fn continue_recording(app: &AppHandle, session_id: String, source: SourceConfig) -> AppResult<StartResponse> {
    recorder::continue_session(app, app.state(), source, session_id)
}

/// What the file dialog offers to transcribe (FR-18): what Windows plays, video included. Any other
/// file can still be picked under 「すべてのファイル」, and Media Foundation says whether it reads it.
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "wma", "amr", "3gp", "mp4", "m4v", "mov", "mkv", "webm", "avi",
    "wmv", "mpg", "mpeg", "ts", "mts", "m2ts",
];

/// Transcribes an audio or video file into a new session (FR-18).
pub fn transcribe_file(app: &AppHandle, path: PathBuf) -> AppResult<StartResponse> {
    recorder::transcribe_file(app, app.state(), path)
}

pub fn stop_recording(app: &AppHandle) -> AppResult<()> {
    recorder::stop(app, app.state())
}

pub fn cancel_finishing(app: &AppHandle) {
    recorder::cancel_finishing(app.state());
}

pub fn pause_recording(app: &AppHandle) -> AppResult<()> {
    recorder::pause(app, app.state())
}

pub fn resume_recording(app: &AppHandle) -> AppResult<()> {
    recorder::resume(app, app.state())
}

pub fn switch_to_system(app: &AppHandle) -> AppResult<()> {
    recorder::switch_to_system(app, app.state())
}

/// Switches what is recorded without stopping (FR-17).
pub fn switch_source(app: &AppHandle, source: SourceConfig) -> AppResult<()> {
    recorder::switch_source(app, app.state(), source)
}

pub fn take_screenshot(app: &AppHandle) -> AppResult<ShotResponse> {
    recorder::take_screenshot(app, app.state())
}

/// The windows the camera's menu offers (FR-34).
pub fn list_windows(app: &AppHandle) -> AppResult<Vec<crate::screenshot::WindowInfo>> {
    use crate::screenshot::ScreenCapturer as _;
    app.state().capturer.windows().map_err(|e| AppError::internal(format!("{e:#}")))
}

pub fn set_shot_window(app: &AppHandle, window: Option<crate::screenshot::WindowInfo>) -> AppResult<()> {
    recorder::set_shot_window(app, app.state(), window)
}

/// Leaves transcription for after Stop, or takes it up again (FR-25).
pub fn defer_transcription(app: &AppHandle, deferred: bool) -> AppResult<()> {
    recorder::set_deferred(app, app.state(), deferred)
}

pub fn add_cut(app: &AppHandle) -> AppResult<()> {
    recorder::add_cut(app.state())
}

/// Marks the line being said as important; `false` when nothing has been said yet.
pub fn mark_current_line(app: &AppHandle) -> AppResult<bool> {
    recorder::mark_current(app.state())
}

fn root(st: &AppState) -> PathBuf {
    PathBuf::from(&st.settings.read().output.root)
}

/// The folder of a session: the live one, or a search under the output root.
pub fn folder_of(app: &AppHandle, session_id: &str) -> AppResult<PathBuf> {
    let st = app.state();
    match &*st.recorder.lock() {
        Phase::Recording(a) if a.store.id == session_id => return Ok(a.store.folder.clone()),
        Phase::Finishing { session_id: id, folder, .. } if id == session_id => return Ok(folder.clone()),
        _ => {}
    }
    recovery::find_folder(&root(st), session_id)
        .ok_or_else(|| AppError::internal(format!("session {session_id} not found")))
}

fn live_session(st: &AppState, session_id: &str) -> Option<Session> {
    match &*st.recorder.lock() {
        Phase::Recording(a) if a.store.id == session_id => Some(a.store.snapshot()),
        _ => None,
    }
}

fn load_session(app: &AppHandle, session_id: &str) -> AppResult<(Session, PathBuf)> {
    let folder = folder_of(app, session_id)?;
    if let Some(s) = live_session(app.state(), session_id) {
        return Ok((s, folder));
    }
    let (session, _, _) = recovery::load(&folder)?;
    Ok((session, folder))
}

#[derive(Debug, Clone)]
pub struct SessionView {
    pub session: Session,
    pub folder: PathBuf,
    /// How the session sounded, for its line; missing when the session has no `levels.bin`.
    pub sound: Option<recovery::Sound>,
}

pub fn get_session(app: &AppHandle, session_id: &str) -> AppResult<SessionView> {
    let (session, folder) = load_session(app, session_id)?;
    let sound = recovery::sound(&session, &folder);
    Ok(SessionView { session, folder, sound })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFormat {
    Plain,
    Agent,
    Markdown,
}

/// Copies some text (one line of a transcript) to the clipboard.
pub fn copy_text(app: &AppHandle, text: &str) -> AppResult<()> {
    app.shell().write_clipboard(text).map_err(|e| AppError::internal(format!("{e:#}")))
}

/// Copies a transcript to the clipboard; returns how many characters.
pub fn copy_transcript(app: &AppHandle, session_id: &str, format: CopyFormat) -> AppResult<usize> {
    let (session, _) = load_session(app, session_id)?;
    let settings = app.state().settings.read().clone();
    let opts = settings.export_options();
    let text = match format {
        CopyFormat::Plain => plaintext::plain(&session, &opts),
        CopyFormat::Agent => plaintext::for_agent(&session, &opts, &settings.vocabulary),
        CopyFormat::Markdown => markdown::body(&session, &opts),
    };
    app.shell().write_clipboard(&text).map_err(|e| AppError::internal(format!("{e:#}")))?;
    Ok(text.chars().count())
}

/// 書き出し → Markdown. Into `out`, a folder the user picked: a new folder in it named after the
/// session, holding `transcript.md` and `images/`. With `zip`, `out` is the archive's path.
pub fn export_markdown(app: &AppHandle, session_id: &str, zip: bool, out: &Path) -> AppResult<PathBuf> {
    let (session, folder) = load_session(app, session_id)?;
    let opts = app.state().settings.read().export_options();
    if zip {
        archive::write(&session, &opts, &folder, out, app.version())?;
        return Ok(out.to_path_buf());
    }
    Ok(saving::markdown_folder(&session, &opts, &folder, out, app.version())?)
}

pub enum PdfExport {
    Pdf(PathBuf),
    /// Typst failed; the print HTML was saved here instead, for the browser to print.
    Html(PathBuf),
}

/// 書き出し → PDF, typeset with Typst. If Typst fails, the print HTML is saved beside where the PDF
/// would have gone (screenshots linked where they are), with a warning notice.
pub fn export_pdf(app: &AppHandle, session_id: &str, out: &Path) -> AppResult<PdfExport> {
    let (session, folder) = load_session(app, session_id)?;
    let opts = app.state().settings.read().export_options();
    let rendered = typst_pdf::render(&session, &opts, &folder, app.version());
    match rendered.and_then(|pdf| recovery::write_atomic(out, &pdf).map_err(anyhow::Error::from)) {
        Ok(()) => return Ok(PdfExport::Pdf(out.to_path_buf())),
        Err(e) => tracing::warn!("PDF export failed, falling back to HTML: {e:#}"),
    }
    let html_doc = html::render_linking(&session, &opts, &|shot| saving::file_url(&folder.join(&shot.file)));
    let fallback = out.with_extension("html");
    std::fs::write(&fallback, html_doc.as_bytes())?;
    events::notice(app, events::Notice::new(events::NoticeLevel::Warn, ErrorCode::PdfFailed.as_str(), "pdfFallback"));
    Ok(PdfExport::Html(fallback))
}

/// 書き出し → Typst: the document the PDF is made from, with its screenshots copied beside it
/// (`<name>_images/`), so `typst compile` there makes the same PDF.
pub fn export_typst(app: &AppHandle, session_id: &str, out: &Path) -> AppResult<PathBuf> {
    let (session, folder) = load_session(app, session_id)?;
    let opts = app.state().settings.read().export_options();
    let links = saving::copy_images(&session, &folder, out)?;
    let src = typst_pdf::typst_source(&session, &opts, &links, app.version());
    recovery::write_atomic(out, src.as_bytes())?;
    Ok(out.to_path_buf())
}

/// Whether a path is inside Chidori's folders (the output root, models, logs): the only places
/// the UI opens in Explorer.
pub fn allowed_path(app: &AppHandle, path: &Path) -> bool {
    let st = app.state();
    let canon = |p: &Path| std::fs::canonicalize(p).ok();
    let Some(target) = canon(path) else { return false };
    let roots = [root(st), st.models_dir.clone(), app.paths().logs.clone()];
    roots.iter().filter_map(|r| canon(r)).any(|r| target.starts_with(r))
}

pub fn list_recoverable(app: &AppHandle) -> Vec<SessionSummary> {
    let st = app.state();
    let active = st.recorder.lock().session_id();
    recovery::recoverable(&root(st), active.as_deref())
}

pub fn recover_session(app: &AppHandle, session_id: &str) -> AppResult<PathBuf> {
    let folder = folder_of(app, session_id)?;
    let opts = app.state().settings.read().export_options();
    Ok(recovery::recover(&folder, &opts, app.version())?)
}

pub fn list_sessions(app: &AppHandle) -> Vec<SessionSummary> {
    let st = app.state();
    let active = st.recorder.lock().session_id();
    recovery::list(&root(st)).into_iter().filter(|s| Some(&s.id) != active.as_ref()).collect()
}

/// The projects in the output root (FR-64).
pub fn list_projects(app: &AppHandle) -> Vec<Project> {
    projects::load(&root(app.state()))
}

pub fn create_project(app: &AppHandle, name: &str, color: &str) -> AppResult<Project> {
    Ok(projects::create(&root(app.state()), name, color)?)
}

/// Renames or recolours a project; returns the whole list.
pub fn update_project(app: &AppHandle, project: &Project) -> AppResult<Vec<Project>> {
    Ok(projects::update(&root(app.state()), project)?)
}

/// Deletes a project; its sessions stay, in no project.
pub fn delete_project(app: &AppHandle, id: &str) -> AppResult<Vec<Project>> {
    Ok(projects::delete(&root(app.state()), id)?)
}

/// Puts sessions in a project, or (`None`) in none; the one being recorded too.
pub fn set_project(app: &AppHandle, session_ids: &[String], project: Option<String>) -> AppResult<()> {
    for id in session_ids {
        edit(app, id, LogEvent::ProjectSet { project: project.clone() })?;
    }
    Ok(())
}

pub fn delete_session(app: &AppHandle, session_id: &str) -> AppResult<()> {
    if app.state().recorder.lock().session_id().as_deref() == Some(session_id) {
        return Err(AppError::internal("cannot delete the session being recorded"));
    }
    let folder = folder_of(app, session_id)?;
    trash::delete(&folder).map_err(|e| AppError::internal(format!("moving to the Recycle Bin failed: {e}")))
}

/// Moves several sessions to the Recycle Bin in one go (History's 選択); the one being recorded
/// stays. Returns how many went.
pub fn delete_sessions(app: &AppHandle, session_ids: &[String]) -> AppResult<usize> {
    let st = app.state();
    let active = st.recorder.lock().session_id();
    let wanted: std::collections::HashSet<&str> =
        session_ids.iter().map(String::as_str).filter(|id| active.as_deref() != Some(*id)).collect();
    let folders: Vec<PathBuf> = recovery::list(&root(st))
        .into_iter()
        .filter(|s| wanted.contains(s.id.as_str()))
        .map(|s| PathBuf::from(s.folder))
        .collect();
    if folders.is_empty() {
        return Ok(0);
    }
    trash::delete_all(&folders).map_err(|e| AppError::internal(format!("moving to the Recycle Bin failed: {e}")))?;
    Ok(folders.len())
}

/// Applies an edit: through the live store while recording, else appended to the log and
/// the outputs regenerated.
fn edit(app: &AppHandle, session_id: &str, event: LogEvent) -> AppResult<Session> {
    let st = app.state();
    if let Phase::Recording(a) = &*st.recorder.lock()
        && a.store.id == session_id
    {
        a.store.record(event);
        return Ok(a.store.snapshot());
    }
    if matches!(&*st.recorder.lock(), Phase::Finishing { session_id: id, .. } if id == session_id) {
        return Err(AppError::internal("the session is still being saved"));
    }
    let folder = folder_of(app, session_id)?;
    SessionLog::open_append(&folder)?.append(&event)?;
    let (session, _, _) = recovery::load(&folder)?;
    let opts = st.settings.read().export_options();
    recovery::save_outputs(&folder, &session, &opts, app.version())?;
    Ok(session)
}

pub fn update_segment(app: &AppHandle, session_id: &str, segment_id: &str, text: &str) -> AppResult<Segment> {
    let session =
        edit(app, session_id, LogEvent::SegmentEdited { id: segment_id.to_string(), text: text.trim().to_string() })?;
    session
        .items
        .into_iter()
        .find_map(|i| match i {
            TimelineItem::Segment(s) if s.id == segment_id => Some(s),
            _ => None,
        })
        .ok_or_else(|| AppError::internal("segment not found"))
}

pub fn mark_segment(app: &AppHandle, session_id: &str, segment_id: &str, important: bool) -> AppResult<Segment> {
    let session = edit(app, session_id, LogEvent::SegmentMarked { id: segment_id.to_string(), important })?;
    session.segments().find(|s| s.id == segment_id).cloned().ok_or_else(|| AppError::internal("segment not found"))
}

pub fn delete_segment(app: &AppHandle, session_id: &str, segment_id: &str) -> AppResult<()> {
    edit(app, session_id, LogEvent::SegmentRemoved { id: segment_id.to_string(), reason: "user".into() })?;
    Ok(())
}

pub fn delete_screenshot(app: &AppHandle, session_id: &str, id: &str) -> AppResult<()> {
    edit(app, session_id, LogEvent::ScreenshotDeleted { id: id.to_string() })?;
    Ok(())
}

pub fn set_caption(app: &AppHandle, session_id: &str, id: &str, caption: &str) -> AppResult<()> {
    edit(app, session_id, LogEvent::CaptionSet { id: id.to_string(), caption: caption.to_string() })?;
    Ok(())
}

pub fn rename_session(app: &AppHandle, session_id: &str, title: &str) -> AppResult<()> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::internal("empty title"));
    }
    edit(app, session_id, LogEvent::TitleChanged { title })?;
    Ok(())
}

pub fn models_list(app: &AppHandle) -> Vec<manager::ModelEntry> {
    manager::list(app.state())
}

pub fn model_download(app: &AppHandle, id: &str) -> AppResult<()> {
    manager::start_download(app, app.state(), id)
}

pub fn model_cancel(app: &AppHandle, id: &str) {
    manager::cancel_download(app.state(), id);
}

pub fn model_select(app: &AppHandle, id: &str) -> AppResult<()> {
    manager::select(app, app.state(), id)
}

pub fn model_delete(app: &AppHandle, id: &str) -> AppResult<()> {
    manager::delete(app, app.state(), id)
}

pub fn model_import(app: &AppHandle, id: &str, path: &Path) -> AppResult<()> {
    manager::import(app, app.state(), id, path)
}

pub fn run_benchmark(app: &AppHandle, model_id: &str) -> AppResult<benchmark::BenchmarkResult> {
    benchmark::run(app, app.state(), model_id)
}

pub fn retry_gpu(app: &AppHandle) {
    let st = app.state();
    st.internal.lock().gpu_failure = None;
    st.persist_internal(app);
    if matches!(&*st.recorder.lock(), Phase::Idle) {
        st.load_selected_model();
    }
}

pub fn mic_test(app: &AppHandle, seconds: u64) -> AppResult<crate::mic_test::MicTestResult> {
    crate::mic_test::run(app, app.state(), seconds.clamp(3, 20))
}

pub fn complete_setup(app: &AppHandle) {
    let st = app.state();
    st.internal.lock().setup_done = true;
    st.persist_internal(app);
    recorder::emit_state(app, st);
}

pub fn set_layout(app: &AppHandle, layout: Layout) {
    let st = app.state();
    st.settings.write().window.layout = layout;
    st.persist_settings(app);
}

/// Quits now when idle (`true`); while recording, stops and quits once the session is saved.
pub fn request_quit(app: &AppHandle) -> AppResult<bool> {
    let st = app.state();
    if matches!(&*st.recorder.lock(), Phase::Idle) {
        return Ok(true);
    }
    st.quit_after_finish.store(true, std::sync::atomic::Ordering::SeqCst);
    if matches!(&*st.recorder.lock(), Phase::Recording(_)) {
        recorder::stop(app, st)?;
    }
    Ok(false)
}
