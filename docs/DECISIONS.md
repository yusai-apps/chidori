# Decisions

Choices made where the spec (`AppSpec.md`) is silent, and the few places where implementation
found a reason to deviate.

## Build and toolchain

- **Iced 0.14 instead of Tauri and React** (from 0.6). The WebView2 that drew the Tauri UI was
  most of the app's memory: 99 of 153 MB at idle with the model loaded, and the line cost 18% of
  a core while recording (0.5.1, measured 2026-10-28 with the window in view). A spike of the
  recording window in Iced used 12 MB drawing on the CPU (tiny-skia) and about 119 MB plus 89 MB
  of GPU memory through wgpu (Vulkan; DX12 69 MB).
- **`--show-in-captures`** makes the window show in screenshots and screen sharing for that run,
  whatever 設定's 「画面共有・スクショに映さない」 says (on by default), and saves nothing. It is
  for capturing the app itself, in testing and docs.
- **A driven run** (`--drive <dir>`, only in builds with the `drive` feature; `docs/DRIVE.md`) is
  how the app is tried without a hand on it: commands from a file become GPUI's own input events,
  the window draws itself to a PNG, and recordings hear sound files. It needs GPUI's
  `test-support` (for `render_to_image` and `simulate_next_frame`), which is why it is a feature
  and not a flag of every build. A hidden window gets no frames from the system, so the driven
  run draws one every 16 ms itself, or the jello would never move.
- **GPUI instead of Iced** (after 0.6). GPUI (Zed's UI framework, through `gpui-kit`, which pins
  a GPUI snapshot and adds `gpui-component`'s text boxes and tooltips) replaces Iced.
  The app keeps Iced's shape: one `App`, `Message`s handled in `App::update`, and `Task`s it
  hands back (`task.rs`); `ui.rs` turns GPUI's events into those messages, queued and handled
  after GPUI's event, so an update never runs inside another. Text boxes are GPUI entities, kept
  between frames by key. On Windows GPUI draws with DirectX 11, so the wgpu backend measurements
  below are history. Where GPUI differs:
  - The window's minimum size and resizability are fixed when it opens, so the compact layout's
    fixed size is kept by putting the window back to it whenever it is resized.
  - The borderless title bar is a native drag area (`HTCAPTION`), so dragging and Aero Snap work
    as on any window.
  - Keys reach the screens through `observe_keystrokes`, after GPUI's own handling (a text box
    has had its key by then).
  - Dropdowns in 設定 are drawn by the app (`widgets::select`), as the source lists already were.
  - 設定's two sliders became − and + steppers, saved at each step, until the redesign.
  - The PNG preview tests went: GPUI has no headless renderer. Their fixtures stayed where tests
    use them.
- **The UI drew on the GPU through DirectX 12** (under Iced) (wgpu, `WGPU_BACKEND=dx12` set at startup
  unless already set). Measured in the finished app (release, turbo-q5, window in view):

  | | Idle RAM | Recording RAM | Recording CPU |
  | --- | --- | --- | --- |
  | 0.5.1, WebView2 | 153 MB | 180 MB (silence) | 18% (silence) |
  | wgpu, Vulkan | 160 MB | 200 MB | 11.7% |
  | wgpu, DX12 | 114 MB | 147 MB | 9.8% |
  | tiny-skia (CPU) | 63 MB | 98 MB | 11.4% |

  Recording rows are with speech being transcribed, except 0.5.1's. Vulkan's driver costs about
  45 MB more than DX12 for the same frames. tiny-skia is lighter still and no slower here; GPU
  drawing was the maintainer's choice. tiny-skia stays compiled in as the fallback where wgpu
  can't start (no GPU driver), and `ICED_BACKEND=tiny-skia` picks it by hand.
- **A Cargo workspace**: `crates/core` (no UI, no windowing), `crates/app` (the GPUI UI and the
  shell: tray, hotkeys, popup menus, dialogs, updater) and `xtask` (the installer build). The core
  keeps the module layout it had under Tauri; Tauri's `AppHandle` became `host::AppHandle`, its
  JSON events typed `events::Event`s, and its commands `ops.rs` functions.
- **Exact pins.** Drift-prone crates (`gpui-kit`, `tray-icon`, `global-hotkey`, `rfd`, `cpal`,
  `whisper-rs`, `wasapi`, `xcap`, `earshot`, `rubato`, `ulid`, `typst*`, `zip`) are pinned with
  `=`; everything else by `Cargo.lock`, which is committed.
- **whisper.cpp optimization on MSVC.** The `cmake` crate replaces CMake's per-config flags with
  `cc`'s, which carry no optimization level on MSVC, so whisper.cpp built unoptimized: model load
  went from 9 s to 84 s and CPU RTF from 0.39 to 3.5. `.cargo/config.toml` sets
  `CMAKE_{C,CXX}_FLAGS_RELEASE` back to `-O2 -Ob2 -DNDEBUG`, and the same for
  `RELWITHDEBINFO`, the config dev builds use (they were unoptimized too).
- **Static C runtime.** `.cargo/config.toml` adds `-C target-feature=+crt-static` for the MSVC
  target and passes `-MT` to whisper.cpp to match (the flags above replace the `cmake` crate's,
  so it can't). The release exe imports no `vcruntime140.dll` or `msvcp140.dll`; it imports
  only the Universal CRT (`api-ms-win-crt-*`), which is part of Windows 10 and 11.
- **Vulkan loader bundled.** whisper.cpp's Vulkan backend links `vulkan-1.dll`, which only GPU
  drivers install, so the exe would not start on a PC without one. `cargo xtask dist` downloads
  LunarG's `VulkanRT-X64-1.4.363.0-Components.zip` (pinned SHA-256 for the zip and the DLL) into
  the git-ignored `crates/app/resources/vulkan/`, and the installer puts `vulkan-1.dll` next to
  the exe with its licence in `licenses\`. With no driver the loader
  reports no devices and the app runs on CPU.
- **Installer size: about 22 MB**, over the spec's 15 MB target (section 16). Most of the
  difference is the Typst compiler linked in for PDF export: before it, the installer was
  12.6 MB.
- **CI.** Actions are pinned to exact tags and Rust to 1.98.1 (the version the code was built
  with). The short target directory is computed at run time (`$GITHUB_WORKSPACE\..\t`) instead
  of assuming a drive letter. `ci.yml` only checks, and skips pushes that change nothing but
  Markdown or `docs/`; installers come from `release.yml`, which builds one per `v*` tag.
- **rustfmt** uses `max_width = 120` with `use_small_heuristics = "Max"`, the style the code was
  written in; `cargo fmt --check` runs in CI.
- **Licence: MIT**, the spec's default for a public repository.
- **Third-party notices are generated at build time.** MIT, BSD and Apache-2.0 ask for their
  notices to travel with the binary, so `cargo xtask dist` writes
  `licenses\THIRD-PARTY-NOTICES.txt` (about 1 MB): each distinct licence text once, with the
  packages that ship it. It covers the crates linked into the exe (from `cargo metadata`; build
  scripts and proc macros only run while compiling), whisper.cpp (MIT, inside whisper-rs-sys),
  libopus, and the fonts (OFL) and icons (ISC) the UI embeds. Packages that ship no licence
  file are listed with their declared licence and source. Generating it rather than committing
  it means it can't go stale. Typst's
  bundled fonts are not compiled in (`typst-assets` without `fonts`); hayagriva's CSL styles
  (CC BY-SA 3.0) are, and its notice in the file covers them.
- **CPU baseline: AVX2.** Local builds use `GGML_NATIVE` (AVX-512 on a Zen 5 CPU). Release
  builds set `GGML_NATIVE=OFF`, which selects `/arch:AVX2`, because a binary built with
  `/arch:AVX512` crashes on most consumer Intel CPUs. At startup the app checks for AVX2 and FMA
  and shows 「このPCのCPUは文字起こしに対応していません」 instead of crashing.
- **Vulkan builds need a short target directory.** ggml-vulkan builds `vulkan-shaders-gen` as a
  nested CMake project; under a normal checkout its MSBuild FileTracker paths pass 260 characters
  (error FTK1011). Builds with `gpu-vulkan` use `CARGO_TARGET_DIR=%USERPROFILE%\.kt`; CI uses a
  short path too. The Vulkan SDK is needed only to build.
- **The installer is cargo-packager's NSIS**, run by `cargo xtask dist` with a config it writes
  (absolute paths, so it runs from anywhere). Its template comes from Tauri's, so it takes the
  same `/P` (passive), `/R` (relaunch) and `/UPDATE` switches and writes the same uninstall entry
  and install folder for the same product name, identifier and publisher (`yusai`, as the Tauri
  installer recorded it).
- **No Node in the build.** The brand's tokens, logos and fonts are vendored in
  `crates/app/brand/` (fonts as TTF from Google Fonts' repository, which the UI embeds), the
  Lucide icons as SVG in `brand/icons/`, and the translations in `crates/app/i18n/`.

## Transcription

- **Whisper states are freed between recordings.** Each source's `WhisperState` holds about
  280 MB of VRAM (turbo-q5) and 700 MB of commit. The worker drops them once its queue is empty
  after a session is saved, a mic test or a benchmark, and recreates them on the next job.
- **GPU device name** comes from `whisper_rs::vulkan::list_devices()` (device 0, the one
  whisper.cpp uses) instead of parsing whisper.cpp's log output.
- **Flash attention** is on for GPU contexts (whisper.cpp's current default); a failure falls
  back to CPU like any other GPU init failure.
- **Remembered GPU failure** is keyed by model ID and GPU name; whisper.cpp does not expose the
  driver version. Settings → モデル has 「GPUを再試行」 to clear it.
- **GPU crash detection.** Before a GPU init the app writes `gpuInitInProgress` to `state.json`
  and clears it afterwards. If it is still set at launch, the init crashed the process, so the
  app records a GPU failure and uses the CPU.
- **Transcribing after Stop** (FR-25). The queued utterances stay in memory, as every backlog
  does: 16 kHz float audio, about 64 KB a second of speech per source, so an hour of non-stop talk
  holds about 230 MB. Writing them to disk was left out until a recording needs it. Hiding the live
  text a minute behind was considered and dropped: drawing it costs nothing next to Whisper, and
  the lines that do arrive are still worth seeing.
- **Backlog merging.** Inside the worker, "lag ≥ 5 s" is measured as the span between the oldest
  and newest waiting utterance of a session, which needs no clock and only matters when there is
  more than one job to merge. A segment Whisper returns from merged audio belongs to the
  utterance under its midpoint but keeps the span of every piece it covers (more than half of
  the piece or a second of it); clamped to one piece, a mic line of echo hid from the echo guard
  the 相手 lines it repeated.
- **Language 自動.** Utterances are decoded with language `auto` until one of 3 s or longer has
  been decoded; its detected language is then locked. The session keeps `language: "auto"`; the
  detected language is logged as a `language_detected` event.
- **Whisper never translates** (`asr/language.rs`). Told a language the speech is not in,
  Whisper translates into it or loops: with 日本語 set, English speech came out in the grey
  partials as 「このグリースを選択して、押し込みを…」, and a final without a prompt as
  「私は、私は、…」 (finals mostly stayed English only because of their prompt). After each decode
  the worker runs one decoder step on the start-of-transcript token over the encoder output the
  decode left in the state, which is how whisper.cpp detects a language, without a second encoder
  pass (5 ms on CPU; the probabilities match whisper.cpp's own detection). When Whisper is at
  least 0.9 sure a clip is in the other of Japanese and English, the clip is decoded again in that
  one, and the source's next clips start in it while Whisper stays at least 0.5 sure; then they go
  back to the configured language. On real speech, Japanese scored at most 0.74 for English (0.6 s
  fragments and silence, turbo-q5 and base-q5) and English at least 0.9 (turbo-q5: 0.999). Clips
  Whisper is unsure of keep the fixed language, as section 8 asks; 自動 works the same way once it
  has locked.
- **Primer** 「えー、それでは始めます。…」 is used only when decoding Japanese.
- **Partials** run only on GPU, when no final job is queued or running, and when the model's
  benchmark tier is 快適 (or no benchmark has run).
- **Segmenter details.** The utterance keeps up to 320 ms of trailing silence (capped by the
  hangover). Pre-roll never reaches back into the previous utterance, so no audio is transcribed
  twice. The soft maximum cuts at the middle of the quietest 6-frame (96 ms) window in the last
  3 s; the hard maximum is checked before a frame is appended so no utterance exceeds 25 s.
- **Echo guard.** A 自分 line that matches an earlier 相手 line is never written; one already
  written is removed with `segment_removed` when its 相手 match arrives. Besides pairwise Dice
  similarity, the 自分 line is compared with all overlapping 相手 lines joined (the share of its
  bigrams found there), because the mic's VAD often cuts one echo line across several 相手
  lines. With turbo-q5 this took the echo left in the two-channel fixture from 82 characters to 0.
- **Short utterances get no context prompt.** With under 1 s of speech the prompt is the word
  list only: given the previous line, Whisper tended to repeat it (a lone 「四」 came out as
  「よろしくお願いします。」).
- **Mic test** runs the VAD and segmenter over its 10 s recording and transcribes only the
  speech, so a silent test shows 「言葉は聞き取れませんでしたが、続けて大丈夫です」 instead of a
  hallucinated 「ありがとうございました。」.
- **Hallucination list** is matched after NFKC; half-width katakana normalizes to katakana, so
  hiragana phrases do not match their katakana forms.

## Audio capture

- **App picker** lists sessions on every active render device, not only the default one, so an
  app playing to a headset that is not the default still shows up.
- **App capture can start before the app runs.** The capture thread waits for a process with the
  same executable (the reattach loop of FR-14), so the Start hotkey works with a last-used app
  that is closed. The status row says 「Zoom の音声を待っています」.
- **Default-device changes (FR-15)** are debounced for 1.5 s and reopen the stream only if the
  default actually changed. Opening a Bluetooth hands-free mic makes Windows flap the default
  while the headset switches profiles; reopening on every notification thrashed.
- **Device switches** show a notice but no timeline marker; the 「音声ソース再接続」 marker is
  for an app that exited and came back.
- **`read_from_device`** with an exactly sized buffer is used instead of
  `read_from_device_to_deque`, which unwraps the `ReleaseBuffer` result inside `wasapi`.
- **Missing QPC timestamps.** A packet with timestamp 0 or the timestamp-error flag is placed at
  "now minus its duration".
- **Gaps are never materialized.** The clock reports gaps as a frame count and the DSP thread
  feeds silence in half-second pieces, so an app that was closed for minutes costs no memory.
- **Quiet streams.** System loopback delivers no packets in silence; after 500 ms without packets
  the DSP thread pads silence up to 300 ms before now so an open utterance can close.
- **Health checks.** "No packets for 5 s" restarts mic and app streams once; only the mic then
  raises a banner, because process loopback can legitimately go quiet. System loopback is exempt.
- **Files (FR-18) are decoded by Media Foundation**, not a Rust decoder such as symphonia: it reads
  every container and codec Windows plays (the AAC in MP4 and M4A from Zoom, Teams and phones, MOV,
  MKV, WebM, WMA, MP3, FLAC) and a video's audio track, with nothing new to ship; symphonia has no
  Opus, AC-3 or WMA. Its own error codes (`0xC00D…`) read as 「このファイルの形式は読み込めません」,
  a file with no audio track as 「このファイルには音声がありません」.
- **Ogg Opus goes through libopus**, which the app ships for FR-09, because Windows can't read it:
  `.opus` voice messages, and Chidori's own `audio/*.ogg`, so a session can be transcribed again
  with another model. Only mono and stereo (mapping family 0).
- **A file session is a session with one `file` source** (相手), not a new kind, so History,
  playback, projects, exports and recovery work unchanged. It is named after the file, and
  `startedAt` is when it was transcribed, so History lists it on that day. Every time it shows is
  the place in the file (`00:12:34`), never a clock time: when a file was recorded is unknown (its
  modified time is often when it was downloaded or copied). 続きを録音 onto one is allowed; the
  added part's times are places in the session too.
- **It runs as finishing from the start** (the `finishing` state): pause, cut, star, screenshots
  and the lag notices mean nothing for a file, and finishing already shows progress, saves at the
  end and offers 処理を中止. The bar is by file time: what was read, minus the speech still queued.
- **Reading waits while 2 minutes of speech are queued.** That keeps memory bounded (queued speech
  is 16 kHz float, about 230 MB an hour) while keeping the queue far enough behind that jobs merge
  into 25 s ones. Measured on the meeting fixture (turbo-q5, RTX 5070 Ti, 2026-10-23): 3 minutes
  of sound in 1.4–3 s from MP4, M4A, WebM, FLAC and Opus, CER 20–25% against one-pass 22.9%.
- **Unprocessed audio**: a cancelled file, or one that broke off, marks what was queued and what
  was never read as unprocessed. So does a model that failed to load: finishing (of a recording
  or a file) waits 2 s for a load that might still be on its way, then saves what it has, as if
  cancelled, instead of waiting on 「仕上げ中…」 for ever. A file read to its end never does, though containers often state
  a length a little different from the sound's (an MP4's video track may run longer).

## Sessions and exports

- **`session.json`** also stores the SHA-256 of files Chidori wrote (`written`), which is how
  an edited `transcript.md` is detected and never overwritten.
- **Rename** changes the title only; the folder keeps its original name.
- **Whether a session needs recovery comes from its log**, not from `session.json`: a snapshot
  is left over by a recording that continued the session and then crashed (FR-08), and an edit
  writes one for a session that never stopped. 履歴 still reads the quicker snapshot when the
  log's last recording stopped.
- **Recovery is offered one session at a time**; 後で (or Escape) puts off all of them until the
  next launch, since the cards look alike and one press per session read as a button that did
  nothing. The dialog says how many more wait. Put-off sessions stay 要復元 in 履歴, where
  ⋯ › 復元 recovers one.
- **Continuing a file's session** keeps the file's times as places in the file and gives the
  continued recording clock times, as any recording has.
- **Start and Stop pressed twice** (a double click, a hotkey and the tray) start or stop once:
  the second press gets `E_BUSY`, which shows nothing.
- **Projects live beside the sessions** (FR-64): `projects.json` in the output root holds each
  project's ID, name and colour, and a session logs only the ID (`project_set`), so the list
  moves with the folders and a rename shows everywhere at once. Deleting a project leaves its
  sessions' IDs in place, naming nothing, so they read as in no project and no session folder is
  rewritten. The Markdown front matter looks the name up beside the folder.
- **A cut is a marker** (`cut`, FR-06), not a new session: the part shares the session's
  sound, line and folder, and a cut needs no new event in the log. It works while paused, so
  a break can end a part. Markdown heads the part with `## HH:MM:SS`, the PDF with the time
  over a hairline as a heading (so the PDF outline lists the parts), and plain text with
  「— HH:MM:SS 区切り —」. Parts have no names yet; the marker's `detail` would hold one.
- **"The line being said" is an utterance, not a time** (FR-07). A line's text arrives seconds
  after it is spoken, so the star cannot just mark the newest line: while someone speaks that
  is the line before. Each DSP thread notes the utterance it has open and the last one it
  closed (`SessionCtx::speaking`); the star takes every utterance open at that moment, else the
  one that ended last, and the store keeps the mark until that utterance's text arrives. Whisper
  may split one utterance into several lines; the mark goes to the one spoken at the moment of
  the press. An utterance that gives no line (too short, a filtered hallucination, 自分's echo
  of 相手) passes its mark to the line of either side being said at the press, else the last one
  that ended within 10 s before it; with neither, the mark is dropped. Marking from the
  transcript works on any line, live or after, through `mark_segment`.
- **Important lines in exports** start with `★ `, a character an agent reads as plainly as a
  person, rather than bold or highlight markup, which differs between Markdown flavours and
  would be lost in plain text. An important line is its own paragraph, so the star never
  claims a neighbour's words.
- **Switching the source is a new capture plan, not a new session** (FR-17). Capture stops,
  the DSP threads flush their last utterances, and the new plan starts at the same session time,
  as a resume does; the session adds the new source to its list (so two apps may both be 相手)
  and logs a `source_changed` marker naming it in Japanese, like every export string. 相手 and
  自分 labels and the echo guard now ask whether both sides were ever recorded
  (`Session::two_sides`), not how many sources there are, so Zoom → Teams alone stays unlabelled.
  If the new source fails, the old plan restarts and the picker goes back to it. While paused
  the new plan waits for resume. 「システム全体に切り替える」 after a silent app is the same switch.
- **Continuing a session appends, it does not merge** (FR-08). The new recording starts at the
  next 100 ms step after the session's end, so the transcript, the screenshots and `levels.bin`
  carry on in one session time, and each continuation records its wall-clock start
  (`Session::continued`) so the clock times after it stay true even days later; the timeline
  marks the seam with a `continued` marker (with the date when it differs). Session zero then
  lies the session's length before now, which may be before the PC started, so the recorder's
  and the stream clock's `t0` are signed. IDs carry on after the highest each kind used in the
  log, removed lines included. `session_continued` is logged only once capture runs, so a
  source that fails leaves the session as it was; a session that ended in a crash is recovered
  first. The title, project and model of record stay the session's; the source is whatever the
  picker holds now, and new sources join its list.
- **The sound is one Opus file per recording** (FR-09). The DSP threads already make 16 kHz
  mono audio placed in session time, so they hand it to one writer thread, which adds the
  sources together by position (a source that sent nothing for 1.5 s stops holding the mix
  back), fills a pause with silence, and encodes 20 ms frames. Opus (libopus 1.6, built from
  `opusic-c`'s bundled source) at 24 kb/s keeps speech clear in about 11 MB an hour, against
  115 MB for WAV. The app plays it through `cpal`, decoding with the core's own Ogg Opus reader
  (`screens/main/playback.rs`); with no seek in the reader, playing from a point decodes up to
  it, which takes a moment far into an hour. A page, and a flush, about once a second: a crashed
  recording's file plays up to its last second. One
  mixed file rather than one per source, because playback is for listening back, where both
  sides belong together. A continuation writes the next file, as an Ogg stream cannot simply be
  appended to; playing runs from one into the next. Encoding takes well under 1% of a core. It
  is on by default (the point of keeping it is having it when wanted) and can be turned off in
  設定 › 音声.
- **Unprocessed audio** after 処理を中止 is recorded as an `unprocessed` marker and
  `unprocessedMs`, rendered as 「（以降、未処理の音声 N 秒）」.
- **Exports are Japanese** in any UI locale: the session stores 自分 / 相手 and the spec defines
  the export strings in Japanese.
- **Joining text.** Besides the spec's Latin-letter/digit rule, a space is inserted after Latin
  sentence punctuation (`. , ! ? ; :`) before a Latin letter, so English sessions read naturally.
  Japanese joins are unaffected.
- **Markdown escaping.** `\ * _ ` [ ] < >` are escaped in transcript text, and a paragraph that
  would start a heading or list is escaped. Front-matter values are YAML-quoted when needed.
- **PDF through Typst** (section 11). The `typst` crate compiles the PDF in process from
  `export/transcript.typ`, so no hidden print window is needed. The session becomes a Typst
  document: the preamble, then one call per block (`#entry`, `#shot`, `#note`) with every piece of
  session text in a string literal, so none of it can be read as Typst syntax. Calls rather than
  a JSON data file keep the exported `.typ` readable. The preamble alone decides the look: A4,
  18 mm margins, Yu Gothic (Meiryo and MS Gothic if missing), the brand's ink with 相手 in 朱,
  子持ち罫 under the title (the kamon's double line marks the top of a printed page), and a footer
  with the app's name, small. Zen Kaku Gothic New is not used here: it is bundled only as web
  fonts for the window. Fonts are read from `%WINDIR%\Fonts` for each export
  rather than kept in memory (about 15 ms), and Typst's memo cache is cleared after each one. A
  one-minute session with a screenshot renders in about 0.2 s (debug build) to 330 KB. Typst is
  Apache-2.0: its LICENSE and NOTICE are installed in `licenses/`. The print HTML stays only as
  the fallback, opened in the browser when Typst fails.
  `cargo run --example export_pdf -- <folder> <out.pdf>` renders a session without the app.
- **Typst file.** 書き出し → Typst saves that same document, so `typst compile` gives the app's
  PDF (checked with the standalone Typst 0.15 compiler on a real session saved under a name with
  a space).
- **Markdown is one option with a ZIP switch.** Off, the user picks a directory and gets a new
  folder named after the session (`-2` if taken) holding `transcript.md` and `images/`, the
  session folder's own layout, so links stay `images/…`. On, the same two go into one ZIP (no
  PDF), written through a `.zip.part` file so a failure leaves nothing half-written. The switch
  is remembered while the app runs; the compact window's menu has both as separate entries.
- **Every export asks where to save it** and none is written into the session folder: a save
  dialog over the window, starting in Downloads, suggests the session folder's name. Typst
  copies its screenshots into `<name>_images/` beside the file (named after it, so an `images/`
  already there is untouched) and links them there. The PDF opens when saved; the others only
  say where they went. If Typst fails, the print HTML goes where the PDF was to go, its
  screenshots as `file:///` links into the session folder. `transcript.md` is still written into
  the session folder on Stop: it is the recording's own copy, not an export.

## Name

- **Chidori, formerly Kikitori.** The kamon brand names each app after the motif in its logo, and
  this app's logo is the plover of 波に千鳥, so it is Chidori — 文字起こし / Transcription. The name
  alone sits in the window title, title bar, tray and notifications; the name with its descriptor
  in the README titles and the store-like places.
- **Everything was renamed**, the identifier included (`dev.yusai.chidori`, crates `chidori` and `chidori-core`,
  the GitHub repository `chidori`, whose old address GitHub redirects, so Kikitori's
  updater still finds new releases). The repository has since moved to the `yusai-apps`
  organisation (`yusai-apps/chidori`); GitHub redirects `yusaitajiri/chidori` too, so copies whose
  update endpoint still names the old owner keep updating. The app's folders are named after the identifier and the
  install after the product name, so two things carry existing installs over: the installer
  silently uninstalls Kikitori before installing Chidori (`REMOVE_KIKITORI` in `xtask`; a silent
  uninstall keeps the app data), and on its first launch Chidori moves `%APPDATA%` and
  `%LOCALAPPDATA%\dev.yusai.kikitori` into its own folders entry by entry (never copying the
  models) and renames `Documents\Kikitori` to `Documents\Chidori` while the save folder setting
  still points at the old default (`migrate.rs`). A folder it can't move stays where it is and
  keeps working: the old save folder stays the setting.
- **The updater key keeps its file name** (`~/.tauri/kikitori.key`): the key itself is what
  existing installs trust, and `scripts/enable-updater.mjs` still defaults to that path.

## UI

- **Look: Chidori's palette on the Jello brand** (round 9, design I2, approved 2026-11-08; the
  `jello-brand` repository's `brand-ui` skill). Greyscale chrome on a warm grey ground (`#F2F2F0`)
  with white surfaces and ink (`#1C1C1A`); colour says only **who is speaking** (相手 coral
  `#D2432F`, its wave `#F58A78`; 自分 teal `#0B7A6A`, its wave `#4FC7B3`) and **that it is
  recording** (one red dot, `#F05A47`). The palette is `theme::p()` (light and dark, following
  Windows); the dark one is derived the same way (near-black ground, one step lighter surfaces,
  the ink card a light card) and is provisional until Yusai has seen it. Plain filled rounded
  rects, **no shadows, glows, gradients or blur anywhere**, and no borders except the 2 px rings
  named below. Jello has no palette or kit of its own, so the parts (buttons, switches, panels,
  chips, fields, dialogs) are `widgets.rs`.
- **The 8 px grid.** Every padding, gap, height and line height is a multiple of `theme::GRID`
  (`g(n)` is n × 8 px). Radii: 24 for panels and the recording card (`R_PANEL`), 16 for rows and
  chips (`R_ROW`), 8 for the ON AIR tag (`R_TAG`), 4 for a bubble's speaker corner (`R_NOTCH`),
  and buttons are pills (height / 2). The window buttons are 32 wide rather than 40 so four fit
  beside the tabs in 420 px.
- **One motion: jello.** `jello.rs` is the brand's `motion/jello.js` ported once (stiffness 380
  and 520, wobble 0.75, 4 substeps, settles within 0.05 px), checked against the brand's golden
  moves (`brand/jello-golden/`) within 0.5 px in a unit test. `motion.rs` draws it: a top that
  springs to its target and, while it moves, a darker side under it (`side`, `ink_side`,
  `thumb_side`); at rest only the top shows. It moves the title bar's tab pill, 設定's section
  chip and 履歴's project chip, the chosen source row, switch thumbs, a new transcript line (in
  from below), the recording card (in from where 開始 was), toasts (in from the bottom edge and
  out the same way) and dialogs. Colour changes only fade; nothing loops or idles. With Windows'
  animation effects off, everything jumps to its target with no side. Motion asks GPUI for its
  own frames while something moves, so the app's tick does not run for it.
- **Type.** Zen Kaku Gothic New Regular and Bold (Medium is gone: Jello has two weights, and the
  bold reads as the one emphasis) and IBM Plex Mono for the timer only, embedded in the exe (TTF
  from Google Fonts' repository, OFL). 14/24 body (the transcript too), 12/16 small, 11/24 for
  the times above bubbles, 13 for tabs and buttons, 16/24 bold headings, 20/24 mono timer (16 in
  the compact window).
- **The title bar** is 48 high on every expanded view: 「Chidori」 in bold (the logo drawing is
  gone from the bar; the app and tray icons stay the kamon kit's until Jello has an icon tool),
  録音 · 履歴 · 設定 as text tabs with the current one on a white pill, 録音 with a small red dot
  while recording elsewhere (it leads back to the recording), then always on top, shrink,
  minimize and close as muted icons. The setup wizard shows no tabs. No bottom tabs. When the
  window is too narrow for all of it (measured from the labels, since English's are wider), 「Chidori」
  gives way first, then the tabs tighten, so the window buttons never leave the window.
- **The start screen** is two labelled white panels. 「相手（聞き取るアプリ）」 with a refresh
  button lists the apps (those with an audio session first), then システム全体 and マイクだけ, as
  64 px rows of the name over 音が出ています or 無音, the chosen one bold with a check and a
  sliding highlight; no app icons. 「自分」 holds 「会話として録音」 (FR-12) and the mic. Under
  them, 開始 is a 72 px round ink button with the red dot, then 「開始」 with the hotkey when one
  is set, and 「ファイルから文字起こし」 as a quiet text button (FR-18). 開始 is the one circle
  besides the play button and the dots: everybody reads a round button with a red dot as "record".
- **Apps say whether they are playing.** The status under each app's name is real sound: the
  peak meters Windows keeps for every audio session (the volume mixer's bars), read by
  `audio::win::meters` only for the apps on the start screen, twenty times a second, with
  nothing captured and nothing sent in silence. A read takes about 30 µs and listing the
  sessions about 2 ms.
- **The recording card.** While recording, an ink card (24 radius, 16 padding) holds a white
  「ON AIR」 tag (the same in both languages for now), the title (click to rename), the source
  (`Zoom ＋ マイク`; a click opens the source menu, FR-17), 「遅れ n秒」 when 30 s or more behind
  (FR-25), the mono timer, and the live wave: bars drawn from the levels the backend sends ten
  times a second, 相手 rising above the middle in coral and 自分 dipping below it in teal, grey
  while paused. These are FR-03's meters: a decorative wave would look the same in every meeting
  and could not show a silent mic. The bar under the transcript (80 high) holds the camera and
  its menu, the star, the cut, then pause and the ink 停止 pill with its red square; while
  finishing, 仕上げ中 with a spinner.
- **The card floats over the transcript** (recording, a finished session, one opened from 履歴):
  the lines start below it and scroll up behind it, so only the card's own ground hides them. In
  a strip of its own the card cut them along a straight edge, under its rounded corners too.
- **Every scrolling area has the same scrollbar** (`scrollbar.rs`): the transcript, 設定 (each
  tab and the model list), 履歴, the setup wizard, the start screen and dropdowns. It shows
  whenever there is more than a screen: a grey thumb 4 px thick that grows to 8 on the jello as
  the pointer comes within 48 px of the edge, with no track and no side, and stays thin while
  something else (a dropdown, a dialog, a menu) is drawn over the area. A press on the track jumps
  there and goes on as a drag. The kit's scrollbar only thickens on hover and eases off the jello,
  and its scroll handle is private, so a page's body is the app's own scroll area, laid out as the
  kit's (its bounce at the ends on a Mac included). Areas are kept by a key and dropped when not
  drawn, so a page, a 設定 tab or a wizard step opens at its top. The 履歴 project chips (sideways,
  with no bar before either) and the kit's multi-line text box keep what they have.
- **On the transcript, the bar drives GPUI's list**: dragging away stops following the newest
  line, and dragging to the end follows it again. The kit's bar left the list's padding out of its
  range, so its thumb stopped a card's height short of the end; the card's room is now the first
  row's top padding instead. Rows not yet laid out count as 56 px, so a finished session's thumb
  is about the right length from its first frame.
- **Switching the source while recording is a menu** (the same as the compact window's): the
  source choices and 「会話として録音」, each switching at once with a toast. It is drawn where
  it fits and native where it doesn't (see "Menus are drawn by the app").
- **A finished session** keeps the card, now white: the title (click to rename), 「11月8日 22:00 ·
  41分 · Zoom ＋ マイク」 (the model in its tooltip), the project chip, and a 40 px round play
  button beside the session's wave, which a click seeks (and plays from, once playing). A past
  session opened from 履歴 has the same card with a back arrow. The bar holds コピー as an ink
  pill split from its ▾ (「Agent用にコピー」, 「Markdownでコピー」), 書き出し as a white pill
  (Markdown, PDF or Typst in a dialog; a menu in the compact window), 「新しい録音」 and ⋯. The
  folder the session was saved to is in ⋯ (フォルダを開く), not on the card.
- **Transcript: bubbles.** Every line is a white bubble at most about 77% of the width: 相手 on the
  left and, when both sides were recorded, 自分 on the right, with the top corner on the
  speaker's side at 4 px. The speaker's name (in their colour) and the time (in grey) sit above
  the first line of a turn; colour is only in the name. A ★ line and the line being played or
  jumped to get a 2 px ink ring; the line still being spoken is a hollow bubble with a 2 px grey
  ring and faint text. Hover actions (▶, ★, edit) sit beside the bubble, away from its side. A
  screenshot is a centred one-line marker (camera, time, スクショ) that opens the image, with its
  caption and delete on hover; thumbnails are gone from the flow, since an image in a 296 px
  bubble column is too small to read and pushes the talk off screen. Pause and reconnect markers
  are centred small text; a cut is centred ink text between two rules. With one source there are
  no names and every bubble is on the left.
- **Speaker colours** are coral and teal rather than one colour and ink: two hues tell the sides
  apart at a glance in the bubbles and the wave, and coral against teal stays distinct with
  red-green colour blindness, which two reds would not.
- **Playing back** is a ▶ in each line's hover actions and the card's play button; while it plays
  the button shows pause, the line being played has the ink ring, and a 2 px ink playhead moves
  along the card's wave, stepping four times a second. The transcript keeps up with the sound
  unless the reader has scrolled away. Not while recording: the file is still being written.
- **The compact window** (380 × 170) is three rows on the ground: the title and source (a click
  opens the source menu) with expand and close; a white 40 px strip with the chosen source before
  recording and the latest line with its speaker's name after; then the dot, the mono timer, a
  short live wave, the camera and a 40 px 停止 pill (開始 before recording, the finished
  session's actions after).
- **履歴** opens with a 48 px white search pill, then project chips (すべて, each project with its
  dot, プロジェクトなし, and ＋ for a new one), the chosen one on a sliding white pill. Choosing a
  project makes the page that project's, its dot and name in place of 「履歴」, with ⋯ to rename,
  recolour or delete it. 選択 picks several recordings to move to the Recycle Bin at once
  (`delete_sessions`), with quick picks for all and for the greyed short and empty ones.
- **Projects are coloured dots** (FR-64), the one place colours beyond the speakers and the record
  dot appear, so they stay small: a round dot before a title or on a chip, never a fill. A new
  project gets a colour no other project uses. A session joins a project from the chip on its
  card, History's row menu or 選択, each a menu ending in 「新しいプロジェクト…」. A new
  recording starts in no project.
- **設定 is chips**: 一般 · 音声 · 文字起こし · スクショ · ショートカット · 出力 in a row that wraps
  onto a second line when the window is too narrow for all six, the chosen one on a sliding white pill, each chip's current setting in its tooltip (and 「!」 on
  ショートカット when a hotkey is taken). Groups are white panels of 56 px rows: switches 48 × 32,
  ink when on; choices as small ground-coloured pills. The model list opens from 文字起こし with
  a back arrow, rather than being a seventh chip. 一般 ends with the version and ログ.
- **Banners and toasts.** An error banner is ink with a coral icon, a warning or note a white
  banner with a grey icon, each with words and its one action, since the palette has no tints
  for them (provisional until Yusai has seen it). Toasts are ink pills over the bottom bar, their
  icon telling the level.
- **Dialogs** are white 24-radius sheets over a scrim, entering with the jello, with full-width
  pill buttons.
- **Less text.** Choices have no descriptions under them (the export dialog lists formats only,
  settings have no help lines, the wizard's paragraphs are one short line or none), and labels
  are short. Pages and wizard steps change without a slide.
- **The star** sits between the camera and the cut while recording, and on every line's hover
  actions (live too, unlike editing). An important line has the ink ring and ★ by its time.
- **The cut button** (scissors) sits after the camera while recording. A cut shows in the
  transcript as a centred marker between rules, and a toast confirms it, since the compact
  window's strip shows only the latest line.
- **One primary per view**: 開始, 停止 or コピー on the main view, 保存 in a dialog, 次へ in the
  wizard; 使う in the model list and テストを開始 in the wizard's mic step are secondary.
- **Only what moves is redrawn.** The app ticks every 100 ms while recording (the live wave and
  timer step with the backend's levels), the spinners and toasts as they need; otherwise it
  draws only when something changes, and motion asks for frames only while it moves. Minimized
  or in the tray nothing ticks at all, and the core stops reading the apps' meters
  (`host::set_ui_hidden`), while its events still arrive, so the transcript keeps up.
- **The window opens expanded** (420 × 640) until it is shrunk; the compact window is remembered
  once chosen.
- **Text on its way.** While recording, speech 1.5 s or more behind (the backend's lag, FR-25)
  shows 「文字起こし中…」 with a spinner under the last line, or in place of 「話し声を待っています…」
  before the first one, unless a grey partial already shows the words coming. A fast GPU writes a
  line within that time, so the note is for slow PCs, where text can take many seconds. 仕上げ中
  has a spinner too, since one long stretch can keep its count still.
- **A session's picture is its sound.** A shape built from when each side spoke is flat in
  continuous speech, where every slice has speech, so the recorder keeps the 10 Hz levels in
  `levels.bin` (2 bytes per 100 ms, paused time silent), and the finished session's card draws
  it as bars of mean loudness, raised together so the loudest stands full height (at most 2×). A
  session without `levels.bin` falls back to when each side spoke. History's rows no longer draw
  it: a 64 px row has no room for a readable wave beside the title, and the time, length and
  screenshot count say more.
- **History is grouped by day** (今日 and 昨日 with the date, then dates) and searchable by title
  and opening words (`SessionSummary.preview`), each day one white panel of 64 px rows (the project's dot, the title, then the time, length
  and screenshots, with the opening words as its tooltip) and no dividers. A recording shorter than 10 s or with no transcript (most likely a test or
  started by accident) is greyed out; it still opens, and its menu still deletes
  it. 選択 picks several recordings to move to the Recycle Bin at once (`delete_sessions`), with
  quick picks for all and for the greyed short and empty ones.
- **続きを録音 is in the ⋯ menus** (a finished session's and History's rows), not a button of its
  own: the finished session's row already holds four buttons in the compact window, and
  continuing is rarer than starting anew. It records with the source the picker holds and opens
  the recording view; while something records, it is greyed out.
- **A session has one ⋯ menu** (`screens/session_menu.rs`, docs/OBJECTS.md), wherever it is shown, so
  it offers the same actions open or as a row: 復元 (a crashed session), 続きを録音, フォルダを開く
  and 削除. A History row, which has no card, adds コピー ▸, 書き出し…, 名前を変更 and
  プロジェクト ▸. Rename and delete confirm in dialogs shared by both (`screens/dialogs.rs`). Deleting
  the session on screen takes it off: the main view goes back to 開始, a past session to 履歴.
- **Always on top is off by default**, toggled by a pin in the title bar on every page.
- **Screenshots take the recorded app by default**: its window while recording an app, the
  cursor's screen while recording system audio or the mic alone (`appWindow`).
- **The camera has a menu** (a chevron joined to it, FR-34), a popup like ⋯ (native where the
  window is too small). The three targets are the setting itself, so a choice there is
  remembered, as in 設定 › スクショ; a window from its list (up to 20, front first, Chidori's own
  left out, from the same `xcap` listing) holds for the rest of the recording only, since a
  window seldom outlives one. A picked window that has closed or been minimized falls back to the
  cursor's screen, with the usual 「（カーソルの画面）」 toast. The picked window travels in
  `recording://state` (`shotWindow`), so the menu's check marks follow the backend.
- **Japanese line breaks** follow Unicode's line breaking (cosmic-text), so a line can break
  inside a word such as ヘッドホン or ください. The web UI broke between phrases
  (`word-break: auto-phrase`); doing that here needs a phrase splitter of our own.
- **The transcript builds only the rows in view** (`screens/main/transcript.rs`): GPUI's `list`
  measures and draws only the rows on screen, and the list's rows are rebuilt only when the
  transcript changes, so a line frame never rebuilds the transcript. (Under Iced, which lays out
  what it is given every frame, 2000 plain rows cost 13% of a core and 71 MB in the spike.)
- **Screenshot placement is strictly chronological** (section 9). Placing a screenshot after
  every sentence containing its time put it after sentences that started later: Whisper often
  cuts one utterance into back-to-back segments, and a segment starting exactly at the
  containing one's end counted as inside.
- **最新へ jumps from far away.** More than two screens above the newest line it scrolls there
  at once instead of gliding: in a long session left in the background the glide took seconds,
  and rows measured on the way kept moving the end.
- **More hotkeys.** Important mark and cut have hotkeys too (off until set), since a meeting
  app is usually in front. Pressed from another app, they confirm with a toast; all four share
  one registration loop, and one set to the same keys as another is reported, not registered.
- **The timer leaves out paused time** (`StatePayload.elapsedMs`); session times still count
  it, so the clock times in the transcript stay true.
- **The session viewer has its own transcript** (`screens/viewer.rs`), so a
  recording that runs while a past session is open keeps receiving its lines.
- **Menus are drawn by the app** (`screens/popup.rs`): the ⋯ menus, コピー ▾, the camera's, the
  source menu and every right click open a white sheet with the `line` ring at the pointer, rows
  that wash under it, a submenu beside its row, ↑ ↓ → ← Enter and Esc. A menu taller than the
  window (the compact window, 380×170, with a long app list) falls back to the native one
  (`native/menu.rs`, through `muda`), which can reach past the window; the tray's menu stays
  native. A pick comes back as an id, routed by its prefix to the screen that opened it, or as
  `msg:<n>` for a menu built from the app's own messages (`popup::Menu`).
- **A right click opens the menu of what is under it**: a history row its ⋯ menu, the session
  card 名前を変更 and its ⋯ items, a line この行をコピー and its hover actions, a screenshot its
  image, caption and delete. The title bar and the compact header open the window's menu
  (常に手前に表示, 縮小/拡大, 最小化, 閉じる) in place of Windows' system menu, and a text box
  切り取り, コピー, 貼り付け, すべて選択 in place of the kit's English one.
- **Hover is one rule** (`theme::hover_of`, `theme::wash`): an ink pill goes a step toward its
  text, a white pill a step toward ink, and a button with no ground of its own (an icon, a
  ghost, a row of a menu) lays a thin wash of ink over whatever it sits on, so it shows on the
  ground, a panel, a dialog or a hovered row alike. A label whose button changes colour on hover
  takes that colour from the button rather than its own.
- **No screen reader or keyboard navigation yet.** The app gives GPUI no accessibility labels or
  tab order yet, so the web UI's `aria-label`s and tab order have no counterpart. Icon buttons
  keep their names as tooltips; Escape, Enter and the hotkeys work. Revisit in the redesign.
- **Text boxes save on Enter** (and the check button), as they did under Iced, which had no event
  for a box losing focus.
  設定's word list and title format also save at the next click anywhere.
- **Closing the window** hides it to the tray only while recording or finishing; when idle it
  quits (FR-93 only covers the recording case).
- **Window position** is remembered with `tauri-plugin-window-state`; the size follows the
  layout. Settings, History and the wizard use the expanded size temporarily.
- **Hotkeys are off until set.** Both default to empty: obvious choices such as `Ctrl+Alt+R` and
  `Ctrl+Alt+S` are often held by another app already, and a warning at every launch would be
  worse than no shortcut. 設定 › ショートカット sets each one or turns it off again; an empty one
  is never registered. A shortcut that fails to register is reported only there (FR-92).
- **Shutter sound** (設定 › スクショ, off by default) is two short bursts of decaying noise that
  `screenshot::shutter_wav` builds at runtime, so the installer ships no audio file. It is played
  from Rust with `PlaySoundW` once the capture succeeds, so it also sounds while the window is in
  the tray.
- **Hotkey recorder** requires Ctrl or Alt (function keys may stand alone) so a shortcut can never
  swallow normal typing.
- **i18n placeholders** use single braces (`{n}`) to match the spec's string table.
- **Notices** carry an i18n key plus `params` and a `toast` flag so Rust never formats UI text.

## Languages

- **The first launch follows the Windows display language** (`GetUserDefaultUILanguage`):
  Japanese on a Japanese Windows, English on any other, so the setup wizard is already in the
  user's language. Once saved, the choice in 設定 › 一般 wins.
- **The UI names the sources.** The state the backend sends says which sources are recorded and
  the app's name (`StatePayload.sources`), and the title bar words them in the UI language
  (`Zoom + マイク`, `Zoom + Mic`); hovering shows all of it when the bar is too narrow. 「マイク」
  is the short form, so the English word is "Mic".
- **A new session's default title** names the source in the UI's words at the start: the app's
  name, システム全体 / All system audio, or マイク / Mic. The title is the user's to rename;
  exports stay Japanese (see Sessions and exports).

## Updates

- **The app checks for itself** (`crates/core/src/update.rs`, `crates/app/src/updater.rs`): it
  reads `latest.json` from the newest GitHub release, downloads the installer it names and checks
  its minisign signature against `crates/core/src/update.pub`. The manifest format, the key and
  the installer's switches are the Tauri updater's, so copies installed before the rewrite take
  the first Iced release as an ordinary update, and later ones the same way.
- **Launch check** runs 10 s after launch and at most once a day; the time is recorded when the
  check runs, whatever its result. A found update is a banner 「新しいバージョン x.y.z があります」
  with 「更新する」.
- **Install** is refused while recording or finishing, and the recorder lock is held from the
  last check until the installer starts, so a recording can't begin in between. The passive NSIS
  installer closes the app and starts it again (`/R`).
- **Release signatures are checked before publishing**: `xtask dist` verifies the new installer's
  `.sig` against the app's public key, so a release signed with another key fails in CI instead
  of on every user's PC.

## Security

- **No web content.** The UI is native; nothing renders HTML or runs script, so the Tauri build's
  capability list and CSP have no counterpart.
- **Opening a folder** only opens paths inside the output root, the models folder or the log
  folder (`ops::allowed_path`).
- **TLS** uses rustls with the ring provider (installed at startup).

## macOS (issue 6, step 1 and part of step 2)

- **Builds.** `gpu-metal` (`whisper-rs/metal`) is the Mac GPU build. Cargo's `[env]` in
  `.cargo/config.toml` cannot be limited to one target and carries MSVC flags for whisper.cpp,
  so Mac builds source `scripts/macos-env.sh`, whose clang flags win over `[env]`.
- **Capture through `cpal`'s Core Audio host** (`audio/mac/`), not ScreenCaptureKit: the mic is
  an input stream, and システム全体 is `cpal`'s loopback stream on the default output, a Core
  Audio process tap (macOS 14.6+). It needs no screen-recording permission, only the audio one,
  and it adds no crate. Buffers carry Core Audio's host time, which is `mach_absolute_time`, and
  `platform::qpc_now_100ns` reads the same clock on macOS, so section 7's placing by timestamp
  holds. The callback never blocks Core Audio's real-time thread: a full queue drops the buffer
  and logs it.
- **Default devices (FR-15)** are polled once a second, as on Windows, with the same 1.5 s
  settle; `cpal` only follows the default for output streams.
- **One app's sound is not done yet.** `app_loopback_supported()` stays false on macOS, so the
  picker disables apps the way Windows 10 does, and the app list is empty. A process tap per
  app (with its helper processes) is the next step.
- **The window** is hidden from captures with `NSWindow.sharingType = .none`, hidden to the
  menu bar with `orderOut`, and kept on top with the floating level. Notifications go through
  `osascript` (no bundle or permission needed). A second launch hands over to the running copy
  through a Unix socket in the user's temp folder, as a named mutex and event do on Windows; Launch
  Services already keeps a `.app` to one copy, but a binary started from a terminal is not.
- **Display language** is the first of `defaults read -g AppleLanguages`.
- **Quitting frees the model in GPUI's quit hook** (`on_app_quit` in `app::run`): `terminate:`
  calls `exit` without returning to `main`, so `main`'s `StopWorker` never runs, and ggml's Metal
  device aborts at exit while a Whisper model is still loaded (issue 16).
- **Kikitori migration** is Windows-only: Kikitori never shipped for macOS.
- **Packaging (step 4).** `cargo xtask dist` on a Mac builds the Metal app, has cargo-packager
  make `Chidori.app` (Info.plist keys from `crates/app/macos/Info.plist`), signs it ad hoc and
  packs `Chidori_<version>_aarch64.dmg` with `hdiutil`. Apple Silicon only, macOS 14.6+. Ad hoc
  because a Developer ID costs $99 a year: a downloaded copy opens through System Settings →
  Privacy & Security → Open Anyway, and macOS may ask for the mic and system audio again after
  an update (the signature changes with every build). A Developer ID and notarization fix both.
  No hardened runtime, which would need entitlements for the mic. Bundled files live in
  `Contents/Resources`, which `host::Paths` reads on macOS.
- **Updates on macOS** use the same daily `latest.json` check, but the banner's button opens the
  release page to download the `.dmg`: there is no installer the app could run, and replacing a
  running bundle by hand is more than the app needs. `latest.json` stays Windows-only.
- **CI and releases.** `ci.yml` adds a `macos-latest` job (clippy with `gpu-metal`, tests); the
  repo is public, so its minutes are free. `release.yml` builds the Windows installer and the
  `.dmg` on their own runners and drafts one release with both.

## Linux (a test platform, not a release)

- **Why.** Cloud sessions and CI runners are Linux. Building there lets an agent build, test and
  drive the real app (`docs/DRIVE.md`) without a Windows or Mac machine; Linux users are not a
  target, and nothing is packaged or released for it.
- **What runs.** The whole core and the GPUI app (GPUI's own X11/Wayland backend, drawn through
  wgpu; on a headless machine Xvfb and Mesa's software Vulkan stand in for a screen and a GPU).
  The recorder is built on every OS: a recording runs from a driven run's sound files
  (`AppState::stand_in`), so record → Whisper (CPU) → transcript → export is testable end to end.
- **What does not, yet.** No device capture: `audio::open_source` returns a source whose start
  fails with a message, and the mic list is empty. No screenshots (`screenshot::NoCapturer`),
  no app list, no notifications, no popup menus (the drawn ones still work), no hotkeys. Real
  capture would be `cpal` (already the Mac path), whose ALSA, PulseAudio and PipeWire hosts give
  the mic and, through a monitor source, the whole system's sound; one app's sound would need
  PipeWire. None of it is planned.
- **Driving.** GPUI has no `render_to_image` on Linux, so a driven window always shows there and
  `shot` reads it back from the X server (`x11rb`, already in the lockfile through GPUI). Run it
  under `xvfb-run` or with `DISPLAY` on an Xvfb.
- **Builds.** `. scripts/linux-env.sh` first (clang flags over `.cargo/config.toml`'s MSVC ones,
  as on macOS); it lists the packages. `ci.yml`'s `linux` job runs clippy with `drive` and the
  tests, CPU only, with no debug info (the GPUI tree's passes the runner's disk).

## Test assets

- **Benchmark clip** `crates/app/resources/bench_ja.wav`: 10 s (4.0–14.0 s) of chapter 4 of the
  LibriVox recording of 新美南吉『ごんぎつね』, released under **CC0 1.0**
  (archive.org item `gongitsune_um_librivox`). 16 kHz mono 16-bit, 320 KB.
- **Integration fixtures** are built by `tests/fixtures/build_fixtures.py` from the same CC0
  recording and the public-domain Aozora Bunko text (card 628) instead of being committed, which
  keeps several megabytes of audio out of the repository. See `tests/fixtures/README.md`.

## Measured results

On the reference machine (RTX 5070 Ti, Vulkan), 2026-10-02. Rechecked on 2026-10-05 before the
first release: the same turbo-q5 CERs, meeting latency p50 509 ms and p95 660 ms.

| Fixture, model | One-pass CER | Streaming CER | Latency (end of utterance → text) |
| --- | --- | --- | --- |
| meeting, turbo-q5, real time | 22.9% | 23.8% | p50 504 ms, p95 639 ms (37 utterances) |
| two-channel, turbo-q5 | — | 相手 23.0%, 自分 23.6% | no echo lines left |
| meeting, base-q5, GPU | 28.9% | 33.1% | — |
| meeting, base-q5, CPU (AVX2, the nightly setup) | 29.1% | 33.1% | p50 810 ms, p95 862 ms |
| two-channel, base-q5, CPU | — | 相手 33.4%, 自分 31.3% | no echo lines left |

turbo-q5 meets section 16 and 18. base-q5 loses about 4 points when cut into utterances, so the
nightly CPU run with base-q5 allows 6 points (`CHIDORI_CER_MARGIN`) and serves as a regression
guard; the three-point rule is checked with turbo-q5 here. Its two-channel 相手 CER sits 1.6
points under the test's 35% limit, so small decoding differences on the runner could trip it.
`CHIDORI_REQUIRE_FIXTURES=1` makes a missing model or fixture fail the run instead of skipping
it.
