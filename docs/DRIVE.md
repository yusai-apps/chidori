# Driving the app

A driven run is the real app, started so that a script (or an agent) can use it without a hand
on the mouse: it clicks, types, records and looks at the window through commands. It is how a
change is tried end to end when `cargo test` can't reach it: what a screen looks like, what a
double click on Start does, what a crash leaves behind.

It exists only in builds with the `drive` feature. A release never has it.

```sh
cargo xtask drive start            # builds with --features drive and starts the app, out of sight
cargo xtask drive shot             # draws the window to target/drive/shot.png
cargo xtask drive click x=640 y=92 # a click at that pixel of the screenshot
cargo xtask drive state            # what the app shows, as JSON
cargo xtask drive stop
```

## What a driven run is

- **The real app.** The same `main`, core, threads and GPUI window as any run. Input goes into
  GPUI as its own mouse and key events (`Window::dispatch_event`), so a click passes through the
  same hit testing and handlers as a hand's. The OS mouse and keyboard are never touched.
- **Out of sight.** The window is created hidden and never takes the focus (on Linux it shows,
  for `shot` to read it back from X; run Linux drives on Xvfb, see `docs/DECISIONS.md` → Linux). A screenshot is the
  window drawing itself to an image (GPUI's `render_to_image`), not a capture of the screen, so
  it works hidden, behind a game, or with 画面共有・スクショに映さない on. The system asks a
  hidden window for no frames, so the run draws its own, 60 a second, and motion plays as it
  does on screen.
  `cargo xtask drive start --visible` shows the window (still without the focus) to watch a run.
- **Its own folders.** Settings, sessions, logs and the window's place are under
  `target/drive/profile`, never the user's. A new profile links to the models already on the PC,
  is past the setup wizard, and uses the smallest model there. `start --fresh` starts a new one.
  `start --gpu` builds with Vulkan (set `CARGO_TARGET_DIR` as for any Vulkan build), or Metal on
  a Mac, which partial (grey) text needs.
- **Beside the app itself.** No single-instance rule, no tray icon, no global hotkeys, no update
  check, no system notifications. Their effects have commands of their own (`hotkey`, `tray`,
  `menu`).
- **Sound from files.** A recording hears no device: silence, or the files a `sound` command
  names, played at the speed they were recorded (`AppState::stand_in`). The fixtures of
  `tests/fixtures` (see its README) are made for this.

What it still does for real: playing a past session's sound comes out of the speakers, a
screenshot taken while recording captures the real screen, a copy writes the real clipboard, and
a file dialog (書き出し, a folder to pick) opens on the real screen and waits for a real click.

## Commands

`cargo xtask drive <command> key=value ...` appends one line to `target/drive/in.jsonl`
(`{"do":"click","x":640,"y":92}`) and prints the answer the app appends to `out.jsonl`
(`{"n":3,"ok":true}`, or `"ok":false` with an `"error"`). A script can also write `in.jsonl`
itself, before the launch or while the app runs: the app reads it from its first line and
answers each line with the same `n`.

Positions are pixels of a screenshot, so what is seen in `shot.png` is where to click.

| Command | Keys | What it does |
| --- | --- | --- |
| `shot` | `file` (default `shot.png`) | Draws the window to a PNG in `target/drive`. Answers with its size and scale. |
| `state` | | The view, the recording, the live transcript (with `live.scroll`: whether it follows the newest line, how far it is scrolled, how far it can scroll and whether its scrollbar is dragged), `scroll` (each scrolling page or dropdown on screen by its key, such as `settings-Transcription`, with its `top`, `max` and `dragging`), banners, toasts, the open dialog, a waiting popup menu and the settings. |
| `click` | `x`, `y`, `button` (`left`, `right`, `middle`), `count` (2 = a double click) | Moves there, presses and releases. |
| `move` | `x`, `y` | Moves the pointer there (hover, tooltips). |
| `drag` | `x`, `y`, `to_x`, `to_y`, `button` | Presses, moves in steps, releases. |
| `scroll` | `x`, `y`, `dy`, `dx` | Scrolls by that many pixels at the point. |
| `key` | `keys` | Keystrokes as GPUI names them, separated by spaces: `enter`, `escape`, `ctrl-f`. |
| `type` | `text` | Types the text into whatever has the focus. |
| `menu` | `id` | Picks an item of a popup or tray menu. `state` lists the menu the app last opened under `popup`; a drawn one also shows in `shot`, and a driven run never opens a native one. |
| `tray` | | A click on the tray icon. |
| `hotkey` | `name` (`toggle`, `screenshot`, `mark`, `cut`), `times` | What a press of that global hotkey does. |
| `window` | `event` (`hidden`, `shown`, `close`, `focused`, `drop` with `path`) | Tells the app its window was minimized, closed, focused, or had a file dropped on it. |
| `resize` | `w`, `h` | Sizes the window (logical pixels). |
| `settings` | `set` (a partial settings object) | Changes settings as 設定 does. |
| `sound` | `others`, `me` (paths; left out = silent) | The files the next recording hears as 相手 and 自分. |
| `wait` | `ms` | Waits. |
| `quit` (`stop`) | | Quits the app. |

A command is answered once it has been handed to the app, not once the app is done with it: a
click on Start answers at once and the recording starts a moment later. Wait on `state`
(`recording.state`) rather than on a fixed time.

## A recording, start to finish

```sh
cargo xtask drive start --fresh
cargo xtask drive sound others=tests/fixtures/generated/meeting.wav
cargo xtask drive hotkey name=toggle        # or a click on the Start button
cargo xtask drive wait ms=20000
cargo xtask drive state                     # live.items fills with lines
cargo xtask drive hotkey name=toggle
cargo xtask drive shot file=review.png
cargo xtask drive stop
```
