//! Start/stop orchestration and thread lifecycle (section 6).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::host::AppHandle;
use chrono::{Local, SecondsFormat};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use crate::asr::worker::{AsrConfig, EngineState, SessionCtx};
use crate::audio::decode::DecodeError;
use crate::audio::dsp::{self, DspParams, DspSummary, JobSink, SessionIds};
use crate::audio::file_source::{FileSource, Pace};
use crate::audio::level::FLOOR_DBFS;
use crate::audio::source::{AudioSource, SourceStatus};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::events::{self, EventSink, Notice, NoticeLevel, RecordedSources, StatePayload, UiState};
use crate::models::catalog;
use crate::platform;
use crate::screenshot::{self, ScreenCapturer};
use crate::segmenter::SegmenterConfig;
use crate::session::audio::{self, AudioRecorder};
use crate::session::levels::LevelWriter;
use crate::session::log::{LogEvent, SessionLog};
use crate::session::model::{MarkerKind, ModelRef, SCHEMA_VERSION, Session, SourceId, SourceInfo};
use crate::session::store::SessionStore;
use crate::session::{paths, recovery};
use crate::settings::{LastApp, Locale, ScreenshotTarget, Settings, SourceMode};
use crate::state::AppState;

use crate::audio::source::CaptureTarget;

pub const ME: &str = "自分";
pub const OTHERS: &str = "相手";
/// This far behind, a banner offers to leave transcription for after Stop (FR-25).
const DEFER_OFFER_LAG_MS: u64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppRef {
    pub exe: String,
    pub root_pid: u32,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    pub mode: SourceMode,
    #[serde(default)]
    pub app: Option<AppRef>,
    #[serde(default)]
    pub include_mic: bool,
    #[serde(default)]
    pub mic_device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResponse {
    pub session_id: String,
    pub folder: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotResponse {
    pub id: String,
    pub t_ms: u64,
}

#[derive(Clone)]
struct PlannedSource {
    id: SourceId,
    target: CaptureTarget,
}

struct RunningSource {
    id: SourceId,
    source: Box<dyn AudioSource>,
    dsp: JoinHandle<DspSummary>,
}

pub struct Active {
    pub store: Arc<SessionStore>,
    ctx: Arc<SessionCtx>,
    /// Session time zero in QPC units: the start, or for a continued session its end moved back
    /// by its length, which may lie before boot (so signed).
    pub t0: i64,
    /// What is being recorded, as chosen; switching the source changes it (FR-17).
    config: SourceConfig,
    pub sources: RecordedSources,
    plan: Vec<PlannedSource>,
    running: Vec<RunningSource>,
    status_rx: Receiver<SourceStatus>,
    status_tx: Sender<SourceStatus>,
    pub paused_at: Option<u64>,
    /// Time spent paused before the current pause; the timer leaves it out.
    paused_ms: u64,
    lag_banner_shown: bool,
    /// Transcription waits for Stop (FR-25); offered once, a minute behind.
    deferred: bool,
    defer_offered: bool,
    pub app_root_pid: Option<u32>,
    pub app_name: Option<String>,
    /// A window picked from the camera's menu: screenshots take it until the recording ends (FR-34).
    pub shot_window: Option<screenshot::WindowInfo>,
    segmenter: SegmenterConfig,
    partials: bool,
    /// The session's sound, kept for its picture (`levels.bin`); `None` if the file failed.
    levels: Option<LevelWriter>,
    /// The recording's sound, kept to play back (FR-09); `None` when off or if the file failed.
    audio: Option<AudioRecorder>,
}

impl Active {
    pub fn now_ms(&self) -> u64 {
        ((platform::qpc_now_100ns() as i64).saturating_sub(self.t0).max(0) / 10_000) as u64
    }

    /// Recorded time for the timer: stands still while paused. Session times (`now_ms`) keep
    /// counting, so transcript clock times stay true.
    pub fn elapsed_ms(&self) -> u64 {
        self.paused_at.unwrap_or_else(|| self.now_ms()).saturating_sub(self.paused_ms)
    }
}

pub enum Phase {
    Idle,
    Recording(Box<Active>),
    Finishing { session_id: String, folder: PathBuf, sources: RecordedSources, cancel: Arc<AtomicBool> },
}

impl Phase {
    pub fn ui_state(&self) -> Option<UiState> {
        match self {
            Phase::Idle => None,
            Phase::Recording(a) if a.paused_at.is_some() => Some(UiState::Paused),
            Phase::Recording(_) => Some(UiState::Recording),
            Phase::Finishing { .. } => Some(UiState::Finishing),
        }
    }

    pub fn session_id(&self) -> Option<String> {
        match self {
            Phase::Idle => None,
            Phase::Recording(a) => Some(a.store.id.clone()),
            Phase::Finishing { session_id, .. } => Some(session_id.clone()),
        }
    }
}

fn events_of(app: &AppHandle) -> Arc<dyn EventSink> {
    Arc::new(app.clone())
}

/// The current UI state payload (also used by `get_app_info`).
pub fn state_payload(st: &AppState) -> StatePayload {
    let rec = st.recorder.lock();
    match &*rec {
        Phase::Recording(a) => StatePayload {
            state: if a.paused_at.is_some() { UiState::Paused } else { UiState::Recording },
            session_id: Some(a.store.id.clone()),
            elapsed_ms: Some(a.elapsed_ms()),
            sources: Some(a.sources.clone()),
            folder: Some(a.store.folder.to_string_lossy().into_owned()),
            shot_window: a.shot_window.clone(),
            deferred: a.deferred,
        },
        Phase::Finishing { session_id, folder, sources, .. } => StatePayload {
            state: UiState::Finishing,
            session_id: Some(session_id.clone()),
            elapsed_ms: None,
            sources: Some(sources.clone()),
            folder: Some(folder.to_string_lossy().into_owned()),
            shot_window: None,
            deferred: false,
        },
        Phase::Idle => {
            drop(rec);
            let state = if st.any_model_installed() { UiState::Ready } else { UiState::NeedsModel };
            StatePayload {
                state,
                session_id: None,
                elapsed_ms: None,
                sources: None,
                folder: None,
                shot_window: None,
                deferred: false,
            }
        }
    }
}

pub fn emit_state(app: &AppHandle, st: &AppState) {
    let payload = state_payload(st);
    app.shell().state_changed(payload.state);
    events::emit(app, events::Event::State(payload));
}

/// What a new session is called by default: the app's name, or the source in the words the UI
/// uses for it in its language at the time (`srcSystem`, `srcMic`). The title is the user's to rename.
fn source_name(config: &SourceConfig, locale: Locale) -> String {
    let ja = locale == Locale::Ja;
    match config.mode {
        SourceMode::Mic => (if ja { "マイク" } else { "Mic" }).into(),
        SourceMode::System => (if ja { "システム全体" } else { "All system audio" }).into(),
        SourceMode::App => config
            .app
            .as_ref()
            .map(|a| a.name.clone().unwrap_or_else(|| exe_stem(&a.exe)))
            .unwrap_or_else(|| (if ja { "アプリ" } else { "App" }).into()),
    }
}

fn exe_stem(exe: &str) -> String {
    std::path::Path::new(exe).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| exe.to_string())
}

/// The sources a recording captures, for the UI to name in its own language.
fn recorded_sources(config: &SourceConfig, app_name: &str) -> RecordedSources {
    let main = match config.mode {
        SourceMode::Mic => SourceId::Mic,
        SourceMode::System => SourceId::System,
        SourceMode::App => SourceId::App,
    };
    let mut ids = vec![main];
    if main != SourceId::Mic && config.include_mic {
        ids.push(SourceId::Mic);
    }
    RecordedSources { ids, app_name: (main == SourceId::App).then(|| app_name.to_string()) }
}

/// What a source choice captures, and how the session names it.
struct Planned {
    plan: Vec<PlannedSource>,
    infos: Vec<SourceInfo>,
    /// The app's process while it runs, for screenshots of its window.
    app_root_pid: Option<u32>,
}

fn plan_sources(config: &SourceConfig, app_name: &str) -> Planned {
    let mut plan: Vec<PlannedSource> = Vec::new();
    let mut infos: Vec<SourceInfo> = Vec::new();
    let mic_target = CaptureTarget::Mic { device_id: config.mic_device_id.clone() };
    let mic_name = crate::audio::mic_device_name(config.mic_device_id.clone());
    let mut app_root_pid = None;
    match config.mode {
        SourceMode::Mic => {
            plan.push(PlannedSource { id: SourceId::Mic, target: mic_target.clone() });
            infos.push(SourceInfo {
                id: SourceId::Mic,
                label: ME.into(),
                exe: None,
                device: mic_name.clone(),
                name: None,
            });
        }
        SourceMode::System => {
            plan.push(PlannedSource { id: SourceId::System, target: CaptureTarget::System });
            infos.push(SourceInfo { id: SourceId::System, label: OTHERS.into(), exe: None, device: None, name: None });
        }
        SourceMode::App => {
            let a = config.app.clone().expect("checked: app mode has an app");
            // The app may not be running yet; capture waits for it (and reattaches, FR-14).
            let alive = a.root_pid != 0 && platform::process_alive(a.root_pid);
            let root_pid = if alive { a.root_pid } else { 0 };
            app_root_pid = alive.then_some(a.root_pid);
            plan.push(PlannedSource {
                id: SourceId::App,
                target: CaptureTarget::App { root_pid, exe: PathBuf::from(&a.exe) },
            });
            infos.push(SourceInfo {
                id: SourceId::App,
                label: OTHERS.into(),
                exe: std::path::Path::new(&a.exe).file_name().map(|f| f.to_string_lossy().into_owned()),
                device: None,
                name: Some(app_name.to_string()),
            });
        }
    }
    if config.mode != SourceMode::Mic && config.include_mic {
        plan.push(PlannedSource { id: SourceId::Mic, target: mic_target });
        infos.push(SourceInfo { id: SourceId::Mic, label: ME.into(), exe: None, device: mic_name, name: None });
    }
    Planned { plan, infos, app_root_pid }
}

/// Checks a source choice before anything starts.
fn check_source(config: &SourceConfig) -> AppResult<()> {
    if config.mode == SourceMode::App {
        if !platform::app_loopback_supported() {
            return Err(AppError::new(ErrorCode::AppLoopbackUnsupported, "app capture needs Windows 11"));
        }
        if config.app.is_none() {
            return Err(AppError::internal("no app selected"));
        }
    }
    Ok(())
}

/// The source as exports name it, in Japanese like the rest of an export: `Teams + マイク`.
fn source_words(config: &SourceConfig, app_name: &str) -> String {
    let main = match config.mode {
        SourceMode::Mic => return "マイク".into(),
        SourceMode::System => "システム全体",
        SourceMode::App => app_name,
    };
    if config.include_mic { format!("{main} + マイク") } else { main.to_string() }
}

/// Remembers the source for the next recording and for the Start/Stop hotkey.
fn remember_source(app: &AppHandle, st: &AppState, config: &SourceConfig) {
    {
        let mut s = st.settings.write();
        s.source.mode = config.mode;
        s.source.include_mic = config.include_mic;
        s.source.mic_device_id = config.mic_device_id.clone();
        if let Some(a) = &config.app {
            s.source.app =
                Some(LastApp { exe: a.exe.clone(), name: a.name.clone().unwrap_or_else(|| exe_stem(&a.exe)) });
        }
    }
    st.persist_settings(app);
}

/// The selected model, which the worker starts loading if it hasn't got it: capture may start
/// while it loads (FR-02). Also whether the session will run on the GPU, as far as is known yet.
fn prepare_model(st: &AppState, settings: &Settings) -> AppResult<(&'static catalog::CatalogEntry, bool)> {
    let entry =
        catalog::find(&settings.model_id).ok_or_else(|| AppError::new(ErrorCode::ModelMissing, "no model selected"))?;
    if !entry.path_in(&st.models_dir).exists() {
        return Err(AppError::new(ErrorCode::ModelMissing, format!("{} is not downloaded", entry.id)));
    }
    let status = st.worker.status();
    if status.model_id.as_deref() != Some(entry.id.as_str())
        || matches!(status.state, EngineState::Missing | EngineState::Failed)
    {
        st.load_selected_model();
    }
    let gpu = if status.state == EngineState::Ready {
        status.gpu
    } else {
        settings.use_gpu && crate::asr::engine::gpu_compiled()
    };
    Ok((entry, gpu))
}

struct NewSession<'a> {
    title: String,
    started: &'a chrono::DateTime<Local>,
    sources: Vec<SourceInfo>,
    entry: &'static catalog::CatalogEntry,
    gpu: bool,
    version: &'a str,
}

/// A new session's folder under the output root, its log begun.
fn create_session(settings: &Settings, new: NewSession) -> AppResult<(PathBuf, SessionLog, Session)> {
    let root = PathBuf::from(&settings.output.root);
    let started_at = new.started.to_rfc3339_opts(SecondsFormat::Secs, false);
    std::fs::create_dir_all(&root)?;
    let folder = paths::unique_folder(&root, &paths::folder_name(new.started, &new.title));
    std::fs::create_dir_all(folder.join("images"))?;
    let session_id = ulid::Ulid::generate().to_string();
    let model = ModelRef { id: new.entry.id.clone(), sha256: new.entry.sha256.clone() };
    let log = SessionLog::create(&folder)?;
    log.append(&LogEvent::SessionStarted {
        id: session_id.clone(),
        title: new.title.clone(),
        started_at: started_at.clone(),
        sources: new.sources.clone(),
        model: model.clone(),
        language: settings.language.as_str().into(),
        gpu: new.gpu,
        app_version: new.version.to_string(),
    })?;
    let session = Session {
        v: SCHEMA_VERSION,
        id: session_id,
        title: new.title,
        started_at,
        ended_at: None,
        duration_ms: 0,
        sources: new.sources,
        model,
        language: settings.language.as_str().into(),
        gpu: new.gpu,
        items: Vec::new(),
        unprocessed_ms: 0,
        project: None,
        continued: Vec::new(),
        audio: Vec::new(),
    };
    Ok((folder, log, session))
}

fn asr_config(settings: &Settings) -> AsrConfig {
    AsrConfig {
        language: settings.language,
        vocabulary: settings.vocabulary.clone(),
        accuracy_first: settings.accuracy_first,
        hallucination_filter: settings.hallucination_filter,
        audio_ctx_experimental: settings.audio_ctx_experimental,
    }
}

fn segmenter_config(settings: &Settings) -> SegmenterConfig {
    SegmenterConfig {
        start_threshold: settings.vad.start_threshold,
        hangover_ms: settings.vad.hangover_ms,
        ..Default::default()
    }
}

/// The right to start, held until the new phase is in place: a second start meanwhile (a
/// double click, a second hotkey press) is turned away instead of starting a second session.
fn claim_start(st: &AppState) -> AppResult<parking_lot::MutexGuard<'_, ()>> {
    let busy = || AppError::new(ErrorCode::Busy, "already recording");
    let guard = st.starting.try_lock().ok_or_else(busy)?;
    if !matches!(&*st.recorder.lock(), Phase::Idle) {
        return Err(busy());
    }
    Ok(guard)
}

/// A new session, or a saved one that a recording continues (FR-08).
enum Begin {
    New { title: Option<String> },
    Continue { session_id: String },
}

/// Starts a recording. Capture runs within a second; the model may still be loading.
pub fn start(app: &AppHandle, st: &AppState, config: SourceConfig, title: Option<String>) -> AppResult<StartResponse> {
    begin(app, st, config, Begin::New { title })
}

/// Records more onto a saved session (FR-08): its transcript, screenshots and sound carry on
/// after its end, and clock times follow the new start.
pub fn continue_session(
    app: &AppHandle,
    st: &AppState,
    config: SourceConfig,
    session_id: String,
) -> AppResult<StartResponse> {
    begin(app, st, config, Begin::Continue { session_id })
}

fn begin(app: &AppHandle, st: &AppState, config: SourceConfig, how: Begin) -> AppResult<StartResponse> {
    let _starting = claim_start(st)?;
    let settings = st.settings.read().clone();
    let (entry, gpu) = prepare_model(st, &settings)?;
    check_source(&config)?;

    let started = Local::now();
    let started_at = started.to_rfc3339_opts(SecondsFormat::Secs, false);
    let app_name = source_name(&config, settings.locale);
    let Planned { plan, infos, app_root_pid } = plan_sources(&config, &app_name);
    let version = app.version().to_string();
    let root = PathBuf::from(&settings.output.root);
    // The session to record into, the session time capture starts at, and whether it is new.
    let (folder, log, session, ids, at_ms, fresh) = match how {
        Begin::New { title } => {
            let title = title
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| paths::expand_title(&settings.output.title_template, &app_name, &started));
            let new = NewSession { title, started: &started, sources: infos.clone(), entry, gpu, version: &version };
            let (folder, log, session) = create_session(&settings, new)?;
            (folder, log, session, SessionIds::default(), 0, true)
        }
        Begin::Continue { session_id } => {
            let folder = recovery::find_folder(&root, &session_id)
                .ok_or_else(|| AppError::internal(format!("session {session_id} not found")))?;
            let (mut session, stopped, _) = recovery::load(&folder)?;
            if !stopped {
                // It ended in a crash: close what it has before carrying on.
                recovery::recover(&folder, &settings.export_options(), &version)?;
                session = recovery::load(&folder)?.0;
            }
            std::fs::create_dir_all(folder.join("images"))?;
            let log = SessionLog::open_append(&folder)?;
            let ids = SessionIds::after_log(&folder);
            // On a fresh step of the sound's readings, just past the end.
            let at_ms = (session.duration_ms / crate::session::levels::STEP_MS + 1) * crate::session::levels::STEP_MS;
            (folder, log, session, ids, at_ms, false)
        }
    };
    let session_id = session.id.clone();
    let session_audio_count = session.audio.len();
    let t0 = (platform::qpc_now_100ns() as i64) - at_ms as i64 * 10_000;
    let store = Arc::new(SessionStore::new(&folder, log, session, Arc::new(ids), events_of(app), settings.echo_guard));
    let ctx = SessionCtx::new(session_id.clone(), asr_config(&settings), store.clone());
    let segmenter = segmenter_config(&settings);
    let levels = if fresh { LevelWriter::create(&folder) } else { LevelWriter::open_append(&folder) };
    let audio_file = audio::file_name(session_audio_count + 1);
    let audio = settings
        .audio
        .record
        .then(|| AudioRecorder::start(folder.join(&audio_file), at_ms * 16))
        .and_then(|r| r.inspect_err(|e| tracing::warn!("{audio_file}: {e:#}")).ok());
    let (status_tx, status_rx) = crossbeam_channel::unbounded();
    let mut active = Active {
        store,
        ctx,
        t0,
        config: config.clone(),
        sources: recorded_sources(&config, &app_name),
        plan,
        running: Vec::new(),
        status_rx,
        status_tx,
        paused_at: None,
        paused_ms: 0,
        lag_banner_shown: false,
        deferred: false,
        defer_offered: false,
        app_root_pid,
        app_name: (config.mode == SourceMode::App).then(|| app_name.clone()),
        shot_window: None,
        segmenter,
        partials: settings.partials && st.partials_tier_ok(&entry.id),
        levels: levels.inspect_err(|e| tracing::warn!("levels.bin: {e}")).ok(),
        audio,
    };
    st.levels.clear();
    if let Err((source, err)) = start_sources(&mut active, st, at_ms) {
        if let Some(sound) = active.audio.take() {
            sound.finish();
            let _ = std::fs::remove_file(folder.join(&audio_file));
        }
        drop(active);
        if fresh {
            let _ = std::fs::remove_dir_all(&folder);
        }
        let msg = format!("{err:#}");
        return Err(
            if msg.contains("privacy")
                || err
                    .downcast_ref::<crate::audio::source::CaptureError>()
                    .is_some_and(|e| matches!(e, crate::audio::source::CaptureError::MicDenied))
            {
                AppError::new(ErrorCode::MicDenied, msg)
            } else {
                AppError::new(ErrorCode::SourceLost, format!("{}: {msg}", source.as_str()))
            },
        );
    }
    if active.audio.is_some() {
        active.store.record(LogEvent::AudioStarted { file: audio_file, start_ms: at_ms });
    }
    if !fresh {
        // Logged once capture runs, so a source that fails to start leaves the session as it was.
        active.store.record(LogEvent::SessionContinued { at_ms, started_at, sources: infos });
        active.store.add_marker(at_ms, MarkerKind::Continued, Some(started.format("%Y-%m-%d").to_string()));
    }
    let response = StartResponse { session_id: session_id.clone(), folder: folder.to_string_lossy().into_owned() };
    *st.recorder.lock() = Phase::Recording(Box::new(active));

    remember_source(app, st, &config);
    emit_state(app, st);
    tracing::info!("recording {}: session {session_id}", if fresh { "started" } else { "continued" });
    Ok(response)
}

fn start_sources(active: &mut Active, st: &AppState, start_ms: u64) -> Result<(), (SourceId, anyhow::Error)> {
    let sink: Arc<dyn JobSink> = st.worker.clone();
    for p in active.plan.clone() {
        let (tx, rx) = crossbeam_channel::bounded(400);
        let params = DspParams {
            source: p.id,
            t0_100ns: active.t0,
            segmenter: active.segmenter.clone(),
            partials: active.partials,
            pad_quiet_stream: p.id == SourceId::System,
            start_ms,
            audio: active.audio.as_ref().map(AudioRecorder::sender),
            track: None,
        };
        let dsp = dsp::spawn(rx, params, active.ctx.clone(), sink.clone(), active.store.ids.clone(), st.levels.clone());
        let stand_in = st.stand_in.lock().clone();
        let mut source: Box<dyn AudioSource> = match stand_in {
            Some(files) => files.source(p.id),
            None => crate::audio::open_source(p.id, p.target.clone(), active.status_tx.clone()),
        };
        if let Err(err) = source.start(tx) {
            let _ = dsp.join();
            stop_sources(active);
            return Err((p.id, err));
        }
        active.running.push(RunningSource { id: p.id, source, dsp });
    }
    Ok(())
}

/// Stops capture and waits for each DSP thread to flush its last utterance.
fn stop_sources(active: &mut Active) {
    for mut r in active.running.drain(..) {
        r.source.stop();
        drop(r.source);
        match r.dsp.join() {
            Ok(summary) => tracing::info!(
                "{}: {} s of audio, {} utterances, {:?}",
                r.id.as_str(),
                summary.samples_16k / 16_000,
                summary.utterances,
                summary.clock
            ),
            Err(_) => tracing::error!("{} DSP thread panicked", r.id.as_str()),
        }
    }
}

/// Stops capture, transcribes what is queued (「仕上げ中…」), then saves the session.
pub fn stop(app: &AppHandle, st: &AppState) -> AppResult<()> {
    let cancel = Arc::new(AtomicBool::new(false));
    // Recording goes straight to Finishing under one lock: never Idle in between, where a start
    // could slip in.
    let mut active = {
        let mut rec = st.recorder.lock();
        let finishing = match &*rec {
            Phase::Recording(a) => Phase::Finishing {
                session_id: a.store.id.clone(),
                folder: a.store.folder.clone(),
                sources: a.sources.clone(),
                cancel: cancel.clone(),
            },
            // A second Stop while the first one finishes.
            Phase::Finishing { .. } => return Err(AppError::new(ErrorCode::Busy, "already stopping")),
            Phase::Idle => return Err(AppError::internal("not recording")),
        };
        match std::mem::replace(&mut *rec, finishing) {
            Phase::Recording(a) => a,
            _ => unreachable!("checked above"),
        }
    };
    let stop_ms = active.paused_at.unwrap_or_else(|| active.now_ms());
    emit_state(app, st);
    let app = app.clone();
    std::thread::Builder::new()
        .name("finishing".into())
        .spawn(move || {
            let st = app.state();
            stop_sources(&mut active);
            finish(&app, st, active, stop_ms, cancel);
        })
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(())
}

fn finish(app: &AppHandle, st: &AppState, mut active: Box<Active>, stop_ms: u64, cancel: Arc<AtomicBool>) {
    // The readings and the sound end with the recording: close their files before the session
    // is listed.
    if let Some(mut track) = active.levels.take()
        && let Err(e) = track.flush()
    {
        tracing::warn!("levels.bin: {e}");
    }
    if let Some(sound) = active.audio.take() {
        sound.finish();
    }
    let store = active.store.clone();
    drop(active);
    finish_session(app, st, &store, stop_ms, &cancel, None);
}

/// How a file's transcription ends (FR-18): its progress counts the whole file, and what was never
/// read (cancelled, or the file broke off) is unprocessed too.
struct FileEnd {
    total_ms: u64,
    unread_ms: u64,
}

/// A file's progress: how far into it the text has got, as far as can be told. Silence counts as
/// done once read; speech once transcribed.
fn file_progress(total_ms: u64, read_ms: u64, pending_ms: u64) -> events::ProgressPayload {
    events::ProgressPayload { done: 0, total: 0, done_ms: read_ms.saturating_sub(pending_ms).min(total_ms), total_ms }
}

/// Finishing waits this many polls (200 ms each) on a model that failed to load before it gives up.
const STALLED_POLLS: u32 = 10;

/// Transcribes what is queued (「仕上げ中…」), then saves the session.
fn finish_session(
    app: &AppHandle,
    st: &AppState,
    store: &Arc<SessionStore>,
    stop_ms: u64,
    cancel: &AtomicBool,
    file: Option<FileEnd>,
) {
    let id = store.id.clone();
    st.worker.drop_partials(&id);
    // What waited for Stop runs now (FR-25).
    st.worker.set_deferred(&id, false);
    let total = st.worker.pending_finals(&id);
    let total_ms = st.worker.pending_ms(&id);
    let mut dropped_ms = None;
    let mut stalled_polls = 0;
    loop {
        let pending = st.worker.pending_finals(&id);
        let progress = match &file {
            Some(f) => file_progress(f.total_ms, f.total_ms - f.unread_ms, st.worker.pending_ms(&id)),
            None => events::ProgressPayload {
                done: total.saturating_sub(pending),
                total,
                done_ms: total_ms.saturating_sub(st.worker.pending_ms(&id)),
                total_ms,
            },
        };
        events::emit(app, events::Event::Finishing(progress));
        if pending == 0 {
            break;
        }
        // A model that failed to load never takes the queue: what waits is unprocessed, as if
        // cancelled. Checked over a while, since a load that was just asked for reads as the
        // last failure until the worker picks it up.
        stalled_polls = if st.worker.stalled() { stalled_polls + 1 } else { 0 };
        let give_up = cancel.load(Ordering::SeqCst) || stalled_polls >= STALLED_POLLS;
        if give_up && dropped_ms.is_none() {
            if !cancel.load(Ordering::SeqCst) {
                tracing::warn!("the model failed to load; {id}'s queued speech stays unprocessed");
            }
            dropped_ms = Some(st.worker.cancel_session(&id));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let unprocessed = dropped_ms.unwrap_or(0) + file.map_or(0, |f| f.unread_ms);
    if unprocessed > 0 {
        store.add_marker(stop_ms, MarkerKind::Unprocessed, Some(unprocessed.div_ceil(1000).to_string()));
    }
    store.record(LogEvent::SessionStopped {
        ended_at: Local::now().to_rfc3339_opts(SecondsFormat::Secs, false),
        duration_ms: stop_ms,
        unprocessed_ms: unprocessed,
        recovered: None,
    });
    if let Err(e) = store.log.sync() {
        tracing::error!("final sync failed: {e}");
    }
    let settings = st.settings.read().clone();
    let session = store.snapshot();
    let version = app.version().to_string();
    let saved = recovery::save_outputs(&store.folder, &session, &settings.export_options(), &version);
    match saved {
        Ok(path) => {
            events::emit(
                app,
                events::Event::SessionSaved(events::SavedPayload {
                    session_id: id.clone(),
                    folder: store.folder.to_string_lossy().into_owned(),
                    transcript_path: path.to_string_lossy().into_owned(),
                }),
            );
            if settings.auto_copy_on_stop {
                let text = crate::export::plaintext::plain(&session, &settings.export_options());
                if app.shell().write_clipboard(&text).is_ok() {
                    events::notice(app, Notice::new(NoticeLevel::Info, "copied", "copied").toast());
                }
            }
        }
        Err(e) => {
            tracing::error!("saving outputs failed: {e}");
            let code = if crate::error::is_disk_full(&e) { ErrorCode::DiskFull } else { ErrorCode::Internal };
            events::notice(app, Notice::new(NoticeLevel::Error, code.as_str(), "saveFailed"));
        }
    }
    *st.recorder.lock() = Phase::Idle;
    // Whisper's working buffers (a few hundred MB of VRAM per source) wait for the next recording.
    st.worker.release_states();
    st.levels.clear();
    emit_state(app, st);
    tracing::info!("session {id} saved");
    if st.quit_after_finish.load(Ordering::SeqCst) {
        app.shell().exit();
    }
}

/// Speech this far ahead of transcription is enough: reading a file waits, so a long one never
/// sits in memory as queued speech (FR-18).
const FILE_READ_AHEAD_MS: u64 = 120_000;

/// Why a file can't be transcribed, in words the UI looks up (`noAudio`, `unsupported`).
fn file_error(e: anyhow::Error) -> AppError {
    let message = match e.downcast_ref::<DecodeError>() {
        Some(DecodeError::NoAudio) => "noAudio".to_string(),
        Some(DecodeError::Unsupported) => "unsupported".to_string(),
        None => format!("{e:#}"),
    };
    AppError::new(ErrorCode::FileUnreadable, message)
}

/// Transcribes an audio or video file into a new session (FR-18). Its sound goes through the
/// recording's pipeline, read as fast as Whisper takes it, while the view finishes (「仕上げ中…」)
/// with how far into the file the text has got. Session time is time in the file.
pub fn transcribe_file(app: &AppHandle, st: &AppState, path: PathBuf) -> AppResult<StartResponse> {
    let _starting = claim_start(st)?;
    let settings = st.settings.read().clone();
    let (entry, gpu) = prepare_model(st, &settings)?;
    let lossy = |s: Option<&std::ffi::OsStr>| s.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = lossy(path.file_name());
    let started = Local::now();
    let version = app.version().to_string();
    let sources = vec![SourceInfo {
        id: SourceId::File,
        label: OTHERS.into(),
        exe: None,
        device: None,
        name: Some(name.clone()),
    }];
    let new = NewSession { title: lossy(path.file_stem()), started: &started, sources, entry, gpu, version: &version };
    let (folder, log, session) = create_session(&settings, new)?;
    let id = session.id.clone();
    let store = Arc::new(SessionStore::new(
        &folder,
        log,
        session,
        Arc::new(SessionIds::default()),
        events_of(app),
        settings.echo_guard,
    ));
    let ctx = SessionCtx::new(id.clone(), asr_config(&settings), store.clone());
    let track = LevelWriter::create(&folder)
        .inspect_err(|e| tracing::warn!("levels.bin: {e}"))
        .ok()
        .map(|w| Arc::new(parking_lot::Mutex::new(w)));
    let audio_file = audio::file_name(1);
    let sound = settings
        .audio
        .record
        .then(|| AudioRecorder::start(folder.join(&audio_file), 0))
        .and_then(|r| r.inspect_err(|e| tracing::warn!("{audio_file}: {e:#}")).ok());
    let params = DspParams {
        source: SourceId::File,
        // The file's own clock: session time zero is its start.
        t0_100ns: 0,
        segmenter: segmenter_config(&settings),
        partials: false,
        pad_quiet_stream: false,
        start_ms: 0,
        audio: sound.as_ref().map(AudioRecorder::sender),
        track: track.clone(),
    };
    st.levels.clear();
    let (tx, rx) = crossbeam_channel::bounded(400);
    let dsp = dsp::spawn(rx, params, ctx.clone(), st.worker.clone(), store.ids.clone(), st.levels.clone());
    let (worker, sid) = (st.worker.clone(), id.clone());
    let behind = move || worker.pending_ms(&sid) >= FILE_READ_AHEAD_MS;
    let mut source = FileSource::new(SourceId::File, path, None, 0, 1.0).with_pace(Pace::AheadOf(Arc::new(behind)));
    if let Err(e) = source.start(tx) {
        let _ = dsp.join();
        if let Some(sound) = sound {
            sound.finish();
        }
        // Nothing may hold a file in the folder open while it goes.
        drop((track, ctx, store));
        let _ = std::fs::remove_dir_all(&folder);
        return Err(file_error(e));
    }
    if sound.is_some() {
        store.record(LogEvent::AudioStarted { file: audio_file, start_ms: 0 });
    }
    let progress = source.progress();
    let cancel = Arc::new(AtomicBool::new(false));
    *st.recorder.lock() = Phase::Finishing {
        session_id: id.clone(),
        folder: folder.clone(),
        sources: RecordedSources { ids: vec![SourceId::File], app_name: Some(name) },
        cancel: cancel.clone(),
    };
    emit_state(app, st);
    tracing::info!("transcribing a file: session {id}");
    let response = StartResponse { session_id: id.clone(), folder: folder.to_string_lossy().into_owned() };
    let app = app.clone();
    std::thread::Builder::new()
        .name("file".into())
        .spawn(move || {
            let st = app.state();
            let total = || progress.duration_ms.load(Ordering::SeqCst).max(progress.read_ms.load(Ordering::SeqCst));
            // Reading waits on Whisper, so a model that failed to load would hold it forever.
            let mut stalled_polls = 0;
            while !progress.done.load(Ordering::SeqCst)
                && !cancel.load(Ordering::SeqCst)
                && stalled_polls < STALLED_POLLS
            {
                let read = progress.read_ms.load(Ordering::SeqCst);
                events::emit(&app, events::Event::Finishing(file_progress(total(), read, st.worker.pending_ms(&id))));
                std::thread::sleep(Duration::from_millis(200));
                stalled_polls = if st.worker.stalled() { stalled_polls + 1 } else { 0 };
            }
            let gave_up = stalled_polls >= STALLED_POLLS && !progress.done.load(Ordering::SeqCst);
            source.stop();
            drop(source);
            match dsp.join() {
                Ok(summary) => tracing::info!(
                    "file: {} s of audio, {} utterances",
                    summary.samples_16k / 16_000,
                    summary.utterances
                ),
                Err(_) => tracing::error!("file DSP thread panicked"),
            }
            if let Some(track) = &track
                && let Err(e) = track.lock().flush()
            {
                tracing::warn!("levels.bin: {e}");
            }
            drop(track);
            if let Some(sound) = sound {
                sound.finish();
            }
            let broke_off = progress.error.lock().take();
            if let Some(detail) = &broke_off {
                events::notice(
                    &app,
                    Notice::new(NoticeLevel::Warn, ErrorCode::FileUnreadable.as_str(), "fileBrokeOff")
                        .param("detail", detail.clone()),
                );
            }
            let (read_ms, total_ms) = (progress.read_ms.load(Ordering::SeqCst), total());
            // A file read to its end has nothing unread, though its stated length may differ a little.
            let stopped_early = broke_off.is_some() || cancel.load(Ordering::SeqCst) || gave_up;
            let unread_ms = if stopped_early { total_ms - read_ms } else { 0 };
            finish_session(&app, st, &store, read_ms, &cancel, Some(FileEnd { total_ms, unread_ms }));
        })
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(response)
}

pub fn cancel_finishing(st: &AppState) {
    if let Phase::Finishing { cancel, .. } = &*st.recorder.lock() {
        cancel.store(true, Ordering::SeqCst);
    }
}

/// Pause (P1): stop capture and keep the session; the timeline gets a marker.
pub fn pause(app: &AppHandle, st: &AppState) -> AppResult<()> {
    let mut rec = st.recorder.lock();
    let Phase::Recording(active) = &mut *rec else { return Err(AppError::internal("not recording")) };
    if active.paused_at.is_some() {
        return Ok(());
    }
    let now = active.now_ms();
    stop_sources(active);
    active.store.add_marker(now, MarkerKind::Paused, None);
    active.paused_at = Some(now);
    drop(rec);
    st.levels.clear();
    emit_state(app, st);
    Ok(())
}

pub fn resume(app: &AppHandle, st: &AppState) -> AppResult<()> {
    let mut rec = st.recorder.lock();
    let Phase::Recording(active) = &mut *rec else { return Err(AppError::internal("not recording")) };
    if active.paused_at.is_none() {
        return Ok(());
    }
    let now = active.now_ms();
    if let Err((source, err)) = start_sources(active, st, now) {
        return Err(AppError::new(ErrorCode::SourceLost, format!("{}: {err:#}", source.as_str())));
    }
    active.store.add_marker(now, MarkerKind::Resumed, None);
    if let Some(at) = active.paused_at.take() {
        active.paused_ms += now.saturating_sub(at);
    }
    drop(rec);
    emit_state(app, st);
    Ok(())
}

/// Switches what is recorded without stopping (FR-17): capture restarts on the new source at
/// this moment, the session gains the new source's name, and the timeline gets a marker. While
/// paused, the new source starts on resume. If it cannot start, the old one carries on.
pub fn switch_source(app: &AppHandle, st: &AppState, config: SourceConfig) -> AppResult<()> {
    check_source(&config)?;
    let locale = st.settings.read().locale;
    let mut rec = st.recorder.lock();
    let Phase::Recording(active) = &mut *rec else { return Err(AppError::internal("not recording")) };
    if active.config == config {
        return Ok(());
    }
    let now = active.now_ms();
    let paused = active.paused_at.is_some();
    let app_name = source_name(&config, locale);
    let Planned { plan, infos, app_root_pid } = plan_sources(&config, &app_name);
    if !paused {
        stop_sources(active);
    }
    let old_plan = std::mem::replace(&mut active.plan, plan);
    if !paused && let Err((source, err)) = start_sources(active, st, now) {
        active.plan = old_plan;
        if let Err((old, e)) = start_sources(active, st, now) {
            tracing::error!("restarting {} after a failed switch: {e:#}", old.as_str());
        }
        drop(rec);
        emit_state(app, st);
        return Err(AppError::new(ErrorCode::SourceLost, format!("{}: {err:#}", source.as_str())));
    }
    {
        let mut session = active.store.session.lock();
        for info in infos {
            if !session.sources.contains(&info) {
                session.sources.push(info);
            }
        }
    }
    active.store.add_marker(now, MarkerKind::SourceChanged, Some(source_words(&config, &app_name)));
    active.sources = recorded_sources(&config, &app_name);
    active.app_root_pid = app_root_pid;
    active.app_name = (config.mode == SourceMode::App).then(|| app_name.clone());
    active.config = config.clone();
    drop(rec);
    st.levels.clear();
    remember_source(app, st, &config);
    emit_state(app, st);
    Ok(())
}

/// 「システム全体に切り替える」 after the app-silence health check (section 7).
pub fn switch_to_system(app: &AppHandle, st: &AppState) -> AppResult<()> {
    let config = match &*st.recorder.lock() {
        Phase::Recording(a) if a.config.mode == SourceMode::App => {
            SourceConfig { mode: SourceMode::System, app: None, ..a.config.clone() }
        }
        _ => return Ok(()),
    };
    switch_source(app, st, config)
}

/// Picks the window screenshots take for the rest of the recording, or (`None`) goes back to the
/// target in the settings (FR-34).
pub fn set_shot_window(app: &AppHandle, st: &AppState, window: Option<screenshot::WindowInfo>) -> AppResult<()> {
    {
        let mut rec = st.recorder.lock();
        let Phase::Recording(active) = &mut *rec else { return Err(AppError::internal("not recording")) };
        active.shot_window = window;
    }
    emit_state(app, st);
    Ok(())
}

/// Leaves transcription for after Stop, or (`false`) takes it up again now (FR-25). The speech
/// waits in the queue, so nothing is lost either way.
pub fn set_deferred(app: &AppHandle, st: &AppState, deferred: bool) -> AppResult<()> {
    {
        let mut rec = st.recorder.lock();
        let Phase::Recording(active) = &mut *rec else { return Err(AppError::internal("not recording")) };
        active.deferred = deferred;
        st.worker.set_deferred(&active.store.id, deferred);
    }
    emit_state(app, st);
    Ok(())
}

/// Cut (FR-06): a new part of the session starts now. Works while paused too, so a break can
/// end one part.
pub fn add_cut(st: &AppState) -> AppResult<()> {
    let rec = st.recorder.lock();
    let Phase::Recording(active) = &*rec else { return Err(AppError::internal("not recording")) };
    active.store.add_marker(active.now_ms(), MarkerKind::Cut, None);
    Ok(())
}

/// Marks the line being said now as important (FR-07): the utterance in progress, else the one
/// that ended last. Its text may still be on its way; the mark waits for it. `false` when nothing
/// has been said yet.
pub fn mark_current(st: &AppState) -> AppResult<bool> {
    let rec = st.recorder.lock();
    let Phase::Recording(active) = &*rec else { return Err(AppError::internal("not recording")) };
    let t_ms = active.paused_at.unwrap_or_else(|| active.now_ms());
    let utterances = crate::asr::worker::current_utterances(&active.ctx.speaking.lock(), t_ms);
    if utterances.is_empty() {
        return Ok(false);
    }
    active.store.mark_utterances(&utterances, t_ms);
    Ok(true)
}

/// Screenshot (FR-30, FR-31). The time is read before any capture work.
pub fn take_screenshot(app: &AppHandle, st: &AppState) -> AppResult<ShotResponse> {
    let (store, t_ms, root_pid, window) = {
        let rec = st.recorder.lock();
        let Phase::Recording(active) = &*rec else {
            events::notice(app, Notice::new(NoticeLevel::Info, "not_recording", "shotOnlyWhileRecording").toast());
            return Err(AppError::internal("not recording"));
        };
        (active.store.clone(), active.now_ms(), active.app_root_pid, active.shot_window.as_ref().map(|w| w.id))
    };
    {
        let mut last = st.last_shot.lock();
        if last.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
            return Err(AppError::internal("ignored repeat"));
        }
        *last = Some(Instant::now());
    }
    let (setting, sound) = {
        let s = st.settings.read();
        (s.screenshot.target, s.screenshot.sound)
    };
    let target = match (window, setting, root_pid) {
        (Some(id), _, _) => screenshot::CaptureTarget::Window { id },
        (None, ScreenshotTarget::AllMonitors, _) => screenshot::CaptureTarget::AllMonitors,
        (None, ScreenshotTarget::AppWindow, Some(pid)) => screenshot::CaptureTarget::AppWindow { root_pid: pid },
        _ => screenshot::CaptureTarget::CursorMonitor,
    };
    let result = st.capturer.capture(target).map_err(|e| AppError::internal(format!("capture failed: {e:#}")))?;
    if sound {
        // After the capture, so the sound confirms a picture was actually taken.
        static SHUTTER: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        crate::platform::play_wav(SHUTTER.get_or_init(screenshot::shutter_wav));
    }
    let (counter, id) = store.ids.next_image();
    let local = crate::export::wall_time(&store.snapshot(), t_ms);
    let hms = local.format("%H%M%S").to_string();
    let multi = result.images.len() > 1;
    let mut first: Option<ShotResponse> = None;
    for (i, captured) in result.images.iter().enumerate() {
        let id = if i == 0 { id.clone() } else { store.ids.next_image().1 };
        let rel = screenshot::file_name(counter, &hms, multi.then_some(i + 1));
        let path = store.folder.join(&rel);
        let image = &captured.image;
        let thumb = std::thread::scope(|s| {
            let thumb = s.spawn(|| screenshot::thumbnail_data_url(image));
            let saved = screenshot::save_png(image, &path);
            (thumb.join().ok().and_then(|t| t.ok()), saved)
        });
        let (thumb, saved) = thumb;
        if let Err(e) = saved {
            let disk = e.downcast_ref::<std::io::Error>().is_some_and(crate::error::is_disk_full);
            return Err(AppError::new(if disk { ErrorCode::DiskFull } else { ErrorCode::Internal }, format!("{e:#}")));
        }
        let ok = store.record(LogEvent::Screenshot {
            id: id.clone(),
            t_ms,
            file: rel.clone(),
            width: image.width(),
            height: image.height(),
            target: captured.label.clone(),
        });
        if ok {
            events::emit(
                app,
                events::Event::Screenshot(events::ScreenshotPayload {
                    id: id.clone(),
                    t_ms,
                    thumb_data_url: thumb.unwrap_or_default(),
                    width: image.width(),
                    height: image.height(),
                    file: rel,
                }),
            );
        }
        first.get_or_insert(ShotResponse { id, t_ms });
    }
    let time = local.format("%H:%M:%S").to_string();
    if app.shell().window_hidden() {
        app.shell().notify(&crate::host::os_text(st.settings.read().locale, "shotAdded", &time));
    }
    let mut n = Notice::new(NoticeLevel::Info, "shot_added", "shotAdded").param("time", time).toast();
    if result.fell_back {
        n = n.param("fallback", true);
    }
    events::notice(app, n);
    first.ok_or_else(|| AppError::internal("nothing captured"))
}

/// Called every 100 ms: levels at 10 Hz; state, lag and health at 1 Hz.
pub fn tick(app: &AppHandle, st: &AppState, n: u64) {
    let mut statuses = Vec::new();
    let mut stop_for_disk = false;
    let mut lag_banner = false;
    let mut defer_banner = false;
    {
        let mut rec = st.recorder.lock();
        let Phase::Recording(active) = &mut *rec else { return };
        if active.paused_at.is_none() {
            let levels = st.levels.snapshot();
            let others =
                levels.iter().filter(|(id, _)| **id != SourceId::Mic).map(|(_, db)| *db).fold(FLOOR_DBFS, f32::max);
            let me = levels.get(&SourceId::Mic).copied().unwrap_or(FLOOR_DBFS);
            let now = active.now_ms();
            let failed = active.levels.as_mut().and_then(|track| track.push(now, others, me).err());
            if let Some(e) = failed {
                tracing::warn!("levels.bin: {e}");
                active.levels = None;
            }
            let levels = levels.into_iter().map(|(k, v)| (k, (v * 10.0).round() / 10.0)).collect();
            events::emit(app, events::Event::Levels(levels));
        }
        while let Ok(s) = active.status_rx.try_recv() {
            statuses.push((s, active.now_ms(), active.app_name.clone()));
        }
        if active.store.disk_full.load(Ordering::SeqCst) {
            stop_for_disk = true;
        }
        if n.is_multiple_of(10) {
            let now = active.now_ms();
            let lag = st.worker.lag_ms(&active.store.id, now);
            let status = st.worker.status();
            let device = match status.state {
                EngineState::Ready if status.gpu => "gpu",
                EngineState::Ready => "cpu",
                _ => "loading",
            };
            events::emit(
                app,
                events::Event::Lag(events::LagPayload { lag_ms: lag, queued: st.worker.queued(), device }),
            );
            // Lag means nothing while transcription waits for Stop.
            if !active.deferred {
                if lag >= DEFER_OFFER_LAG_MS && !active.defer_offered {
                    active.defer_offered = true;
                    defer_banner = true;
                }
                if lag > 120_000 && !active.lag_banner_shown {
                    active.lag_banner_shown = true;
                    lag_banner = true;
                }
            }
            if let Err(e) = active.store.log.sync_if_due() {
                tracing::warn!("log sync failed: {e}");
            }
            if let Some(track) = active.levels.as_mut()
                && let Err(e) = track.flush()
            {
                tracing::warn!("levels.bin: {e}");
            }
        }
    }
    if n.is_multiple_of(10) {
        let payload = state_payload(st);
        events::emit(app, events::Event::State(payload));
    }
    if defer_banner {
        events::notice(
            app,
            Notice::new(NoticeLevel::Warn, "lag_defer", "lagDefer").action("deferTranscription", "defer_transcription"),
        );
    }
    if lag_banner {
        events::notice(
            app,
            Notice::new(NoticeLevel::Warn, "lag", "lagSwitchModel").action("openModels", "open_models"),
        );
    }
    for (status, now, app_name) in statuses {
        handle_status(app, st, status, now, app_name);
    }
    if stop_for_disk {
        events::notice(app, Notice::new(NoticeLevel::Error, ErrorCode::DiskFull.as_str(), "diskFull"));
        let _ = stop(app, st);
    }
}

fn handle_status(app: &AppHandle, st: &AppState, status: SourceStatus, now: u64, app_name: Option<String>) {
    let name = app_name.unwrap_or_else(|| "アプリ".into());
    match status {
        SourceStatus::Started { .. } => {}
        SourceStatus::Lost { source } => {
            if let Phase::Recording(a) = &mut *st.recorder.lock()
                && source == SourceId::App
            {
                a.app_root_pid = None;
            }
            events::notice(
                app,
                Notice::new(NoticeLevel::Warn, ErrorCode::SourceLost.as_str(), "srcWaiting").param("app", name),
            );
        }
        SourceStatus::Reattached { source, detail } => {
            let pid = detail.strip_prefix("pid ").and_then(|p| p.parse::<u32>().ok());
            if let Phase::Recording(a) = &mut *st.recorder.lock() {
                if source == SourceId::App {
                    a.app_root_pid = pid;
                }
                a.store.add_marker(now, MarkerKind::SourceReattached, None);
            }
            events::notice(
                app,
                Notice::new(NoticeLevel::Info, "source_reattached", "srcReattached").param("app", name).toast(),
            );
        }
        SourceStatus::DeviceSwitched { device, .. } => {
            events::notice(
                app,
                Notice::new(NoticeLevel::Info, "device_switched", "deviceSwitched")
                    .param("device", device.unwrap_or_default())
                    .toast(),
            );
        }
        SourceStatus::MicDenied => {
            events::notice(
                app,
                Notice::new(NoticeLevel::Error, ErrorCode::MicDenied.as_str(), "micDenied")
                    .action("openSettings", "open_mic_privacy"),
            );
        }
        SourceStatus::AppSilent => {
            events::notice(
                app,
                Notice::new(NoticeLevel::Warn, ErrorCode::SourceSilent.as_str(), "appSilent")
                    .action("switchToSystem", "switch_to_system"),
            );
        }
        SourceStatus::Failed { message, .. } => {
            events::notice(
                app,
                Notice::new(NoticeLevel::Error, ErrorCode::SourceLost.as_str(), "sourceFailed")
                    .param("detail", message),
            );
        }
    }
}

/// Spawns the 100 ms ticker for the app's lifetime.
pub fn spawn_ticker(app: AppHandle) {
    std::thread::Builder::new()
        .name("ticker".into())
        .spawn(move || {
            let mut n = 0u64;
            loop {
                std::thread::sleep(Duration::from_millis(100));
                n += 1;
                let st = app.state();
                tick(&app, st, n);
            }
        })
        .expect("spawn ticker");
}

/// Start with the last-used source (the Start/Stop hotkey while idle).
pub fn start_last_used(app: &AppHandle, st: &AppState) -> AppResult<StartResponse> {
    let s = st.settings.read().clone();
    let mut mode = s.source.mode;
    let mut app_ref = None;
    if mode == SourceMode::App {
        match &s.source.app {
            Some(last) => {
                let pid = crate::audio::find_app_root(std::path::Path::new(&last.exe));
                app_ref = Some(AppRef { exe: last.exe.clone(), root_pid: pid, name: Some(last.name.clone()) });
            }
            None => mode = SourceMode::System,
        }
        if !platform::app_loopback_supported() {
            mode = SourceMode::System;
            app_ref = None;
        }
    }
    let config = SourceConfig {
        mode,
        app: app_ref,
        include_mic: s.source.include_mic,
        mic_device_id: s.source.mic_device_id.clone(),
    };
    start(app, st, config, None)
}

/// Start/Stop toggle shared by the button, hotkey and tray.
pub fn toggle(app: &AppHandle, st: &AppState) {
    let phase = st.recorder.lock().ui_state();
    let result = match phase {
        None => start_last_used(app, st).map(|_| ()),
        Some(UiState::Recording) | Some(UiState::Paused) => stop(app, st),
        _ => Ok(()),
    };
    if let Err(e) = result {
        // A second press while the first one starts or stops: nothing to say.
        if e.code == ErrorCode::Busy {
            return;
        }
        tracing::warn!("toggle failed: {e}");
        let mut n = Notice::new(NoticeLevel::Error, e.code.as_str(), "startFailed").param("detail", e.message.clone());
        if e.code == ErrorCode::MicDenied {
            n = n.action("openSettings", "open_mic_privacy");
        }
        if e.code == ErrorCode::ModelMissing {
            n = n.action("openModels", "open_models");
        }
        events::notice(app, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(mode: SourceMode, include_mic: bool) -> SourceConfig {
        let app = (mode == SourceMode::App).then(|| AppRef {
            // A path this OS splits, so the name comes from its file stem.
            exe: if cfg!(windows) {
                r"C:\Program Files\Zoom\bin\Zoom.exe"
            } else {
                "/Applications/Zoom.app/Contents/MacOS/Zoom"
            }
            .into(),
            root_pid: 0,
            name: None,
        });
        SourceConfig { mode, app, include_mic, mic_device_id: None }
    }

    #[test]
    fn default_titles_follow_the_ui_language() {
        assert_eq!(source_name(&config(SourceMode::System, true), Locale::Ja), "システム全体");
        assert_eq!(source_name(&config(SourceMode::System, true), Locale::En), "All system audio");
        assert_eq!(source_name(&config(SourceMode::Mic, false), Locale::En), "Mic");
        // An app keeps its own name, here its executable's.
        assert_eq!(source_name(&config(SourceMode::App, true), Locale::En), "Zoom");
    }

    #[test]
    fn a_switch_names_the_new_source_in_the_export_language() {
        assert_eq!(source_words(&config(SourceMode::App, true), "Teams"), "Teams + マイク");
        assert_eq!(source_words(&config(SourceMode::System, false), "x"), "システム全体");
        assert_eq!(source_words(&config(SourceMode::Mic, true), "x"), "マイク");
    }

    #[test]
    fn recorded_sources_list_the_mic_last() {
        let s = recorded_sources(&config(SourceMode::App, true), "Zoom");
        assert_eq!(s.ids, vec![SourceId::App, SourceId::Mic]);
        assert_eq!(s.app_name.as_deref(), Some("Zoom"));
        let s = recorded_sources(&config(SourceMode::Mic, true), "マイク");
        assert_eq!((s.ids, s.app_name), (vec![SourceId::Mic], None));
    }
}
