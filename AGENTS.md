# AGENTS.md

Chidori is a Windows desktop app that transcribes live audio (one app, the whole system, or the
mic) with local Whisper, drops screenshots into the transcript where they were taken, and exports
Markdown, PDF and Typst. All Rust: the UI is GPUI (through `gpui-kit`) in `crates/app`, everything else (capture,
transcription, sessions, models, export) is `crates/core`, whisper.cpp through `whisper-rs`,
WASAPI capture.

## Read first

- `AppSpec.md` is the spec. Code comments cite it ("section 8", "FR-53"); read the section for
  what you are changing.
- `docs/DECISIONS.md` holds every choice the spec leaves open and every deviation from it. Where
  the spec is silent, pick the simplest option that keeps the UI one-tap simple and add a line
  there. When you change something the spec defines, update the spec section too.
- The look is Chidori's own palette on the Jello brand (the `jello-brand` repository,
  `plugin/skills/brand-ui/SKILL.md`): greyscale chrome, colour only for who is speaking (相手
  coral, 自分 teal) and that it is recording (one red dot), plain rounded rects on the 8 px grid
  (`theme::GRID`, `g(n)`), and no shadows, glows or gradients. Everything that moves uses the one
  jello port in `crates/app/src/jello.rs` (checked against the brand's golden moves in
  `crates/app/brand/jello-golden/`), drawn by `crates/app/src/motion.rs`. The palette and type
  are in `crates/app/src/theme.rs`, the fonts in `crates/app/brand/fonts/`, the shared parts in
  `crates/app/src/widgets.rs`. Read `docs/DECISIONS.md` → UI for how the app applies it.

## Commands

Windows 10/11 x64 with the MSVC build tools and Rust (the version in `.github/workflows/ci.yml`).
GPU builds also need the Vulkan SDK and LLVM (see `README.en.md` → For developers).
On Linux (a cloud session or CI; a test platform, not a release) source `scripts/linux-env.sh`
first; it lists the packages. Recording there runs only from a driven run's sound files, and a
driven run needs a display: `Xvfb :99 & export DISPLAY=:99` (`docs/DECISIONS.md` → Linux).

```sh
cargo run -p chidori                          # CPU build
cargo run -p chidori --features gpu-vulkan    # Vulkan build
cargo xtask dist                              # the release installer, as CI builds it
```

Vulkan builds need a short cargo target dir, or whisper.cpp's shader build passes Windows'
260-character path limit: set `CARGO_TARGET_DIR=%USERPROFILE%\.kt` for any cargo command with
`--features gpu-vulkan` (and for `xtask dist`). `xtask dist` needs `cargo install cargo-packager`.

Before you finish, run what CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --features chidori/gpu-vulkan -- -D warnings
cargo test --workspace
```

To check a screen or try a change end to end, drive the app (`docs/DRIVE.md`):
`cargo xtask drive start` runs the real app out of sight on folders of its own, and
`cargo xtask drive shot`, `click`, `type`, `state`... look at it and use it without the mouse, with
sound files in place of the devices. `crate::preview::app()` (`crates/app/src/preview.rs`) builds
the real `App` on a temporary core for unit tests of a screen's logic.

The streaming accuracy tests in `crates/core/tests/pipeline.rs` are `#[ignore]`d; its header and
`tests/fixtures/README.md` explain how to build the fixtures and run them. Partial (grey) text
only runs on the GPU, so its tests need `--features gpu-vulkan`.

## Conventions

- Code, comments, commit messages and docs are in English. UI text is Japanese first, English
  second: every string goes through `crates/app/src/i18n.rs` with keys in both
  `crates/app/i18n/ja.json` and `crates/app/i18n/en.json` (a test checks they match).
- The README is the exception: `README.md` is Japanese and `README.en.md` is its English twin.
  Change both together; the developer sections live only in the English one.
- Nothing the user records leaves the PC. The only network use is model downloads and the daily
  update check; keep it that way.
- The UI never does the core's work: what it asks of the core is a function in
  `crates/core/src/ops.rs` (run off the UI thread with `app::call`), and what the core tells it is
  an `events::Event`. Settings and state files keep the JSON layout the Tauri build wrote, so
  installed copies carry them over.
- OS-specific code stays behind the traits in spec section 6 (`AudioSource`, `ScreenCapturer`,
  `AudioAppLister`), so a macOS port only adds files.
- Library APIs drift: read the current docs (or the crate's source under `~/.cargo/registry`)
  before using a crate. Drift-prone crates (`gpui-kit`, the tray and hotkey crates) are pinned with `=`;
  the lockfile is committed.
- Never invent model URLs, SHA-256 hashes or file sizes; take them from the source.
- rustfmt uses `max_width = 120`. Files are LF (`.gitattributes`).
- Exports are snapshot-tested with `insta` (`crates/core/src/export/snapshots/`). Check a changed
  snapshot before accepting it.
- Comments say why, not what. Match the code around you.
- A commit's subject is one imperative line saying what changed for the user, for example
  "Show apps' live sound, keep one title bar, and draw the line less often".
- Never commit updater signing keys, Whisper models or generated fixtures (all git-ignored).
