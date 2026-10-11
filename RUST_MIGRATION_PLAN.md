# nvencFFX Migration Plan to Rust (egui + eframe + wgpu)

This document outlines the step-by-step roadmap for migrating **nvencFFX** from Python (CustomTkinter + ctypes + Nuitka) to **Rust** using the **egui / eframe / wgpu** graphics stack.

> **Source baseline**: `nvencFFX.py` — 8,930 lines, 235 functions, 8 classes (`PROCESSENTRY32W`, `_GUID`, `TextCheckbox`, `CTkContextMenu`, `DropTarget`, `TrayIcon`, `TaskbarProgress`, `BatchConverterWindow`, `VideoConverterApp`).

---

## 1. Migration Goals & Benchmarks

| Metric | Current Python Build (Nuitka) | Target Rust Build (egui + wgpu) |
|---|---|---|
| **Distribution Format** | Directory with 68+ files (Python runtime, DLLs, `.pyd`) | **Single standalone `.exe`** |
| **Distribution Size** | ~51.5 MB (without FFmpeg) / ~374 MB build | **~12–15 MB** (UPX-compressed: ~4–6 MB) |
| **Startup Time** | 1.5–2.5 seconds | **< 50 ms (near-instant)** |
| **RAM Footprint** | ~80–200 MB | **~25–45 MB** |
| **UI Rendering** | Software / Tkinter Canvas | **Hardware-accelerated GPU** (DirectX 12 / Vulkan via `wgpu`) |
| **Win32 Integration** | Fragile `ctypes` bindings (~700 LOC) | Type-safe Rust crates (`windows-sys`, `tray-icon`) |

---

## 2. Target Rust Architecture

```text
nvencffx/
├── Cargo.toml
├── build.rs                  # Embed nff.ico and Windows manifest via winresource
├── src/
│   ├── main.rs               # Entry point, winit/eframe setup, windows_subsystem attribute
│   ├── app.rs                # Main App state (eframe::App implementation)
│   ├── app/batch.rs          # Batch Converter window + queue (child module of app)
│   ├── ui/                   # egui interface components
│   │   ├── mod.rs
│   │   ├── theme.rs          # Color palette (#4FB62F accent, dark mode, styles)
│   │   ├── widgets.rs        # Custom widgets (TextCheckbox, chip_button, radio_button, tooltip helpers)
│   │   ├── main_view.rs      # Main layout (Input/Output, codec, quality, actions)
│   │   ├── encoder_section.rs  # Advanced NVENC settings (preset, tune, multipass, AQ, profile, level, tier)
│   │   ├── fps_scale_section.rs # FPS mode, scaling, interpolation
│   │   ├── audio_section.rs  # Audio codec selection (Copy, AAC, Opus, Custom, Disable)
│   │   ├── trim_section.rs   # Trimming controls (timeline slider, -ss/-to, streamcopy, precise)
│   │   ├── presets_section.rs # Preset selector with save/load/delete
│   │   └── log_view.rs       # Real-time streaming FFmpeg output log
│   ├── core/                 # FFmpeg business logic
│   │   ├── mod.rs
│   │   ├── command.rs        # FFmpeg CLI argument builder (encoding, streamcopy, custom command modes)
│   │   ├── probe.rs          # ffprobe runner (media info, duration, codec detection, LRU cache)
│   │   ├── process.rs        # Process management (spawn, stderr reader, cancel, kill tree via toolhelp32)
│   │   ├── progress.rs       # Stream parser for progress (time=, fps=, speed=, size=, bitrate=, percentage)
│   │   └── estimate.rs       # Output file size estimation (ffprobe duration × bitrate)
│   ├── config/               # Settings & presets
│   │   ├── mod.rs
│   │   ├── settings.rs       # AppSettings: serde model for nff_settings.json (all 47 fields)
│   │   ├── presets.rs         # Preset JSON serde models, backward-compatible loading
│   │   └── defaults.rs       # Default values per codec, factory presets
│   ├── win32/                # Native Windows API integration
│   │   ├── mod.rs
│   │   ├── taskbar.rs        # ITaskbarList3 (green/yellow/red progress overlay)
│   │   ├── suspend.rs        # NtSuspendProcess / NtResumeProcess (pause/resume conversion)
│   │   ├── tray.rs           # Tray icon, menu, balloons + global hotkeys (Alt+F8 / Alt+F9)
│   │   └── process_tree.rs   # Kill ffmpeg.exe child tree via CreateToolhelp32Snapshot
│   └── recording/            # Screen recording subsystem
│       ├── mod.rs
│       ├── capture.rs        # FFmpeg ddagrab screen capture, segmented recording with auto-restart
│       └── audio.rs          # Desktop audio loopback via WASAPI (cpal or raw bindings)
```

---

## 3. Complete Dependency List

```toml
[dependencies]
# GUI framework
eframe = { version = "0.36", default-features = false, features = ["wgpu", "default_fonts"] }
egui = "0.36"

# Serialization (settings, presets)
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.154"

# File dialogs (Browse, Save As, Output Folder)
rfd = "0.17"

# Launch external files (Play Input/Output, Open output folder)
open = "5.4"

# Clipboard: reading for the text menu's Paste (eframe already uses it for copying)
arboard = { version = "3.6", default-features = false }

# Win32 API (ITaskbarList3, NtSuspendProcess, tray icon + RegisterHotKey, CreateToolhelp32Snapshot)
# Tray and hotkeys are raw Win32 too (Phase 5): `tray-icon` has no balloons, `global-hotkey` is not needed
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_System_Com",
    "Win32_System_Diagnostics_Debug",
    "Win32_System_Diagnostics_ToolHelp",
    "Win32_System_LibraryLoader",
    "Win32_System_Threading",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
] }

# Audio loopback capture (WASAPI)
cpal = "0.18"

[build-dependencies]
winresource = "0.1.31"  # maintained fork of the abandoned `winres`

[profile.release]
opt-level = "z"       # Optimize for size
lto = true            # Link-Time Optimization
codegen-units = 1     # Maximize cross-crate optimizations
panic = "abort"       # Remove stack unwinding tables
strip = true          # Strip all debug symbols
```

> **Note**: Before adding each dependency, verify the latest version on crates.io. The versions above are approximate baselines.

---

## 4. Settings Schema (nff_settings.json — 47 fields)

All fields from the current Python settings must be mapped 1:1 to a Rust `AppSettings` struct. Missing any field means broken backward compatibility.

| Group | Fields |
|---|---|
| **Paths** | `ffmpeg_path`, `last_input_dir`, `last_output_dir` |
| **Codec & Quality** | `codec`, `bitrate`, `constant_qp_mode`, `quality_level` |
| **Audio** | `audio_option`, `custom_abitrate` |
| **Encoder Core** | `encoder_preset`, `encoder_tune`, `encoder_profile`, `encoder_level`, `encoder_tier`, `encoder_coder`, `encoder_threads` |
| **Encoder Advanced** | `encoder_hwaccel`, `encoder_multipass`, `encoder_rc`, `encoder_lookahead_level`, `encoder_split_encode_mode` |
| **Encoder Flags** | `encoder_spatial_aq`, `encoder_temporal_aq`, `encoder_strict_gop`, `encoder_no_scenecut`, `encoder_weighted_pred` |
| **FPS & Scale** | `fps_option`, `custom_fps`, `fps_mode`, `video_format_option`, `custom_video_width`, `interpolation_algo` |
| **CUDA** | `cuda_output_format` |
| **Trimming** | `trim_start`, `trim_end`, `trim_streamcopy`, `precise_trim` |
| **Additional** | `additional_options`, `additional_filter_options`, `additional_audio_filter_options` |
| **Presets** | `selected_preset`, `custom_preset_selected` |
| **Batch** | `batch_output_folder`, `batch_change_container`, `batch_output_container` |
| **Meta** | `version` |

---

## 5. Phased Implementation Plan

### Phase 0: Environment Setup & Project Initialization ✅ DONE

- [x] Rust toolchain installed (rustc 1.99.0, cargo 1.99.0)
- [x] Cargo project initialized (`Cargo.toml`, `build.rs`)
- [x] Core dependencies added: `eframe 0.36`, `egui 0.36`, `serde`, `serde_json`, `winresource` (upgraded from 0.29 / `winres` on 2026-10-11)
- [x] Minimal window running with `wgpu` backend
- [x] Window icon embedded via `winresource` (`nff.ico`)
- [x] Release profile configured (opt-level z, LTO, strip)

### Phase 1: Design System & UI Theme ✅ DONE (visual parity with 1.8.4)

Completed:
- [x] Roboto Regular embedded (only face used; Convert and the drop overlay use it at 14/20px) (`assets/fonts/`, Apache 2.0, the same font CustomTkinter ships), 13px body like `CTkFont`
- [x] Palette = `nvencFFX.py` colors + CTk dark-theme defaults (`#DCE4EE` labels, `#565B5E` entry border, `#949A9F` checkbox/radio border, `gray41` scrollbar)
- [x] Both egui theme slots styled, so the OS light/dark setting cannot swap the look
- [x] Window 820×700, fixed width and resizable height, as in the original
- [x] Pixel grid of the original reproduced (`app.rs::grid`): label col 41px, entry col 257px (350 wide), button col 617px (140 wide), 38px rows, 736px section frames
- [x] CTk look-alike widgets (`widgets.rs`): `button`/`bold_button`, `entry` (incl. disabled grey state), `checkbox`, `radio`/`radio_value`, `option_menu` (dark field + green arrow, dropdown with green hover), `text_checkbox`, `trim_slider`, `reset_button`, `section_frame`, `file_drag_drop_overlay`
- [x] Layout helpers `row` / `cell` / `sub_rect` / `place` for absolute placement on that grid

- [x] `rfd` file dialogs on Browse / Save As / FFmpeg (parented to the main window); right-click opens the containing folder
- [x] Modal dialogs replacing Tk `messagebox` / `simpledialog` (`ui/dialogs.rs`: info, warning, error, yes/no, text prompt)
- [x] Self-drawn title bar for secondary windows (`dialogs::title_bar`, undecorated viewport); the main window keeps the native dark title bar for Aero Snap and native resizing

- [x] Context menu widget (done in Phase 5): Cut / Copy / Paste / Delete / Select All on text fields; right-click on Convert pauses; the log's Copy comes with the log inspector (Phase 8)

### Phase 2: Complete Main UI Layout ✅ DONE

Every control of the 1.8.4 main window is on screen and wired to the original's logic, apart from those listed under Remaining below. The UI now binds to one `config::Settings` value (the preset / `nff_settings.json` model), so presets and the command builder read the same state.

- [x] **Quick filter buttons** (`core/filters.rs`): append-once `-vf`/`-af` chains, speed x2 / x0.5 (with `atempo` + AAC fallback), Stereo out (`-ac 2`), Clear
- [x] **Trim / Auto / Default / Save / Save As / Delete**: Trim validates `HH:MM:SS` against the duration and the other field (also on focus loss / Enter) and writes `-ss … -to …`; Auto sets every "auto"-capable encoder menu to auto and clears the flags; Default and the FHD/HD radios apply `_apply_preset`; preset files are saved in the original key order with 4-space indent and merged on load like `_apply_settings_dict`
- [x] **Trim slider**: handles map to `trim_start`/`trim_end` via the ffprobe duration (1 s minimum gap, end stops 0.1 s before the last frame); reset restores the full range; a new input sets `trim_end` to its duration
- [x] **Output auto-naming**: `<stem>_<codec>_custom.<ext>` in `last_output_dir` (if it exists) or next to the input, container kept for mp4/mkv/mov/ts/mts/m2ts/webm; a codec switch renames only auto-named outputs
- [x] **Estimated file size**: `(video + audio kbps) × duration / 8 / 1024` from a background, cached ffprobe run (`core/probe.rs`, `core/estimate.rs`)
- [x] **Window height follows the sections** (`_update_window_size`): opening or closing a section sets the height to 700 + what the open sections add, capped at 1.8.4's `maxsize` (900 / 800 / 650 px at <125% / ≥125% / ≥200% scaling); only on a toggle, so a height dragged by hand stays until then
- [x] **Output line + progress bar** under the status row (parity with 1.8.4's `ffmpeg_output` label and hidden progress frame; the full log inspector stays in Phase 8)
- [x] **Custom command mode**: Output opens an editable preview window (Copy / Apply / Reset / Close); Apply validates `-i` + output file; right-click on Output copies the command with spaced paths quoted
- [x] Parity fixes: H.264 hides Lookahead Level (as the original), CUDA Output Format is disabled unless HW Accel = cuda, tune = lossless turns AQ off, `Send to nvencFFX` argument is loaded at startup, ffmpeg / ffprobe auto-detected (exe folder, then `PATH`)

Remaining (moved to later phases):
- [ ] **Streams** button (stream map window) and the `?` / Help buttons → Phase 8
- [ ] Thumbnail preview while dragging the trim handles → Phase 8
- [x] Play Input / Play Output / Play 10s Preview / Convert actions (Phase 4)
- [ ] Batch / Screen Record actions → Phases 6, 7

Deliberate differences from 1.8.4:
- Loading a preset no longer forces the profile back to `main` (the original's `_update_codec_settings` overwrote e.g. `main10` from a preset); invalid values for the current codec are still reset.
- The main scroll bar is shown only when the content does not fit (CTkScrollableFrame always showed it, even with nothing to scroll); the layout does not move when it appears.
- The auto height uses the measured height a section adds (the original added `winfo_reqheight() + 10`, 10 px short for sections with `pady=10`) and also stops 80 px short of the monitor height, so the window never extends under the taskbar on small screens.
- The command preview quotes arguments with spaces and its parser removes double quotes, so `-metadata title="a b"` style arguments round-trip (Python's non-POSIX `shlex` kept the quote characters).
- The Input / Output / FFmpeg Path fields, the quality field + Constant QP checkbox and the codec radios start at x = 120 instead of 226 (the original's empty gap after the labels came from CTk grid column sizing); the path fields widen from 350 to 478 px up to the buttons. The Auto / Default buttons on the encoder section header stay at 226.
- The right-hand buttons (Browse … Help) move from x = 586 to 608 and the section frames widen from 736 to 738 px, so everything ends 41 px from the right window edge, the same as the labels on the left (1.8.4 left 63 px on the right).

### Phase 3: Configuration Persistence & Presets ✅ DONE

1. ✅ `config::Settings` holds all 46 keys of `_get_current_settings` (same order); `apply_json` merges like `_apply_settings_dict` (missing / empty keys keep the current value)
2. ✅ Missing keys in old files are handled by the merge
3. ✅ `nff_settings.json` next to the exe is loaded at startup (after ffmpeg auto-detection, so a saved path that still exists wins; a BOM is skipped) and saved 1 s after the last change (the original's `after(1000)` debounce) and on exit; a saved custom preset whose file is gone resets to "none", and the preset indicator shows the restored selection
4. ✅ (done in Phase 2) Preset loading from `presets/*.json`:
   - Scan preset directory at startup, populate preset list
   - Save/Load/Delete presets
   - Preset files use the same field schema as settings (minus paths)
5. ✅ `last_input_dir` / `last_output_dir` persist with the rest and seed Browse / Save As / auto-naming (a folder that no longer exists falls back to the working directory / the input's folder)
6. ✅ `version`: the stored value is ignored on load and every save writes this build's version, as the original does; the schema is unchanged since 1.x, so a future migration step goes into `Settings::load`

### Phase 4: FFmpeg Core Engine & Process Runner ✅ DONE

1. ✅ **Command Line Builder** (`core/command.rs`), ported in Phase 2 with tests against the original's output: encoding / stream copy / custom command modes, the full NVENC parameter matrix, CQP vs VBR/CBR with maxrate/bufsize, `-vf`/`-af` chains, stream mapping + `mov_text`, fast vs precise trim, `:v` → `:v:0` retargeting, hwaccel, `-filter_complex` detection. Phase 4 adds `preview_clip` (the two commands of Play 10s Preview: 10 s stream copy from the middle, then the encode with `-ss`/`-to` removed and `-map 0` inserted for custom commands).

2. ✅ **ffprobe Integration** (`core/probe.rs`):
   - ffprobe next to the ffmpeg path, else beside the exe / on `PATH`
   - Durations: background probes, cached per file (`DurationCache`)
   - `media_info`: `ffprobe -hide_banner <file>` (stderr), `Metadata:` / `Chapters:` blocks removed, 10 s timeout; `InfoCache` keeps the 50 most recently used results
   - Shown as the Input File hover tooltip (monospace, 600 px wide), probed lazily on first hover
   - JSON mode (`-show_streams -show_format`) is only needed by the Streams window → Phase 8

3. ✅ **Process Runner** (`core/process.rs`, `core/progress.rs`):
   - `CREATE_NO_WINDOW` (stable Rust has no `STARTUPINFO` access; console apps need nothing else to stay hidden)
   - stdout and stderr share one `std::io::pipe` (like `stderr=STDOUT`); a worker thread splits on `
` and `
` and sends lines over `mpsc`, waking the UI with `request_repaint`
   - Several commands run in sequence, each only after the previous one succeeded (preview = copy + encode)
   - Cancel terminates the process; `stop` also waits for it to exit (used on close, so temp files can be deleted)
   - `time=` parsing accepts every ffmpeg time syntax (`HH:MM:SS.ms`, `MM:SS`, `12.5`, `1500ms`, negative values, `N/A`)

4. ✅ **Process Tree Kill** (`win32/process_tree.rs`): Toolhelp32 snapshot of the children of our process named `ffmpeg.exe` / `ffprobe.exe` / the configured ffmpeg file name, terminated on window close (after the running task is stopped)

5. ✅ **Size Estimation** (`core/estimate.rs`), done in Phase 2

6. ✅ **App wiring**:
   - Convert ↔ Cancel (red); status / output line / progress bar with `0%` then one decimal like the original; completion beep (`MessageBeep(MB_ICONASTERISK)`), "Cancelled" info box, error box with exit code (unsigned, as Python showed it) and the last output line, "ffmpeg.exe not found" box
   - An applied custom command runs as it is (`_build_ffmpeg_command(preview=False)`)
   - Play 10s Preview ↔ Cancel Preview; disabled (grey) in Copy mode, but a running preview stays cancellable; temp clips in `%TEMP%` deleted before the next preview and on exit; the result opens in the default player
   - Play Input / Play Output open the file with `ShellExecuteW` (`os.startfile`)
   - Convert during a preview and preview during a conversion show the original's "Please wait..." errors

Deliberate differences from 1.8.4:
- Progress is measured against the command's `-ss`/`-to`/`-t` range (capped by the input duration) instead of the full input duration, so trimmed conversions reach 100%.
- The preview's stream copy runs in the background thread; the original ran it synchronously and froze the window meanwhile.
- The process kill on exit matches our own children by parent PID, not every `ffmpeg.exe` started from the same path (which also hit other programs' instances).
- The input tooltip is probed on first hover instead of on every input change.

Pause / resume and the taskbar progress overlay followed in Phase 5.

### Phase 5: Win32 Platform Integration ✅ DONE

All raw `windows-sys` like the rest of `src/win32/`; neither `tray-icon` nor `global-hotkey` is used (`tray-icon` has no balloon notifications, and one hidden window serves both the tray and the hotkeys).

1. ✅ **Taskbar Progress Overlay** (`win32/taskbar.rs`, `TaskbarProgress`):
   - `CoCreateInstance(TaskbarList)` → `ITaskbarList3` through a hand-written vtable (same slots as the original's `_com_vtable_call`), `HrInit`, released on exit
   - `SetProgressValue(hwnd, n, 100)` in 1% steps; `SetProgressState`: green on start / resume, yellow when paused, red after an error (until the next conversion), cleared on success or cancel
   - Driven by the conversion only, as in the original (not by Play 10s Preview); unavailable COM (no Explorer) silently disables it

2. ✅ **Process Pause / Resume** (`win32/suspend.rs`, `Runner::pause` / `resume`):
   - `NtSuspendProcess` / `NtResumeProcess` from `ntdll`
   - Right-click on Convert pauses / resumes (`_on_convert_right_click`); while paused the button reads "Resume" (orange) and a left click resumes instead of cancelling; status "Conversion paused" / "Conversion in progress..."
   - Cancel and exit still terminate a suspended ffmpeg (`TerminateProcess` works on it, so no resume first)

3. ✅ **System Tray** (`win32/tray.rs`, `TrayIcon`):
   - Shown from startup until exit: icon from the exe (resource 1, `IDI_APPLICATION` fallback), tooltip `nvencFFX <version>`
   - Native menu: "Start Recording (Alt + F8)", "Stop Recording (Alt + F9)", separator, "Open nvencFFX", "Exit"; double-click = Open
   - Open restores and focuses the window (`_restore_app`), Exit closes it like the title bar's X (so `on_exit` runs)
   - `Tray::balloon` (`show_balloon`, `NIF_INFO`) is ready for the recording notifications
   - Start / Stop Recording reach the app as commands but do nothing until Phase 7

4. ✅ **Global Hotkeys** (in `win32/tray.rs`):
   - `RegisterHotKey` Alt+F8 / Alt+F9 on the tray's hidden window, which runs on its own thread with its own message loop; commands reach the UI over `mpsc` + `request_repaint`
   - Unregistered, and the icon removed, when the app exits (`Tray` drop joins the thread)

5. ✅ **Clipboard & text context menu** (`ui/widgets.rs`, `CTkContextMenu`):
   - Copy goes through egui (`ctx.copy_text`); eframe always enables egui-winit's `clipboard` feature, i.e. `arboard`. Paste from the menu reads the clipboard with `arboard` directly (same crate, no new code compiled)
   - Right-click in any enabled entry and in the command preview opens Cut / Copy / Paste / Delete / Select All (`SECONDARY_BG`, radius 10, 120×28 items with green hover); Copy without a selection copies the whole text, Cut / Delete need a selection
   - egui moves the cursor on any pointer press, so the selection is saved before a right-click reaches the `TextEdit` and restored after it
   - Ctrl+A / C / V / X on non-Latin keyboard layouts (`_handle_key_press`) need no port: egui-winit falls back to the physical key
   - The log's "Copy" belongs to the log inspector → Phase 8

Also: the per-frame bookkeeping (`update_state`: ffmpeg output, probes, tray commands, autosave) moved from `App::ui` to `App::logic`. eframe 0.36 does not call `ui` while the window is minimized, only `logic`, so before this the taskbar progress, the completion beep and the tray's Open stalled until the window was restored.

Deliberate differences from 1.8.4:
- The hotkeys use `MOD_NOREPEAT`, so holding Alt+F8 does not fire repeatedly.
- The tray window re-adds the icon on Explorer's `TaskbarCreated` broadcast (the original's icon vanished after an Explorer restart), and posts `WM_NULL` after the menu closes (KB135788) so the next right-click opens it reliably.
- Pause / resume use the process handle `std` already holds instead of reopening the PID.
- Hotkeys and tray messages go to a separate hidden window instead of a subclassed main window.
- No minimize-to-tray (the plan's earlier idea): 1.8.4 minimizes to the taskbar and keeps the tray icon visible throughout, as this port does.

### Phase 6: Batch Converter & Task Queue ✅ DONE

`BatchConverterWindow` ported 1:1 in `app/batch.rs` (a child module of `app`, so it drives the main window's state like the original did through `main_app`). The earlier draft of this section listed recursive folder ingestion, per-file pause and a separate overall progress bar; 1.8.4 has none of them, so they were not added.

1. ✅ **Window** (immediate viewport, self-drawn title bar, 600×400 content, resizable with that as the minimum, opened centered on the main window):
   - File list: number, name (cut to 40 characters), status (`Ready` / `Converting` / `Done` / `Failed` / `Failed: <reason>` / `Cancelled`), red × per file
   - `Output:` folder line (batch folder, else the main window's last output folder if it exists, else "Original folder(s)"; cut to 85 characters)
   - Output Folder ("Select Output Folder"), "Change output container" checkbox enabling the mp4 / mkv / mov menu
   - Add Files ("Select Video Files", same filters as Browse), Close, Remove All
   - Drag & drop of files onto the window: video extensions join the list (duplicates ignored), anything else gets the "Unsupported File" warning
   - Closing it during a batch only hides it; the list stays until the app exits (it is not persisted)
   - Resize borders (`dialogs::resize_edges`, `ViewportCommand::BeginResize`), since winit gives undecorated windows none; file dialogs are parented to the batch window through its HWND (`win32::foreground_window`)

2. ✅ **Queue run** (`start_batch_conversion`, `_convert_next_file`, `_run_single_conversion`):
   - With files listed, Convert reads "Batch Convert" and runs them; each file becomes the main window's input, with output `<stem>_<codec>_custom<ext>` in the output folder above (`<ext>` = the chosen container or the input's own), built from the current main-window settings
   - Files run one after another as the regular conversion task: same progress bar and output line, per file; the taskbar button shows the progress of the whole list (`_get_taskbar_progress`)
   - A file whose command cannot be built is marked `Failed: <reason>` and the batch moves on; a non-zero exit is `Failed`
   - End: "Batch conversion completed!", taskbar red if any file failed, else cleared; no completion sound and no message boxes, as in the original
   - The main window's input / output are restored afterwards (only if they were set, as in the original)

3. ✅ **Pause / Cancel**: right click on Convert pauses the whole batch ("Batch conversion paused", orange "Resume"); Cancel ends it at once ("Batch conversion cancelled", current file `Cancelled`, taskbar cleared)

4. ✅ **Settings**: `batch_output_folder`, `batch_change_container`, `batch_output_container` were already in `Settings` (Phase 3) and are edited by the window and autosaved

Deliberate differences from 1.8.4:
- An applied custom command gets each file's input (`-i`) and output (last argument) swapped in, like the preview does. 1.8.4 ran the identical command once per file.
- Removing files during a batch keeps it on track: it goes on with the file after the one converting. In 1.8.4 the index shifted, so the wrong entry got the status and a file could be skipped.
- A batch cannot start while Play 10s Preview runs, and the preview cannot start during a batch ("Please wait until ..." errors as for a single conversion). 1.8.4 let them run side by side.
- The Convert button's text follows the state on every frame. 1.8.4 could show "Batch Convert" during a single conversion after files were added, and adding a file reset the single conversion's pause flag.
- Dropping several unsupported files shows one warning instead of one per file.
- The next file starts right away instead of after 100 ms, and durations are probed in the background instead of blocking the window before every file.

### Phase 7: Screen Recording & WASAPI Audio Capture

1. **Screen Capture** (`capture.rs`):
   - Build FFmpeg command with `-f lavfi -i ddagrab=framerate={fps}` and `-vf setparams=range=limited`
   - Segmented recording: write numbered video segments to temp files (`nvencffx_record_{timestamp}_{pid}_{ns}`)
   - Auto-restart on segment failure (up to 3 attempts)
   - Audio is captured separately and muxed after recording stops
   - Final mux: concatenate video segments + merge WAV audio into output file

2. **WASAPI Audio Loopback** (`audio.rs`):
   - `cpal` with WASAPI loopback device (captures desktop/game audio without virtual cable)
   - Write to temp `.wav` file concurrently with video recording
   - Synchronize start/stop with video segments

3. **Recording UI Integration**:
   - Toggle button: "Screen Record" ↔ "Stop Recording" (green → red)
   - Minimize window 1 second after recording starts
   - Tray icon balloon notifications for recording state changes
   - Title bar update: "Screen is recording now - nvencFFX"

### Phase 8: UI Polish, Tooltips & Help

1. Port descriptive tooltips to all encoder settings via `.on_hover_text(...)` — the Python version has extensive technical tooltips for every NVENC option
2. Real-time FFmpeg log inspector with search, auto-scroll, word-wrap toggle, and "Copy" context menu
3. Integrated Help dialog (port `nff-help.txt` — 20 KB of documentation)
4. Integrated About dialog (port `nff-about.txt` — version, author, license)
5. File association: "Send to nvencFFX" shell integration (equivalent of `Send to nvencFFX.bat`)
6. ffprobe tooltip on input file hover (shows codec, resolution, duration, bitrate)

### Phase 9: Binary Optimization & Release Packaging

1. Release profile already configured in `Cargo.toml` (opt-level z, LTO, strip, panic=abort)
2. `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` — already in `main.rs`
3. Embed version metadata, copyright, and `nff.ico` via `winresource` — already in `build.rs`
4. Post-build: optional UPX compression for ~4–6 MB final binary
5. Output: **Single executable (~12–15 MB)** with native DX12/Vulkan hardware acceleration
6. Test: verify nff_settings.json and presets/*.json backward compatibility with Python version files

---

## 6. Current Progress Tracker

| Phase | Status | Key Files | LOC |
|---|---|---|---|
| Phase 0 — Setup | ✅ Done | `Cargo.toml`, `build.rs`, `main.rs` | ~50 |
| Phase 1 — Theme & Widgets | ✅ Done (context menu added in Phase 5) | `ui/theme.rs`, `ui/widgets.rs`, `ui/dialogs.rs` | ~950 |
| Phase 2 — Main UI Layout | ✅ Done (Streams / help windows moved to Phase 8) | `app.rs`, `core/filters.rs`, `core/timecode.rs` | ~1,700 |
| Phase 3 — Settings & Presets | ✅ Done | `config/settings.rs`, `config/presets.rs` | ~575 |
| Phase 4 — FFmpeg Core | ✅ Done | `core/command.rs`, `core/probe.rs`, `core/process.rs`, `core/progress.rs`, `win32/process_tree.rs` | ~1,300 |
| Phase 5 — Win32 Integration | ✅ Done (recording commands wait for Phase 7) | `win32/taskbar.rs`, `win32/suspend.rs`, `win32/tray.rs`, `ui/widgets.rs` (context menu) | ~800 |
| Phase 6 — Batch Converter | ✅ Done | `app/batch.rs`, `ui/dialogs.rs` (resize edges), `win32/mod.rs` (dialog parent) | ~750 |
| Phase 7 — Screen Recording | ⬜ Not started | — | — |
| Phase 8 — Polish & Help | ⬜ Not started | — | — |
| Phase 9 — Release | 🟡 Config done | `Cargo.toml`, `build.rs` | — |

**Current Rust codebase**: ~7,000 LOC across 23 files (57 tests).

---

## 7. Critical Risks & Notes

1. **egui file dialogs**: egui has no built-in file dialog. The `rfd` crate is **required** for Browse/Save As/Output Folder buttons — this is a hard dependency, not optional.
2. ~~**Audio options gap**~~: resolved; all 9 audio modes are in the UI and the command builder.
3. ~~**Custom command mode**~~: resolved; editing / applying (Phase 2) and running the applied command (Phase 4) are done.
4. **Settings backward compat**: The Rust version must read and write `nff_settings.json` that the Python version can also read. Test with real user settings files.
5. ~~**Process tree kill**~~: resolved in Phase 4 (`win32/process_tree.rs`); reuse it for recording stop in Phase 7.
6. **Screen recording complexity**: The segmented recording system with WASAPI audio muxing is the most complex feature (~650 LOC in Python). Consider deferring to a later release if needed.
7. **Work while minimized**: eframe 0.36 calls only `App::logic` (not `App::ui`) while the window is minimized or hidden. Anything that must keep running then (ffmpeg progress, tray / hotkey commands, and in Phase 7 the recording state) belongs in `update_state`, which runs from `logic`.
8. **Tray / hotkey commands for recording**: Alt+F8 / Alt+F9 and the tray's Start / Stop Recording are delivered but ignored until Phase 7 wires them to the recorder; the `Tray::balloon` API is unused until then.
9. **eframe 0.36 + edition 2024**: eframe 0.36 needs rustc ≥ 1.95 (`rust-version` in `Cargo.toml`). It brings hinted text (skrifa + vello_cpu); keep `PresentMode::AutoNoVsync` in `main.rs`, since eframe still repaints on every window move and vsync makes dragging stutter on Windows.
10. **Batch window, manual checks**: the queue run is covered by tests and was checked with the real ffmpeg / NVENC. Resizing by the self-drawn borders, the native file dialogs being owned by the batch window, and drag & drop onto it need a manual run; the dialog parent is the foreground window at click time, which is the batch window unless focus moved meanwhile.
