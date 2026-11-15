# Chidori — Live Transcription App Spec (v1)

Oct 2, 2026 · @Yusai Tajiri · trimmed to the shipped app on 2026-10-05

## 1. How to use this spec

This spec says what Chidori does and why. The code holds the details: exact parameters, event
types, settings keys, the model catalog. Where the two disagree, fix whichever is wrong.
`docs/DECISIONS.md` records every choice the spec left open and every deviation from it, with
the reason; `AGENTS.md` has the build and contribution rules.

Section numbers are cited from code comments ("section 8", "FR-53"), so they stay fixed.

**Glossary**

| Term | Meaning |
| --- | --- |
| Session | One recording, from Start to Stop, saved as one folder |
| Source | An audio input: `mic`, `system` (everything playing) or `app` (one process tree); `file` for an audio or video file (FR-18) |
| Process loopback | Windows capture of one process tree's audio output only |
| Utterance | A speech span cut out by the VAD, sent to Whisper as one job |
| Segment | One finalized line of transcript text with start/end times and a source |
| Partial | Grey provisional text for the utterance still in progress |
| Timeline | The ordered segments, screenshots and markers that every export is built from |

## 2. Product overview

Chidori turns a meeting or lecture on a Windows PC into a Markdown or PDF transcript with
screenshots in place. No audio leaves the machine.

It came from a chat between the author and two friends. One used a free local transcriber, pasted
its typo-ridden text into Claude to fix, and wanted one tap to start, stop, copy and drop a
screenshot into the transcript. The other wanted to record only Zoom's audio. The existing tool
could not pick an app.

**Goals:** one tap each to start, stop and copy; real-time Japanese transcription fully on the PC;
pick the source (mic, whole system or one app), optionally adding the mic as 自分 against 相手;
one key drops a screenshot at that moment; export Markdown, PDF and Typst; a small installer that
downloads its model on first run; typo cleanup by copying the transcript with a ready-made
prompt for any AI chat or agent (no API key).

**Non-goals:** diarization beyond 自分/相手, translation, summaries, accounts or sync, video (a
video file's sound is transcribed, FR-18, but its pictures are not kept),
audio editing, a caption overlay, platforms other than Windows 10/11 x64 (macOS is phase 2,
section 19).

**Users:** Japanese-speaking students and office workers in Zoom, Teams or Meet calls and
lectures, on anything from integrated-GPU laptops to gaming PCs. Speech is mostly Japanese with
English terms. Distribution is to the author and friends: an unsigned installer from GitHub
Releases.

**Success:** launch to recording in two clicks at most; a 60-minute call with screenshots exports
with no manual fixes; on a usable GPU text appears within about 3 s of a sentence ending; a
friend installs and runs it alone, apart from the SmartScreen warning.

## 3. Core flows

**Flow A: a Zoom call with screenshots (the main flow).** Open Chidori. Pick Zoom as the source;
「会話として録音」 (record as a conversation) adds the mic. Press 開始. The live line shows each
side's sound and lines appear a few seconds after each sentence, grey while still being spoken.
Press the screenshot key while Zoom keeps focus; a toast confirms and the picture lands in the
transcript at that moment. Press 停止: queued audio is transcribed (「仕上げ中…」) and the session
saves itself. Then コピー, or 書き出し for Markdown, PDF or Typst.

**Flow B: first run.** Run the installer (More info → Run anyway at SmartScreen). The wizard
recommends a model for the PC, downloads it with a progress bar (resumable), runs a 10-second mic
test that shows the first words, and opens ready to record.

**Flow C: typo cleanup.** After stopping, 「Agent用にコピー」 puts a cleanup prompt and the
transcript on the clipboard for pasting into an AI chat or agent.

**Flow E: a recording made elsewhere.** On the start screen, 「ファイルから文字起こし」 (or drop a
lecture's MP4 on the window). The view goes straight to 「文字起こし中 · 12:34 / 58:00」 with a bar,
lines arriving as they are transcribed, and opens as a finished session when it is done.

**Flow D: crash recovery.** On the next launch the app finds a session with no end marker, asks
「前回の記録が途中で終了しています。復元しますか？」, and 復元 rebuilds the transcript from the event
log and screenshots on disk.

## 4. Functional requirements

All of these shipped in v1. IDs are cited in code; gaps in the numbering are dropped or deferred
items (section 19).

| ID | Area | Requirement |
| --- | --- | --- |
| FR-01 | Recording | One button toggles Start/Stop; a global hotkey does too once the user sets one (none by default). |
| FR-02 | Recording | Capture starts within 1 s of Start. If the model is still loading, audio is buffered, not lost. |
| FR-03 | Recording | While recording: a recording mark, elapsed time (without paused time), each source's level drawn as the live line, and the tray in its recording state. |
| FR-04 | Recording | On Stop, transcribe all queued audio with progress, then save the session. |
| FR-05 | Recording | Pause and resume, with markers in the timeline. |
| FR-06 | Recording | A cut button starts a new part of the session (a new topic or scene), recorded as a `cut` marker; exports head each part with its time. |
| FR-07 | Recording | A star button (and hotkey) marks the line being said as important, even before its text arrives; any line can be marked or unmarked from the transcript while recording or after. Exports start an important line with ★. |
| FR-08 | Recording | 「続きを録音」 (a finished session's ⋯ menu, History's row menu) records more onto a saved session, today's or an old one: its transcript, screenshots and sound carry on after its end, and clock times after that point follow the new start. |
| FR-09 | Recording | Keep the recording's sound (on by default, 設定 › 音声): every source mixed into one Opus file per recording. A finished session plays it back from any line (its ▶), or from the start with the deck's play button; the line playing is highlighted and the session's line shows where the sound is. |
| FR-10 | Sources | Sources: the whole system, the mic alone, or one app. |
| FR-11 | Sources | The app list puts apps with an audio session first, each showing its live sound, then other apps with windows; it has a refresh button. |
| FR-12 | Sources | 「会話として録音」 (on by default) adds the mic to an app or system recording, labelling lines 自分 (mic) and 相手 (the rest). |
| FR-13 | Sources | Mic choice; default is the Windows default communications device. |
| FR-14 | Sources | If the chosen app exits and starts again, reattach within 3 s and mark it. App capture can also start before the app is running. |
| FR-15 | Sources | When the default output or mic device changes mid-recording, reopen the stream on the new one. |
| FR-16 | Sources | Echo guard: drop 自分 lines that are the speakers picked up by the mic (section 8). |
| FR-17 | Sources | Switch the source while recording, without stopping: capture restarts on the new source, the session lists it, and the timeline gets a `source_changed` marker. If it cannot start, the old source carries on. |
| FR-18 | Sources | Transcribe an audio or video file into a new session: 「ファイルから文字起こし」 under 開始 (in the compact window, in the source panel), or a file dropped on the window. It runs as fast as Whisper keeps up, shown as finishing with how far into the file the text has got; 処理を中止 keeps what is done. The session is named after the file and its times are places in the file (`00:12:34`), in the app and in every export. |
| FR-20 | Transcription | Local Whisper through whisper.cpp. Language 日本語 (default), English or 自動. |
| FR-21 | Transcription | Final lines appear within the latency targets of section 16. |
| FR-22 | Transcription | Grey provisional text for the utterance in progress (GPU only). |
| FR-23 | Transcription | Filter hallucinated and repeated phrases (section 8). |
| FR-24 | Transcription | GPU through Vulkan, with automatic CPU fallback. |
| FR-25 | Transcription | When transcription falls behind, show the lag (「遅れ 12秒」), and a minute behind offer to transcribe after Stop. Never drop audio. |
| FR-26 | Transcription | A custom word list (up to 50 terms) is passed to Whisper as its prompt. |
| FR-30 | Screenshots | A button, and a global hotkey once set, captures the recorded app's window (else the screen under the cursor). Alternatives: the cursor's screen, all screens. |
| FR-31 | Screenshots | The image is placed in the timeline at capture time, shown in the live view and confirmed by a toast. |
| FR-32 | Screenshots | Chidori's own window never appears in screenshots or screen shares. |
| FR-33 | Screenshots | Delete a screenshot or caption it from the transcript. |
| FR-34 | Screenshots | The camera's menu picks what screenshots take: the recorded app's window, the cursor's screen or all screens (remembered as the setting), or any open window, for the rest of the recording. |
| FR-40 | Transcript | Live list; auto-scroll pauses when the user scrolls up and 最新へ appears; a scrollbar at the right edge scrolls to any point (dragging it to the end resumes auto-scroll). |
| FR-41 | Transcript | Edit or delete a line after Stop; every export uses the edits. |
| FR-50 | Copy | コピー copies plain text; settings choose times and labels. Its chevron (コピー ▾) copies for an agent or as Markdown. |
| FR-51 | Copy | 「Agent用にコピー」 copies the cleanup prompt followed by the transcript. |
| FR-52 | Sessions | On Stop, `transcript.md` and `images/` are saved in the session folder, which opens with one click. |
| FR-53 | Export | PDF with Japanese fonts and full-width screenshots; also Typst source and Markdown (folder or ZIP). |
| FR-60 | Sessions | Each session is one folder under the output root (default `Documents\Chidori`). |
| FR-61 | Sessions | A crash-safe event log; the next launch offers recovery (Flow D). |
| FR-62 | Sessions | History, grouped by day: open, re-export, delete one or several (to the Recycle Bin). A session offers the same ⋯ menu open and as a row in History, 削除 included. |
| FR-63 | Sessions | Rename a session; the default title is the source app's name. |
| FR-64 | Sessions | Projects: named, coloured groups of sessions. A session joins one from its header (also while recording), History's row menu or a multi-selection; History filters by project, which turns it into the project's page, whose ⋯ renames, recolours and deletes it (deleting keeps the recordings). |
| FR-70 | Models | The first-run wizard recommends a model and downloads it with resume and checksum check. |
| FR-71 | Models | Model manager: download, import from a file, delete, switch; size and speed tier shown. The recommended models and the ones installed are listed; the rest open from ほかのモデル. |
| FR-72 | Models | A 10-second benchmark rates how well the PC runs a model. |
| FR-90 | System | Tray menu: 開始/停止, スクショ, ウィンドウを表示, 終了. |
| FR-91 | System | Single instance: a second launch focuses the running window. |
| FR-92 | System | Configurable hotkeys (Start/Stop, screenshot, important mark, cut); one that fails to register is reported in Settings. |
| FR-93 | System | Closing the window hides it to the tray while recording or finishing, and quits otherwise. Quitting while recording asks first. |
| FR-94 | System | Compact and expanded layouts (expanded by default), an always-on-top pin (off by default), remembered window position. |
| FR-95 | System | Update check at most once a day; installing asks first. |

## 5. UI and UX

One small window: expanded (420 × 640, resizable) for the transcript, History and Settings, and
what it opens in until you shrink it; compact (380 × 170) for sitting beside a call. The look is
Chidori's own palette on the Jello brand (the `jello-brand` repository): greyscale chrome, colour
only for who is speaking (相手 coral, 自分 teal) and that it is recording (one red dot), plain
rounded rects on an 8 px grid with no shadows or glows, and one spring ("jello") for everything
that moves. `docs/DECISIONS.md` → UI explains how the app applies it and is the reference for UI
changes.

- **Places.** Expanded: 録音 · 履歴 · 設定 as tabs in the title bar, the active one on a white
  pill. The recording place has three phases: the start screen (相手 and 自分 as two white
  panels, then a round 開始 with its hotkey and ファイルから文字起こし), the recording (an ink
  card with ON AIR, the title, the source, the timer and the live wave; the transcript as white
  bubbles; a bottom bar with the camera, star, cut, pause and 停止) and the finished session (the
  card turns white with the date, length, source, project, a play button and the session's wave;
  the bar holds コピー, 書き出し and ⋯). The card's title renames on a click; its source opens a
  menu that switches the source (FR-17). The compact window has three rows: the title and
  source, the latest line, and the dot, timer and controls.
- **Rare actions** sit in popup menus, drawn by the app (native where the window is too small to
  hold one): a session's ⋯ (続きを録音, フォルダを開く, 削除, the same in History's rows) and コピー ▾
  (Agent用, Markdown). A right click on a row, a line, a screenshot, the card, the title bar or a
  text box opens the menu of what is under it.
- **Errors** are a banner at the top with one action, e.g. 「マイクへのアクセスがオフです [設定を開く]」
  opening `ms-settings:privacy-microphone`. An error is an ink banner with a coral icon, a
  warning a white one with a grey icon, always with words, so colour alone never carries one.
- **Screenshot confirmation** is a toast; with the window hidden, a silent Windows notification.
- **Light and dark** follow Windows. Every icon button names itself in a tooltip. Screen readers
  and keyboard navigation are not supported yet (DECISIONS → UI).
- **Text** is Japanese first, English second, all through `crates/app/src/i18n.rs`
  (`crates/app/i18n/`). The first launch
  is in Japanese on a Japanese Windows and in English on any other.

**States** (`recording://state`): `needs_model` → `ready` → `recording` ⇄ `paused` →
`finishing` → `ready`. Start works while the model loads (FR-02). A lost app reattaches without
leaving `recording`; a source that fails shows an error banner while the others carry on; a
full disk stops the recording and saves what exists.

**Tray.** The app icon in ink when idle, paused or finishing, and in its 朱 while recording (the icon is unchanged from the kamon kit until Jello has an icon tool).
Left-click shows or hides the window.

**Hotkeys** are off until set in 設定 › ショートカット. When idle, the start hotkey starts with the
last-used source; the screenshot, important-mark and cut hotkeys show 「録音中のみ使えます」.

**Setup wizard:** welcome (everything stays on this PC) → model → download and benchmark →
10-second mic test → shortcuts (optional) → done.

## 6. System architecture

All audio, transcription and file work is in `crates/core`; the GPUI UI in `crates/app` renders
state and calls the core.
Each source has its own capture and DSP threads, and exactly one worker owns the Whisper model.

| Thread or task | Owns | Sends to |
| --- | --- | --- |
| Main (GPUI event loop) | The window, tray, hotkeys, popup menus | Calls into the core (blocking ones on a worker) |
| Capture, one per source | WASAPI client at MMCSS "Pro Audio" priority | Raw chunks (bounded channel, about 2 s) |
| DSP, one per source | Clock placement, downmix, resampling, VAD, segmenter, level | Jobs to the ASR queue; levels to the UI and `levels.bin` |
| ASR worker (one) | The Whisper context and one state per source | Post-processing, then the session log and UI |
| Blocking tasks | Screenshots, exports | Disk, clipboard, UI |

One ASR worker is deliberate: two would fight over the GPU, and Whisper's cost per call is
nearly fixed (section 8), so merging jobs beats running them in parallel.

**OS boundary.** Windows code lives behind `AudioSource`, `AudioAppLister` and `ScreenCapturer`
(plus `Vad` for the detector and `AudioDecoder` for files), so the macOS port adds files rather than
changing the core.

**Stack.** All Rust. GPUI (through `gpui-kit`, with `gpui-component`'s text boxes and tooltips)
for the UI, drawn with DirectX 11, with `tray-icon` (and its `muda` popup menus), `global-hotkey`, `rfd` file dialogs and
`cpal` for playback; `whisper-rs` (whisper.cpp, quantized GGML models, Vulkan on
NVIDIA/AMD/Intel); `wasapi` for all three sources (QPC timestamps, session listing, device
events); `rubato` to 16 kHz; `earshot` for VAD (pure Rust, nothing like ONNX Runtime to ship);
`xcap` for screenshots; `typst` for PDF. The UI calls the core through `ops.rs` and hears from it
through typed `events::Event`s. Cargo feature `gpu-vulkan` is the shipped build; without it the
build is CPU-only.

## 7. Audio capture (Windows)

All three sources use the `wasapi` crate in event-driven shared mode with 20 ms buffers. Per-app
capture needs Windows build 20348 or later (in practice Windows 11); on Windows 10 the app
choices are disabled with 「この機能には Windows 11 が必要です」 and system and mic still work.

| Source | Setup | Notes |
| --- | --- | --- |
| `mic` | Capture client on the chosen endpoint | Needs Windows' mic privacy permission |
| `system` | Loopback on the default render endpoint | Everything playing, notifications included |
| `app` | Application loopback on the root PID, including its process tree | Follows the app across output devices |

**Process loopback quirks.** Never call `get_mixformat`, `is_supported` or `get_device_period`;
they fail in this mode. Request 48 kHz stereo float, falling back to 16-bit PCM. Device position
is always 0, so time comes from the QPC timestamp.

**Time.** Every buffer is placed by its QPC timestamp, never by counting samples. Gaps over 20 ms
become silence, so all streams stay aligned with wall-clock time and each other; overlaps are
dropped. System loopback sends nothing in silence, so the DSP thread pads silence itself after
500 ms without packets, letting an open utterance close.

**App picker.** Lists audio sessions on every active render device, skipping system sounds,
Chidori itself and `audiodg.exe`. Each PID is walked up its parents while the executable name
matches; that root covers every tab of a browser, whose audio comes from a child process.
Sessions playing now come first, then the rest, then (under その他) windowed apps with no
session yet. Each playing app shows its live sound from the peak meter Windows keeps for every
session; nothing is captured for this.

**Limits to tell the user.** A browser is captured whole, never one tab. The user's own voice is
not in an app's output, which is why the mic is added by default.

**Reattach (FR-14).** The selected app's executable is remembered. When its process goes, the
capture thread waits for a new root process of the same executable and marks 「音声ソース再接続」
in the timeline.

**Device changes (FR-15).** Default-device changes reopen `mic` and `system` streams on the new
default, debounced so a Bluetooth headset switching profiles doesn't thrash them. `app` streams
need nothing.

**Health checks.** An `app` stream that delivers only zeros for 30 s while its session meter
shows sound offers 「システム全体に切り替えますか？」 (process loopback has been reported silent on a
Windows 11 build). A stream with no packets for 5 s is restarted once. Access denied on the mic
shows the mic privacy banner.

## 8. Transcription pipeline

Real time comes from cutting speech at natural pauses and transcribing each piece, not from a
streaming model. whisper.cpp's encoder always processes a 30-second window, so a 2 s clip costs
nearly as much as a 25 s one. The pipeline is built around that.

**Per source:** downmix to mono, resample to 16 kHz, score 16 ms frames with earshot, and let the
segmenter cut utterances: it opens on a few frames of speech, closes after about 640 ms of
silence (the hangover, adjustable 400–1200 ms), keeps 320 ms of pre-roll so first syllables
survive, drops anything under 400 ms, and cuts long speech at the quietest point after 15 s and
always by 25 s, inside Whisper's window.

**Files (FR-18).** Media Foundation's source reader decodes the file's first audio track to float
samples (any container and codec Windows plays; a video's pictures are never decoded); Ogg Opus,
which Windows can't read, goes through libopus. The file thread places each block by its position
in the file, so session time is file time, and the source runs the same DSP as a recording, without
partials. Reading waits while 2 minutes of speech are queued, so a long file never sits in memory;
the queue is always behind, so jobs merge (below) and the file runs many times faster than real
time on a GPU. Its line is drawn from `levels.bin`, written by the DSP at file time.

**ASR worker.** Final jobs go before partials, oldest first across sources; only the newest
partial per source is kept. When it falls behind, waiting jobs from one source merge into one of
up to 25 s with 200 ms of silence between pieces, and each returned segment is mapped back to
its original time. Whisper states are freed between recordings (they hold hundreds of MB of
VRAM).

**Decoding.** Greedy by default; beam search when 精度優先 (accuracy first) is on. A fixed
language is more stable on short clips than detection. `no_context` is on and context is passed
explicitly in the prompt instead; Whisper's own VAD is off because ours already cut the audio.

**Prompt.** The word list joined with 「、」 and ended with 「。」, then the source's last accepted
line (up to 60 characters), or the primer 「えー、それでは始めます。よろしくお願いします。」 when
there is none and the language is Japanese. A punctuated prompt makes Whisper punctuate. Under
120 characters in all. A rejected line is never carried over, and utterances under 1 s get the
word list only (Whisper tended to repeat the previous line).

**Language.** 自動 locks onto the language of the first utterance of 3 s or more. Whisper is
never allowed to translate: after each decode the worker checks the language Whisper hears, and
when it is at least 0.9 sure a clip is in the other of Japanese and English, decodes it again in
that one (details in DECISIONS).

**Post-processing, in order:**

1. Remove spaces between Japanese characters; keep them next to Latin words.
2. Drop the segment if no-speech probability is 0.8 or more, or it is only symbols (♪, (音楽)).
3. Hallucinations: drop text that matches or starts with a phrase in `hallucinations_ja.txt`
   (after NFKC, without punctuation) when the utterance is also short (< 4 s), quiet
   (< −40 dBFS) or doubtful (no-speech > 0.3). Never add ご清聴ありがとうございました; people
   really say it.
4. Repetition: collapse a 1–10 character unit repeated 4+ times to 3; if that removes over 60%
   of the text, drop it.
5. Echo guard.
6. Write to the session log and emit to the UI.

**Echo guard (FR-16).** Only with the mic and an app/system stream both on. A 自分 line is compared
with 相手 lines within ±1.5 s (character-bigram Dice ≥ 0.6, and also against the overlapping
相手 lines joined, since the mic's VAD cuts differently). A match is never written, or removed
with `segment_removed` if it was. On by default; the UI suggests headphones.

**Partials (FR-22).** Every 1.5 s of an open utterance, if the GPU is in use, no final job is
waiting and the model's benchmark tier is 快適: greedy, no prompt, single segment. The final line
with the same utterance ID replaces it.

**Lag** = now minus the end of the oldest waiting utterance.

| Lag | Behaviour |
| --- | --- |
| Under 5 s | Normal |
| 5 s or more | No partials; waiting jobs merge |
| 15 s or more | 「遅れ n秒」 is bold |
| 30 s or more | 「遅れ n秒」 turns yellow |
| 60 s or more | Banner, once: 「停止後にまとめる」 |
| Over 120 s | Banner: 「軽量モデルなら追いつきます」 |

「停止後にまとめる」 stops running Whisper for the rest of the recording: speech is still cut into
utterances and queued per source (相手 and 自分 stay apart), but nothing runs until Stop, so the PC
keeps up with capture alone. The line reads 「停止後に文字起こし · 今すぐ再開」, which takes it up
again. The lag banners stop while it waits.

Audio is never dropped. On Stop the queue drains with progress, a bar by queued audio once it has
taken 5 s; 処理を中止 ends the transcript with 「（以降、未処理の音声 N 秒）」.

**GPU.** Before anything touches Vulkan, implicit layers are disabled
(`VK_LOADER_LAYERS_DISABLE`; overlays such as OBS or Steam crash it), unless
`CHIDORI_KEEP_VK_LAYERS=1`. The model loads in the background at launch with a warm-up on
silence. If GPU init fails, or crashed the process last time, the app uses the CPU, says
「GPUを使えないためCPUで実行します」, and remembers it for that model and GPU until 「GPUを再試行」.

## 9. Screenshots and timeline placement

A screenshot is stamped with the session time when the key fires and placed at that time.
Placement is computed every time, never stored, because sentences arrive seconds after the
screenshot.

**Capture.** Repeats within 500 ms are ignored. The time is read first, before any capture work.
The target is the recorded app's largest visible window (else the screen under the cursor), the
screen under the cursor, or every screen (one image each, same time, `-m1`, `-m2`), as set; or a
window picked from the camera's menu for this recording (FR-34), until it closes (then the
screen under the cursor). A
full-resolution PNG goes to `images/0001_151603.png` (counter, local `HHMMSS`), a 320 px JPEG
thumbnail goes to the UI, and the `screenshot` event is logged once the file is written.

**Keeping Chidori out.** The window is content-protected (`WDA_EXCLUDEFROMCAPTURE`): invisible,
not black, in screenshots and in the user's screen share. On by default.

**Known limits.** DRM video shows black. Windows does not deliver global hotkeys to a normal app
while an elevated app has focus.

**Placement rule.** Strictly chronological, so the times shown never go backwards. A segment
sorts at its start, a screenshot or marker at its own time. On a tie: resume, reattach and cut
markers, then segments (相手 before 自分), then screenshots, then pause and unprocessed markers,
then IDs. `order_timeline` in Rust (exports) and `orderTimeline` in TypeScript (live view) are
the same pure function; a late segment re-sorts, it is never appended. Partials sort at their
utterance start.

Example: 相手 speaks 15:16:01–15:16:06, a screenshot is taken at 15:16:03, 自分 starts at
15:16:05. The order is the 相手 line, the screenshot, the 自分 line.

## 10. Data model and files on disk

While recording, an append-only event log is the source of truth. Everything else is regenerated
from it, so a crash loses at most the last line.

| What | Where |
| --- | --- |
| Sessions | `Documents\Chidori\` (configurable) |
| Projects | `projects.json` beside the session folders (FR-64) |
| Settings, app state | `%APPDATA%\dev.yusai.chidori\` (`settings.json`, `state.json`) |
| Models | `%LOCALAPPDATA%\dev.yusai.chidori\models\` (large, not roaming) |
| Logs | `%LOCALAPPDATA%\dev.yusai.chidori\logs\` |

**Session folder** `YYYY-MM-DD_HHMM_<title>/` (title sanitized for Windows, at most 40
characters, `-2` and so on if taken):

- `session.jsonl`: the event log, one JSON object per line with schema version `v`. Events:
  `session_started`, `segment`, `segment_removed`, `segment_edited`, `segment_marked`, `screenshot`,
  `screenshot_deleted`, `caption_set`, `marker`, `title_changed`, `project_set`, `language_detected`,
  `audio_started`, `session_continued`, `session_stopped`. Each is appended and flushed, synced every 5 s and on Stop; a truncated
  last line is ignored on replay.
- `session.json`: a snapshot written on Stop and after edits, plus hashes of files Chidori
  wrote.
- `transcript.md`: written on Stop and after edits.
- `levels.bin`: each side's level ten times a second, drawn as the session's line.
- `audio/0001.ogg` (`0002` for a continuation, FR-08): the recording's sound (FR-09), all sources
  mixed, Opus at 24 kb/s in Ogg (about 11 MB an hour). The file's time is session time from
  where the log's `audio_started` says it starts; pauses are silence.
- `images/`: screenshots.

Session IDs are ULIDs; segment, image and marker IDs count up within a session. Shown times are
`startedAt + tMs` in local `HH:MM:SS`. A continued session (FR-08) records on from just after its
end, so session time stays one line, and lists each continuation as `{ atMs, startedAt }`: from
`atMs` on, shown times count from that `startedAt`. The log's `session_continued` opens the
session again until the next `session_stopped`.

**Recovery (FR-61).** At launch, a `session.jsonl` without `session_stopped` is offered for
recovery (Flow D): replay the log, write the snapshot and `transcript.md`, append
`session_stopped` with `"recovered": true`.

**Deleting** moves the folder to the Recycle Bin, never a permanent delete.

## 11. Export and copy

Every output is built from the session timeline, not from `transcript.md`. Text is UTF-8 without
BOM, LF line endings, and in Japanese whatever the UI language.

**Markdown** (`transcript.md`, and the Markdown export):

```markdown
---
title: Zoom
date: 2026-10-02 15:13
duration: 01:02:15
sources: [相手 (Zoom), 自分 (マイク)]
model: turbo-q5
app: Chidori 0.1.0
---

# Zoom — 2026-10-02 15:13

**[15:15:58] 相手:** それでは始めます。次のスライドお願いします。

![15:16:03 のスクリーンショット](images/0001_151603.png)

**[15:16:05] 自分:** よろしくお願いします。

*— 15:40:12 音声ソース再接続 —*
```

- A session in a project has `project: <name>` in its front matter, after the title.
- Consecutive lines from one source merge into a paragraph when under 2 s apart, up to 400
  characters, shown at the first line's time.
- Japanese joins without a space; a space goes only between Latin letters or digits (and after
  Latin sentence punctuation).
- Settings: timestamps on/off; labels on, off or auto (only when there are two sources).
- A cut (FR-06) is a `## HH:MM:SS` heading, so a reader or an agent sees the session's parts;
  other markers are italic lines as above.
- An important line (FR-07) starts with `★ ` and is never merged into a paragraph with other
  lines. Plain text does the same, the PDF sets it in bold after a star, and the cleanup prompt
  tells the agent what ★ means and to keep it.
- Image links are relative. Alt text is the caption, else 「HH:MM:SS のスクリーンショット」; a
  caption is also an italic line under the image.
- If the user edited `transcript.md` (its hash differs from what Chidori wrote), Chidori writes
  `transcript (2).md` instead of overwriting it.

**Clipboard**

| Action | Content |
| --- | --- |
| コピー | Plain text, one paragraph per line (`[15:15:58] 相手: …`), screenshots as `[画像 15:16:03]` unless hidden |
| Agent用にコピー | The cleanup prompt (`export/prompts.rs`), then the text with screenshots as `[画像:0001 15:16:03]`, which the prompt says to keep untouched |
| Markdownでコピー | The Markdown body without front matter |

**Exports (書き出し).** Each asks where to save, starting in Downloads with the session folder's
name; nothing is written into the session folder.

- **PDF (FR-53):** the session becomes a Typst document (`export/transcript.typ` preamble, then
  one call per block with all session text in string literals so it is never read as markup),
  compiled in process with the `typst` crate. A4, 18 mm margins, Yu Gothic (Meiryo, then MS
  Gothic), the brand's ink with 相手 in 朱, 子持ち罫 under the title, screenshots full width as
  JPEG ≤ 1600 px, and the app's name in the footer. It opens when saved. If Typst fails, the print HTML is saved instead and opened
  in the browser with 「ブラウザの印刷からPDFに保存してください」.
- **Typst:** the same document as a `.typ` file, with its screenshots in `<name>_images/`, so
  `typst compile` gives the same PDF after the user edits it.
- **Markdown:** a new folder holding `transcript.md` and `images/`, or the same two in one ZIP.

## 12. Model management

The default is Whisper large-v3-turbo at q5_0 (`turbo-q5`, 574 MB): it handles Japanese mixed
with English and runs in real time on most GPUs. Models are never bundled; the wizard downloads
one.

**Catalog** (`models/catalog.json`): `turbo-q5`, `kotoba-v2-q5` (Japanese-only audio),
`turbo-q8`, `large-v3-q5` (GPU only), `small-q5` (weak PCs), `base-q5` (tests only, hidden).
Each entry pins a Hugging Face repo, file, commit revision, size and SHA-256, all taken from the
LFS metadata for that revision, never typed from memory.

**Download.** From `https://huggingface.co/{repo}/resolve/{revision}/{file}`, after checking free
space. Written to `{file}.part` and resumed with `Range` (a 200 answer starts over); cancelling
keeps the `.part`. Verified against SHA-256 before the rename. ファイルから追加 imports a file
downloaded elsewhere, for networks that block Hugging Face, with the same check.

**Recommendation.** `turbo-q5`, or `small-q5` under 8 GB of RAM. The 10-second benchmark
(`resources/bench_ja.wav`, CC0) rates a model:

| Time for the 10 s clip | Tier | Effect |
| --- | --- | --- |
| Under 1.5 s | 快適 | Partials on |
| 1.5–4 s | 普通 | Partials off |
| Over 4 s | 重い | Suggests `small-q5` |

The status row shows `GPU: <name>` from the Vulkan device list, or `CPU`.

## 13. Claude cleanup (dropped)

Dropped on 2026-10-04: 「Agent用にコピー」 (section 11) covers typo cleanup with no API key, and no
transcript text leaves the PC.

## 14. Core and UI

The core is the source of truth. What the UI asks of it is a plain function in
`crates/core/src/ops.rs`, run off the UI thread; what the core tells the UI is an `events::Event`
(state, levels, segments, partials, screenshots, markers, lag, finishing progress, model status
and downloads, saved sessions, notices), sent through the app's `bridge`. Every error is an
`AppError` with an `E_*` code from `error.rs`. Notices carry an i18n key and params, so the core
never formats UI text. What only the shell can do (the clipboard, notifications, quitting) the
core asks through `host::Shell`.

## 15. Settings

Settings are in `settings.json` in the config folder; `crates/core/src/settings.rs` defines
every key and default. Loading never fails: unknown keys are dropped, wrong types and missing keys
get defaults, ranges are clamped. Internal state that is not a user choice (setup done, GPU
failure, benchmark results) lives in `state.json`.

## 16. Non-functional requirements

**Performance** (measured with the harness in section 18; results in DECISIONS)

| Metric | Target |
| --- | --- |
| End of utterance to final text, GPU, `turbo-q5` | p50 ≤ 2 s, p95 ≤ 4 s |
| Lag on CPU with the recommended model | Under 30 s through a 30-minute session |
| Start to capture running | ≤ 1 s |
| Screenshot key to toast | ≤ 500 ms at 1440p; PNG saved within 1.5 s at 4K |
| CPU while idle | Under 1% |
| Memory while recording | Model size + 400 MB at most; flat over 2 hours |
| Launch to window | ≤ 2 s; the model keeps loading in the background |
| Installer | ≤ 15 MB |
| Accuracy | Streaming CER at most 3 points worse than one pass over the same file with the same model |

**Privacy.** Audio never leaves the PC. It is saved only as each recording's sound file in its
own session folder (FR-09, can be turned off); other buffers are freed once transcribed. The
only network use is model downloads (Hugging Face) and the daily update check (GitHub Releases).
No telemetry. Logs hold IDs, lengths and timings, never transcript text.

**Reliability.** The event log makes every session crash-safe; a panic hook logs and flushes the
active session. A panicking ASR job resets the Whisper states and is retried once. A full disk
stops capture cleanly with `E_DISK_FULL`. Sessions of 3 hours or more are supported.

**Security.** The app loads no web content. Models are checked against pinned hashes, updates
against the release key's signature. `open_path` only opens paths inside the output
root, models or logs.

## 17. Packaging, distribution and CI

An unsigned NSIS installer on GitHub Releases, installed per user without admin rights, with an
update check of its own.

- **Installer** (`cargo xtask dist`, cargo-packager's NSIS template): `currentUser`, Japanese and
  English. Its product name, identifier and publisher are those of the Tauri build's installer,
  so it installs over a copy from before the Iced rewrite: the same folder, uninstall entry and
  shortcuts. Before installing it removes Kikitori (the app's earlier name) if present.
- **Runs on a bare Windows.** The C runtime is linked statically. The Vulkan loader
  (`vulkan-1.dll`) ships next to the exe, so a PC with no GPU driver still starts and runs on the
  CPU. Release builds target AVX2 (AVX-512 builds crash on most Intel consumer CPUs); without
  AVX2 and FMA the app says so instead of crashing.
- **Licences.** `licenses\` holds Typst's, the Vulkan runtime's and `THIRD-PARTY-NOTICES.txt`,
  written at build time by `cargo xtask dist` (the crates in the exe, whisper.cpp, libopus, the
  fonts and icons).
- **Signing.** None; friends click More info → Run anyway. Revisit if people outside the friend
  group use it.
- **Updater** (`crates/core/src/update.rs`). A release's `latest.json` names the installer and its
  minisign signature, made with the maintainer's key (the `TAURI_SIGNING_PRIVATE_KEY` secret;
  the format and key are the Tauri updater's, so copies from before the rewrite update too).
  Checked at most once a day after launch. Installing asks first, is refused while recording,
  checks the signature, and runs the installer in passive mode (`/P /UPDATE /R`), which
  relaunches the app.
- **Versioning.** SemVer; the workspace `Cargo.toml` is the source, `CHANGELOG.md` alongside.

- **macOS.** `cargo xtask dist` on a Mac makes `Chidori_<version>_aarch64.dmg`: the Metal build
  as `Chidori.app` (Apple Silicon, macOS 14.6+), signed ad hoc, not notarized. Its update banner
  opens the release page instead of installing (see DECISIONS → macOS).

**CI** (`.github/workflows/`): `ci.yml` runs rustfmt, the Vulkan clippy build and the tests on
every push and pull request that changes more than docs, and the Metal clippy build and the
tests on macOS; `release.yml` builds the Windows installer (signed, after checking the signature
against the app's key) and the Mac `.dmg`, and drafts one release with both from a `v*` tag;
`nightly.yml` runs the ignored integration tests on CPU with `base-q5`.

## 18. Testing and acceptance

Most logic is pure and unit-tested without a model or sound card: segmenter, clock, resampling,
post-processing, echo guard, job merging, timeline order, exports
(`insta` snapshots), the event log and recovery, downloads against a mock server, levels and
folder names. The UI has unit tests for its models and screens' logic, a check that `ja.json` and
`en.json` have the same keys; screens are checked by running the app.

**Integration harness** (`crates/core/tests/pipeline.rs`, `#[ignore]`d). `FileSource` plays WAV
fixtures through the real pipeline: a "meeting" of Japanese speech with pauses, and a
two-channel file (left = mic, right = app) for labels and echo. Fixtures are built from CC0 and
public-domain sources by `tests/fixtures/build_fixtures.py`. The metric is character error rate
after NFKC and removing punctuation and spaces. Pass: streaming CER at most 3 points worse than
one pass with the same model, and latency within section 16 on the reference GPU machine.

**Manual matrix** (before a release)

| Area | Cases |
| --- | --- |
| Windows | 11; 10 22H2 with app mode disabled |
| Apps | Zoom, Teams, Meet in Chrome, YouTube in Edge, Discord |
| Audio | Headphones vs speakers (echo guard); Bluetooth headset switched mid-session; USB mic unplugged |
| GPU | NVIDIA, Intel integrated, AMD; a VM with no GPU driver (CPU fallback) |
| Displays | 100%, 125%, 150% scaling; two monitors with the cursor on the second |
| Long run | 2 hours: memory flat, lag stable on GPU |
| Failure | Kill the process mid-session (recovery); full disk; first run offline |

## 19. Future work

**macOS (phase 2, in progress).** The section 6 traits exist so the port only adds files. Done:
the mic and the whole system's sound through `cpal`'s Core Audio host (a process tap, 14.6+),
Metal, screenshots through `xcap`, the window, tray and hotkeys, and the `.dmg`
(DECISIONS → macOS). Left: one app's sound, transcribing files other than WAV and Ogg Opus (the other formats go
through Media Foundation), signing and notarization.

| Piece | Approach | To verify |
| --- | --- | --- |
| App and system audio | ScreenCaptureKit (13+) with a filter for the chosen app; Core Audio process taps (14.2+) as the alternative | That audio is filtered by app, not just video |
| Mic | `cpal`, or ScreenCaptureKit's mic (15+) | Alignment with the app stream |
| Screenshots | ScreenCaptureKit or `xcap` | Excluding Chidori's window |
| GPU | `whisper-rs` feature `metal` | `turbo-q5` speed on M-series |
| PDF | The same Typst export with a macOS font list (Hiragino Sans) | — |
| Distribution | Signing and notarization, or right-click → Open for friends | — |

**Later:** sleep and wake during a recording (restart sources, add a marker; not handled yet),
region screenshots (FR-34), optional audio saving for re-transcription with a larger model
(FR-54), diarization beyond 自分/相手, summaries,
other engines behind an engine trait, a caption overlay, export to Notion or Google Docs, Linux as a
release (it builds today only as a test platform: `docs/DECISIONS.md` → Linux).

## 20. References and prior art

| Project | What it shows |
| --- | --- |
| [Handy](https://github.com/cjpais/Handy) | Tauri + Rust offline speech-to-text: VAD before transcription, Zustand, i18next, the Vulkan implicit-layer fix |
| [Meetily](https://github.com/Zackriya-Solutions/meetily) | A Tauri meeting transcriber mixing mic and system audio, Whisper on Vulkan |
| [ApplicationLoopback sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/) | Per-process loopback in C++; the build 20348 requirement |
| [MeetCap #35](https://github.com/lybym/MeetCap/pull/35), [#38](https://github.com/lybym/MeetCap/issues/38) | Placing process-loopback audio by QPC; silent process loopback on a Windows 11 build |
