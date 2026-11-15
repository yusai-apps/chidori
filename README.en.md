<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/chidori-on-dark.svg">
    <img src="docs/images/chidori.svg" alt="" width="72">
  </picture>
  <br>
  Chidori — Transcription
</h1>

[日本語](README.md) · **English**

Chidori was called Kikitori before. Updating from Kikitori keeps your settings, models and recordings.

Chidori is a Windows app that transcribes meetings, lectures and videos as they happen, **entirely on your PC**.
It tells an app's audio (Zoom, Teams and so on) apart from your microphone, labelling the lines Others and Me, and drops screenshots into the transcript at the moment you take them.
Afterwards, save it as Markdown or PDF, or use Copy for AI agents to ask an AI for a summary. Audio and video files you already have can be transcribed too.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/screenshot-dark.png">
  <img src="docs/images/screenshot.png" alt="Chidori while recording, in Japanese: the expanded window on the left and the compact one on the right. Lines from the other side (相手) and from you (自分); the line along the bottom is drawn from the live sound.">
</picture>

- Your audio never goes to the internet: transcription runs on your PC with Whisper.
- Runs on Windows 11 (64-bit). It also runs on Windows 10 22H2, but there it can't record a single app, only all system audio or the microphone.
- A Mac version (Apple Silicon, macOS 14.6 or later) is in early testing: it records the microphone and all system audio, not yet a single app.
- Disk space: 200 MB to 1.1 GB per model (the standard one is about 570 MB).
- Made for Japanese first, and in English too: unless Windows is set to Japanese, the app starts in English. Japanese and English speech are transcribed as spoken, never translated.

## Install

1. Download `Chidori_x.y.z_x64-setup.exe` from [Releases](../../releases/latest).
   - If your browser warns that the file isn't commonly downloaded, choose **Keep** from its "…" menu.
2. Open the downloaded file.
3. If a blue "**Windows protected your PC**" screen appears, click **More info**, then **Run anyway**.
   - It appears because the installer isn't code-signed yet. Chidori just lacks a signature: it doesn't change your PC's settings or ask for administrator rights.
4. Follow the installer. No administrator rights are needed (it installs for your user only).

### Mac (early testing)

1. Download `Chidori_x.y.z_aarch64.dmg` from [Releases](../../releases/latest), open it, and drag **Chidori** onto **Applications**.
2. Open Chidori. When macOS says it can't check the app for malware, click **Done**, then go to **System Settings → Privacy & Security** and click **Open Anyway** at the bottom (the app isn't notarized by Apple yet).
3. The first recording asks for the microphone, and **All system audio** asks to record the Mac's sound. Allow both.
4. When an update is out, its banner opens the download page: download the new `.dmg` and drag it over the old copy. macOS may ask for the permissions again.

On a Mac, transcribing a file works for WAV and Opus only for now.

## First run

Setup starts on first launch, in English unless Windows is set to Japanese.

1. **Choose a model**: the one that suits your PC is already picked. Keep it.
2. **Download**: the model downloads once (about 570 MB for the standard one). An interrupted download resumes where it stopped.
3. **Mic test**: speak for about 10 seconds; when your words appear, you're ready.
4. **Shortcuts**: off at first; you can set them later in Settings → Hotkeys.

To change the app's language later: Settings → General → Language. If your meetings are in English, also set Settings → Transcription → Transcription language to English.

## Using it

| To | Do this |
| --- | --- |
| Start or stop recording | **Start** / **Stop** |
| Choose what to transcribe | **Source**: an app (Zoom and so on), All system audio, or Microphone only. Turn on **Record a conversation** to add your own voice as Me |
| Transcribe a recording or video you already have | **Transcribe a file** under **Start** (in the compact window, inside the source picker), or drop the file on the window. Anything Windows plays works: MP4, MOV, MKV, WebM, M4A, MP3, WAV, FLAC, Opus and more. It runs as fast as your PC keeps up, and its times are places in the file (00:12:34) |
| Change the source while recording | Pick again from the source at the top of the recording (in the compact window, **Recording** in the title bar); it switches without stopping |
| Add a screenshot | **Screenshot** while recording. The **⌄** beside it picks what it takes: the app's window, a screen, or any other window |
| Mark where a new topic or scene starts | The **scissors** button while recording. Exports give each part a heading |
| Mark something important | The **star** button while recording (it marks the line being said). Each line's star adds or removes the mark too; exports start such lines with ★ |
| Use the transcript | **Copy**, or **Export** (Markdown as a folder or a ZIP, PDF, Typst). The ▾ beside Copy has Copy for AI agents and Copy as Markdown |
| See past recordings | **History** in the title bar. You can also fix or delete lines there |
| Listen back | A line's **▶** plays from that line, the play button below from the start (the sound takes about 11 MB an hour; turn it off in **Settings → Audio**) |
| Add to a recording you stopped | **Continue recording** in its ⋯ menu (or History's ⋯); the new part follows the old one |
| Group recordings into projects | Pick a project from the square beside a recording's title, a finished recording's header, or History's ⋯ and **Select**. Filtering History by a project turns it into that project's page, whose ⋯ renames, recolours or deletes it |

- Recordings are saved automatically in `Documents\Chidori`, one folder per recording named by date and title (`transcript.md` and images).
- Exports and copies are in Japanese whatever the app's language: lines are labelled 相手 (Others) and 自分 (Me).
- Through speakers, your mic may pick up the other side's voice. Headphones work best (duplicate lines are removed automatically).
- Start/Stop, Screenshot, the important mark and the cut can get keys that work from any app: Settings → Hotkeys (off at first).
- On a PC without a usable GPU, Chidori runs on the CPU. If it feels slow, choose **Lightweight** in Settings → Models. Once transcription is a minute behind, **After Stop** pauses it while you record, to keep the PC light, and catches up when you stop.

## Privacy

- Audio never leaves your PC. Each recording's sound is saved in its folder so you can listen back (about 11 MB an hour). Turn it off in **Settings → Audio** and it is discarded once transcribed.
- The network is used only for model downloads (huggingface.co) and a daily update check (GitHub).
- No telemetry.

## Troubleshooting

- **The app's sound isn't recorded**: switch the source to All system audio (an app that isn't playing anything can't be recorded on its own).
- **Shortcuts don't work**: Windows doesn't deliver them while an app running as administrator is in front.
- **Logs**: **Open logs** at the bottom of Settings. Logs never contain transcript text.
- **Uninstall**: Windows Settings → Apps → Installed apps → Chidori. Your recordings (`Documents\Chidori`) stay.

---

## For developers

All Rust: the UI is [GPUI](https://www.gpui.rs) through `gpui-kit` (`crates/app`), and capture, transcription, sessions and export are `crates/core`, with whisper.cpp (`whisper-rs`) and WASAPI capture. The full spec is [AppSpec.md](AppSpec.md); design choices are in [docs/DECISIONS.md](docs/DECISIONS.md).

### Prerequisites (Windows 10/11 x64)

- Rust (see the toolchain version in `.github/workflows/ci.yml`) with the MSVC build tools (Visual Studio 2022 Build Tools, "Desktop development with C++")
- CMake (comes with the Visual Studio C++ workload)
- For GPU builds: the Vulkan SDK (`winget install KhronosGroup.VulkanSDK --accept-package-agreements --accept-source-agreements`) and LLVM for bindgen (`winget install LLVM.LLVM`)
- For the installer: `cargo install cargo-packager --locked`

### Prerequisites (macOS, Apple Silicon, in progress)

The Mac port ([issue 6](https://github.com/yusai-apps/chidori/issues/6)) records the mic and the whole system's sound (macOS 14.6+); one app's sound is not done yet.

- Xcode, Rust (the same toolchain version), and CMake (`brew install rustup cmake`, then `rustup toolchain install <version> --component rustfmt,clippy`)
- Before any cargo command: `. scripts/macos-env.sh`. It gives whisper.cpp clang's flags in place of the MSVC ones in `.cargo/config.toml`, and puts Homebrew's rustup on `PATH`.
- `cargo run -p chidori --features gpu-metal` is the Metal build. The first recording asks for the microphone, and システム全体 asks for system audio recording, both for the app that launched it (your terminal, for a dev build).
- `cargo xtask dist` (after `cargo install cargo-packager --locked`) builds `Chidori.app` and the `.dmg` in `target/dist`, as the release does. Open the `.app` from there to get the permission prompts in Chidori's own name.

### Prerequisites (Linux, for tests only)

Linux is not a release platform: it builds so CI and cloud sessions can test and drive the app ([docs/DECISIONS.md](docs/DECISIONS.md) → Linux). Recording works only from sound files in a driven run; there is no device capture or screenshot.

- Rust (the same toolchain version), CMake, clang, and the packages listed in `scripts/linux-env.sh` (Ubuntu 24.04 names)
- Before any cargo command: `. scripts/linux-env.sh`
- Without a screen: `xvfb` and `mesa-vulkan-drivers`, then `Xvfb :99 & export DISPLAY=:99` before `cargo xtask drive start`

### Run and test

```sh
cargo run -p chidori                          # CPU build
cargo run -p chidori --features gpu-vulkan    # Vulkan build: set CARGO_TARGET_DIR=%USERPROFILE%\.kt first
cargo test --workspace
```

The window keeps itself out of screenshots and screen sharing by default (設定 › スクリーンショット). To capture the app itself, launch it with `--show-in-captures` (for example `cargo run -p chidori -- --show-in-captures`): that run shows in captures without changing the setting.

Vulkan builds need a short target folder: whisper.cpp's shader build otherwise passes Windows' 260-character path limit.

The integration harness (streaming accuracy and latency against fixture recordings) is described in `crates/core/tests/pipeline.rs` and [tests/fixtures/README.md](tests/fixtures/README.md).

### Build the installer

```sh
cargo xtask dist
```

This builds the Vulkan release, downloads the Khronos Vulkan loader (pinned SHA-256) and bundles it next to the exe, collects the licences of every bundled crate, font and icon set into `licenses\THIRD-PARTY-NOTICES.txt`, and writes `Chidori_<version>_x64-setup.exe` under `<target>\dist\`. With `CARGO_PACKAGER_SIGN_PRIVATE_KEY` (and its password) set, it also signs the installer for the updater and checks the signature against the app's key.

### Releasing

The updater checks `latest.json` in the newest GitHub release of this repository, and only installs an installer signed with the release key (its public half is `crates/core/src/update.pub`). The repository's Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` hold that key and its password; keep them safe, because without them installed copies can't be updated.

Each release:

1. Update `version` in the workspace `Cargo.toml` and move the `Unreleased` notes in [CHANGELOG.md](CHANGELOG.md) under the new version.
2. Commit, then tag and push: `git tag v0.6.0 && git push origin v0.6.0`.
3. The `release` workflow builds the installer, its signature and `latest.json`, and opens a **draft** release. Check it, then publish it; installed copies see the update within a day.

## License

[MIT](LICENSE). Whisper models are downloaded from Hugging Face under their own licenses (MIT, or Apache-2.0 for kotoba-whisper). The bundled Vulkan loader is © The Khronos Group under Apache-2.0 (see `licenses\VulkanRT-License.txt` in the install folder). The licenses of whisper.cpp, Typst, the fonts, the icons and every other crate inside the app are in the same `licenses` folder.
