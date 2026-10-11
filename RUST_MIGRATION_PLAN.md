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
│   ├── app/record.rs         # Screen Record button, title, minimize, balloons (child module of app)
│   ├── app/help.rs           # Help window (Manual / About / License) and the `?` ffmpeg help windows
│   ├── app/streams.rs        # Streams window (stream checkboxes → -map options)
│   ├── app/thumbnail.rs      # Frame preview above the trim slider
│   ├── app/vmaf.rs           # VMAF analysis (right click on Play Output File)
│   ├── ui/                   # egui interface components
│   │   ├── mod.rs
│   │   ├── theme.rs          # Color palette (#4FB62F accent, dark mode, styles)
│   │   ├── widgets.rs        # Custom widgets (TextCheckbox, chip_button, radio_button, tooltip helpers)
│   │   ├── main_view.rs      # Main layout (Input/Output, codec, quality, actions)
│   │   ├── encoder_section.rs  # Advanced NVENC settings (preset, tune, multipass, AQ, profile, level, tier)
│   │   ├── fps_scale_section.rs # FPS mode, scaling, interpolation
│   │   ├── audio_section.rs  # Audio codec selection (Copy, AAC, Opus, Custom, Disable)
│   │   ├── trim_section.rs   # Trimming controls (timeline slider, -ss/-to, streamcopy, precise)
│   │   └── presets_section.rs # Preset selector with save/load/delete
│   ├── core/                 # FFmpeg business logic
│   │   ├── mod.rs
│   │   ├── command.rs        # FFmpeg CLI argument builder (encoding, streamcopy, custom command modes)
│   │   ├── probe.rs          # ffprobe runner (media info, duration, codec detection, LRU cache)
│   │   ├── process.rs        # Process management (spawn, stderr reader, cancel, kill tree via toolhelp32)
│   │   ├── progress.rs       # Stream parser for progress (time=, fps=, speed=, size=, bitrate=, percentage)
│   │   ├── estimate.rs       # Output file size estimation (ffprobe duration × bitrate)
│   │   ├── streams.rs        # Stream list from ffprobe, -map options
│   │   ├── thumbnail.rs      # Trim slider frame grabs (CUDA nv12 / p010le, CPU), raw RGBA
│   │   └── vmaf.rs           # libvmaf command and score parsing
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
│       ├── mod.rs            # Session worker: segments, auto-restart, stop via `q`, join + mux
│       ├── capture.rs        # ddagrab command line, temp file names, concat list, mux command
│       └── audio.rs          # Desktop audio: WASAPI loopback (raw COM) -> 16-bit WAV
```

---

## 3. Complete Dependency List

```toml
[dependencies]
# GUI framework
eframe = { version = "0.36", default-features = false, features = ["wgpu_no_default_features", "default_fonts"] }
egui = "0.36"
# Only the Windows backends (Phase 9): eframe's `wgpu` feature adds GLES, Metal and web
wgpu = { version = "30", default-features = false, features = ["std", "parking_lot", "dx12", "vulkan", "wgsl"] }

# Serialization (settings, presets)
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.154"

# File dialogs (Browse, Save As, Output Folder)
rfd = "0.17"

# Launch external files (Play Input/Output, Open output folder)
open = "5.4"

# Clipboard: reading for the text menu's Paste (eframe already uses it for copying)
arboard = { version = "3.6", default-features = false }

# Win32 API (ITaskbarList3, NtSuspendProcess, tray icon + RegisterHotKey, CreateToolhelp32Snapshot,
# WASAPI loopback). Tray and hotkeys are raw Win32 too (Phase 5): `tray-icon` has no balloons,
# `global-hotkey` is not needed. WASAPI is raw COM too (Phase 7): `cpal` would pull in the
# `windows` crate for one capture stream.
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_Media_Audio",
    "Win32_System_Com",
    "Win32_System_Diagnostics_Debug",
    "Win32_System_Diagnostics_ToolHelp",
    "Win32_System_LibraryLoader",
    "Win32_System_Performance",
    "Win32_System_SystemInformation",
    "Win32_System_Threading",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
] }

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

- [x] Context menu widget (done in Phase 5): Cut / Copy / Paste / Delete / Select All on text fields; right-click on Convert pauses

### Phase 2: Complete Main UI Layout ✅ DONE

Every control of the 1.8.4 main window is on screen and wired to the original's logic, apart from those listed under Remaining below. The UI now binds to one `config::Settings` value (the preset / `nff_settings.json` model), so presets and the command builder read the same state.

- [x] **Quick filter buttons** (`core/filters.rs`): append-once `-vf`/`-af` chains, speed x2 / x0.5 (with `atempo` + AAC fallback), Stereo out (`-ac 2`), Clear
- [x] **Trim / Auto / Default / Save / Save As / Delete**: Trim validates `HH:MM:SS` against the duration and the other field (also on focus loss / Enter) and writes `-ss … -to …`; Auto sets every "auto"-capable encoder menu to auto and clears the flags; Default and the FHD/HD radios apply `_apply_preset`; preset files are saved in the original key order with 4-space indent and merged on load like `_apply_settings_dict`
- [x] **Trim slider**: handles map to `trim_start`/`trim_end` via the ffprobe duration (1 s minimum gap, end stops 0.1 s before the last frame); reset restores the full range; a new input sets `trim_end` to its duration
- [x] **Output auto-naming**: `<stem>_<codec>_custom.<ext>` in `last_output_dir` (if it exists) or next to the input, container kept for mp4/mkv/mov/ts/mts/m2ts/webm; a codec switch renames only auto-named outputs
- [x] **Estimated file size**: `(video + audio kbps) × duration / 8 / 1024` from a background, cached ffprobe run (`core/probe.rs`, `core/estimate.rs`)
- [x] **Window height follows the sections** (`_update_window_size`): opening or closing a section sets the height to 700 + what the open sections add, capped at 1.8.4's `maxsize` (900 / 800 / 650 px at <125% / ≥125% / ≥200% scaling); only on a toggle, so a height dragged by hand stays until then
- [x] **Output line + progress bar** under the status row (parity with 1.8.4's `ffmpeg_output` label and hidden progress frame)
- [x] **Custom command mode**: Output opens an editable preview window (Copy / Apply / Reset / Close); Apply validates `-i` + output file; right-click on Output copies the command with spaced paths quoted
- [x] Parity fixes: H.264 hides Lookahead Level (as the original), CUDA Output Format is disabled unless HW Accel = cuda, tune = lossless turns AQ off, `Send to nvencFFX` argument is loaded at startup, ffmpeg / ffprobe auto-detected (exe folder, then `PATH`)

Remaining (moved to later phases):
- [x] **Streams** button (stream map window) and the `?` / Help buttons (Phase 8)
- [x] Thumbnail preview while dragging the trim handles (Phase 8)
- [x] Play Input / Play Output / Play 10s Preview / Convert actions (Phase 4)
- [x] Batch / Screen Record actions (Phases 6, 7)

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
   - No JSON mode (`-show_streams -show_format`) needed: the Streams window (Phase 8) parses the `-hide_banner` overview like the original

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
   - Start / Stop Recording reach the app as commands (wired to the recorder in Phase 7)

4. ✅ **Global Hotkeys** (in `win32/tray.rs`):
   - `RegisterHotKey` Alt+F8 / Alt+F9 on the tray's hidden window, which runs on its own thread with its own message loop; commands reach the UI over `mpsc` + `request_repaint`
   - Unregistered, and the icon removed, when the app exits (`Tray` drop joins the thread)

5. ✅ **Clipboard & text context menu** (`ui/widgets.rs`, `CTkContextMenu`):
   - Copy goes through egui (`ctx.copy_text`); eframe always enables egui-winit's `clipboard` feature, i.e. `arboard`. Paste from the menu reads the clipboard with `arboard` directly (same crate, no new code compiled)
   - Right-click in any enabled entry and in the command preview opens Cut / Copy / Paste / Delete / Select All (`SECONDARY_BG`, radius 10, 120×28 items with green hover); Copy without a selection copies the whole text, Cut / Delete need a selection
   - egui moves the cursor on any pointer press, so the selection is saved before a right-click reaches the `TextEdit` and restored after it
   - Ctrl+A / C / V / X on non-Latin keyboard layouts (`_handle_key_press`) need no port: egui-winit falls back to the physical key

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

### Phase 7: Screen Recording & WASAPI Audio Capture ✅ DONE

The recording code of 1.8.4 (`_start_recording` … `_monitor_recording`, `_record_audio_loop`) ported in `src/recording/` and `app/record.rs`. One worker thread per recording (`recording::Session`) runs the whole session and reports `Event`s over `mpsc` + `request_repaint`, like the conversion runner; the UI only reacts to them in `update_state`, so the recording keeps going while the window is minimized.

1. ✅ **Screen Capture** (`recording/capture.rs`, `recording/mod.rs`):
   - Command as in 1.8.4: `-y -thread_queue_size 4096 -use_wallclock_as_timestamps 1 -f lavfi -i ddagrab=framerate=<fps> -vf setparams=range=limited -c:v <codec>_nvenc [-preset:v] -rc:v constqp -qp:v <q>` (or `-rc:v <vbr|cbr> -b:v <k>`), the Add FF Options while that section is open, `-fps_mode <mode> -an`; FPS "Source" (or empty) records at 60
   - Output: the Output File field, else `screen_record-<dd_mm_yy-HH_MM_SS>.mp4` on the Desktop
   - Segments `%TEMP%\nvencffx_record_<date>_<pid>_<ns>_part<n><ext>`, started 2 s after the click; an ffmpeg that ends on its own (ddagrab stops on a display mode change) starts the next segment after 1 s, at most 3 times per recording, then the recording is given up and what was captured is saved ("Saved partial recording to …")
   - Stop writes `q` to ffmpeg's stdin (killed after 5 s if it does not finish); the segments are joined with the concat demuxer (`-c copy`), one segment is used as it is
   - Final mux: `-i <video> -i <wav> -c:v copy <Audio Settings> -map 0:v:0 -map 1:a:0`, or a move if there is no audio; temp files are deleted only after the output is saved, otherwise they stay and the message says where
   - Messages, status texts and the restart / error wording are the original's ("Display changed. Restarting recording (n/3)...", "Joining recording segments...", "Error finishing recording", …)

2. ✅ **WASAPI Audio Loopback** (`recording/audio.rs`):
   - Raw COM like the taskbar overlay: `IMMDeviceEnumerator` → default render endpoint → `IAudioClient` (shared mode, `AUDCLNT_STREAMFLAGS_LOOPBACK`, the device's mix format) → `IAudioCaptureClient`, polled every 10 ms on its own thread
   - Written as 16-bit PCM WAV (as `paInt16` did) with the mix format's channels and rate; float / 16 / 24 / 32-bit mix formats are converted
   - Recorded once per session, from the first segment's start to the stop (also across restarts); "Disable" in Audio Settings records none
   - Errors (no playback device, unsupported format) only leave the recording without audio, as before

3. ✅ **Recording UI Integration** (`app/record.rs`):
   - Screen Record (grey) ↔ Stop Recording (red); disabled while the recording is saved; "Please wait until the previous recording is saved." on a start meanwhile
   - Title "Screen is recording now - nvencFFX" while recording; the window minimizes 1 s after the start (not if stopped before) and is restored if ffmpeg cannot start or the restarts run out
   - Tray balloons "Recording Started" / "Recording Saved", completion beep
   - Alt+F8 / Alt+F9 and the tray's Start / Stop Recording drive it
   - Closing the app during a recording stops it and waits (up to 5 min) until it is saved

Deliberate differences from 1.8.4:
- Alt+F8 / the tray's Start do nothing while a recording runs. 1.8.4 started a new session over the running one, orphaning its ffmpeg and segments.
- Alt+F9 / the tray's Stop do nothing without a recording. 1.8.4 set "Screen recording cancelled" and reset the title, also over a running conversion's status.
- The audio settings are read when the recording starts (1.8.4 read them when it ended), so an invalid custom audio bitrate is reported before recording ("Failed to start screen recording: Custom audio bitrate must be a number.") instead of failing the save afterwards.
- Stretches where nothing plays are written as silence. WASAPI loopback delivers no packets then; packets are placed by their timestamps, so the audio keeps its place against the video (gaps over 100 ms, also before the first sound and up to the stop).
- The default output goes to the shell's Desktop folder (also when it was moved, e.g. into OneDrive) instead of `~\Desktop`.
- The title changes only after the FFmpeg path check (1.8.4 left "Screen is recording now" in the title when the path was empty).
- Closing the app during a recording saves it first. 1.8.4 started the save on a daemon thread and quit, so the recording was lost (the segments stayed in `%TEMP%`).
- An ffmpeg that cannot be started for the join / mux gives the same "Failed to join … / Failed to mux …" message as a failed run (1.8.4 showed the Python exception text).

### Phase 8: Help, Streams, Frame Preview & VMAF ✅ DONE

The rest of the 1.8.4 main window: the Help and `?` windows, the Streams window, the frame preview of the trim slider and the VMAF analysis. The earlier draft of this section listed technical tooltips on every encoder option and a log inspector with search, auto-scroll, word wrap and Copy; 1.8.4 has neither (it has five right-click hints and the one-line output under the status), so they were not added. The input file's ffprobe tooltip was already done in Phase 4, and the "Send to" argument in Phase 2.

1. ✅ **Help window** (`app/help.rs`, `_show_main_help`): "nvencFFX Help", 800×700, Manual / About / License tabs (106 px, the open one green); `nff-help.txt` with its `#` lines as green headings without the `#`, `nff-about.txt`, `nff-license.txt`; monospace 14 px, word-wrapped, selectable; Close
2. ✅ **`?` windows** (`_show_help_window`, `_fetch_help_info`): "NVENC Encoder Options" (`ffmpeg -h encoder=<codec>_nvenc`) on Add FF Options, "FFmpeg Video Filters" (`ffmpeg -filters`) on Add Video Filters; "Loading help information..." until ffmpeg (run in the background) answers, stdout or else stderr, "Error retrieving help information:" if it cannot start
3. ✅ **Streams window** (`app/streams.rs`, `core/streams.rs`, `_open_map_window`, `_create_stream_map_window`):
   - "Stream Selection - <file>", 600×600 (min 600×550), centered on the main window; the original's warnings for no input / a missing file / no ffprobe
   - `ffprobe -hide_banner` in the background ("Analyzing file, please wait..."), parsed like the original's `Stream #0:N...: Video|Audio|Subtitle: ...` regex; Video / Audio / Subtitle / Pictures (cover art) sections, map codes `0:v:N` / `0:a:N` / `0:s:N` by position within the type
   - Apply / Check All / Uncheck All enabled once loaded; Apply drops the old `-map` / `-ignore_unknown` from Add FF Options and adds `-ignore_unknown -map ...` for the checked streams (nothing if all are checked; "At least one stream must be selected." if none)
   - The selection is kept per input file for the session and restored on reopening (also after Close)
4. ✅ **Frame preview on the trim slider** (`app/thumbnail.rs`, `core/thumbnail.rs`, `_show_thumbnail_preview`): pressing the slider shows the frame at the pointer's time at once, dragging a handle refreshes it 200 ms after the handle rests, releasing hides it; tried as in the original: CUDA decode + `scale_cuda` with an 8-bit (`nv12`), then 10-bit (`p010le`) download, then the CPU, 5 s each; "Preview unavailable" if all fail
5. ✅ **VMAF** (`app/vmaf.rs`, `core/vmaf.rs`, `_on_vmaf_right_click`, `_run_vmaf_analysis`, `_execute_vmaf`): right click on Play Output File (hint "Right-click to run VMAF analysis"); both sizes from ffprobe, the output scaled to the input's size with the Interpolation Algo if they differ; `-t 30` of both through `libvmaf=n_threads=<cores>:n_subsample=10`; status "VMAF Analysis in progress...", the output lines under it, then "VMAF score: 95.13" / "VMAF score (with scaling): ..." with the completion beep; a second right click cancels ("VMAF analysis cancelled"); runs alongside a conversion, as in the original
6. ✅ Tooltip delay 0.3 s (`CTkToolTip(delay=0.3)`) for the right-click hints
7. ✅ Consolas 14 px (`font=("Consolas", 14)`) in the input tooltip, the command preview and the help windows, as in 1.8.4: read at startup from `%WINDIR%\Fonts\consola.ttf` (Microsoft's font cannot be embedded), egui's Hack as the fallback; until now these used Hack

Also: `probe::output_with_timeout` (a child with captured output, killed after a timeout) now backs the input tooltip's 10 s limit too; before, a hung ffprobe was left running. The process tree test killed every `ffmpeg.exe` child of the test binary, which broke other tests running ffmpeg in parallel; it now uses a name nothing runs under. Radio buttons were 25 px (28 px checked) instead of CTk's 22 px: epaint 0.36 strokes circles outside the radius, so the ring now sits inside it (checked: 6 px ring around a 10 px hole).

Deliberate differences from 1.8.4:
- The help texts are embedded in the exe instead of read from files next to it, so the single exe needs no text files (and the "File not found" case is gone).
- Clicking Help or a `?` again brings its open window to the front (1.8.4 ignored the click; its Streams window already did this).
- Help and `?` windows open centered on the main window (1.8.4 left the position to the window manager) and have a 400×300 minimum size.
- The `?` windows scroll horizontally as well; 1.8.4's label cut off lines wider than the window.
- The frame preview is grabbed in the background and drawn inside the main window, kept within it (below the slider if there is no room above); 1.8.4 ran ffmpeg on the UI thread (the window froze for each frame) and at >100% display scaling moved the popup to the screen's top-left corner.
- ffmpeg delivers the frame as raw RGBA instead of an MJPEG decoded with Pillow, so no image decoder is needed.
- The preview keeps the video's aspect ratio inside the 352×198 box; 1.8.4 stretched every frame to 16:9.
- A press on the slider (not on a handle) picks the nearer handle, as since Phase 2, so the preview follows that drag; 1.8.4 showed the frame of the click and moved nothing.
- The input file tooltip appears after 0.3 s like the other hints (1.8.4: 0.5 s); egui has one tooltip delay.
- "Preview unavailable" appears in the box above the slider; 1.8.4 put it in a popup at the screen's top-left corner if the very first frame failed.

### Phase 9: Binary Optimization & Release Packaging ✅ DONE

1. ✅ **Release profile** (`Cargo.toml`, since Phase 0): `opt-level = "z"`, LTO, one codegen unit, `panic = "abort"`, stripped
2. ✅ **No console**: `windows_subsystem = "windows"` in release builds (`main.rs`), also when Send to starts it with a file argument
3. ✅ **wgpu backends**: eframe's `wgpu` feature compiled in every wgpu backend (DX12, Vulkan, GLES with EGL / ANGLE loading, Metal, WebGPU / WebGL). Now eframe's `wgpu_no_default_features` plus `wgpu` with `dx12`, `vulkan`, `wgsl` only: 8.77 → 8.48 MB. Adapter choice unchanged (egui-wgpu requests `PRIMARY | GL` at high performance, wgpu enumerates Vulkan before DX12); DX12's WARP software adapter stays the last resort. Checked: starts on the default, `WGPU_BACKEND=dx12` and `=vulkan`; `=gl` now exits with 1.
   Kept: `default_fonts` (about 1 MB). egui's Hack / Ubuntu / Noto Emoji / emoji icon fonts are the glyph fallback behind Roboto and Consolas (symbols, emoji in file names).
4. ✅ **Version resource and manifest** (`build.rs`, `assets/nvencFFX.manifest`). The earlier note "already in `build.rs`" was wrong: the exe had only the icon.
   - Version info as the 1.8.4 Nuitka build (`nff-nuitka-build.txt`): ProductName / FileDescription "nvencFFX", CompanyName "Deepseek Enjoyer", LegalCopyright "Copyright (C) 2026 hadouken", OriginalFilename "nvencFFX.exe", InternalName "nvencFFX"; FileVersion / ProductVersion from `package.version`
   - Manifest: `asInvoker`, Windows 10 / 11 `supportedOS`, PerMonitorV2 DPI awareness (what winit sets at runtime anyway, now from process start), `longPathAware`, Common Controls 6
   - A resource compile error is now a visible cargo warning (it went to stderr, which cargo hides from build scripts)
5. ✅ **Packaging** (`build-release.ps1`): `cargo build --release`, then `dist\nvencFFX-<version>\` with `nvencFFX.exe`, `presets\*.json`, `LICENSE.txt`, and `dist\nvencFFX-<version>.zip`. The exe gets its mixed-case name there; the crate stays `nvencffx` (a mixed-case bin name trips rustc's crate-name lint). `-Upx [-UpxPath <upx.exe>]` packs the exe.
   Measured (RTX 3060, Windows 10):

   | | exe | zip | window shown after |
   |---|---|---|---|
   | plain | 8.48 MB | 3.71 MB | ~40 ms |
   | UPX `--best --lzma` | 2.88 MB | 2.74 MB | ~280 ms |

   UPX stays opt-in: zip compression already gets most of its download saving, the packed exe unpacks itself into memory on every start, and UPX-packed files are a common antivirus false positive.
   Working set after start: ~70 MB on Vulkan, ~115 MB with the default adapter choice, ~180 MB on DX12, mostly GPU driver DLLs; section 1's 25–45 MB target is not reached.
6. ✅ **1.8.x settings / preset compatibility**:
   - `config/testdata/nff_settings-1.8.4.json` was written by Python with the original's `json.dump(indent=4, ensure_ascii=False)` to a UTF-8 text file (non-ASCII paths, emoji, quotes, backslashes, tab, ESC, DEL). Loading and saving it gives the same bytes apart from the version stamp (`.gitattributes` keeps git from converting it).
   - Every shipped `presets/*.json` (1.7.x / 1.8.x, subsets of the keys) applies every value it holds
   - Manual: the repo's own 1.8.4 `nff_settings.json` through the packaged exe (start, close) comes back with only `"version"` changed; a file written by the exe reads with Python's `json.load` (46 keys)
7. ✅ **Texts**: README: release download and folder layout, build from source with cargo, `build-release.ps1` (+ UPX), the Send to shortcut straight to `nvencFFX.exe`, Rust badge instead of Python. `nff-about.txt`: 2.0.0, credits egui / eframe, wgpu, rfd, serde, Roboto instead of CustomTkinter / Pillow / CTkToolTip; a test keeps its first line equal to the window title.

Also: settings and preset files are written with CRLF line ends. 1.8.4 opened them in text mode, which writes CRLF on Windows; the port wrote LF since Phase 3. Python read both, but the files now match byte for byte.

Deliberate differences from 1.8.4:
- The release folder holds `nvencFFX.exe`, `presets` and `LICENSE.txt` only. 1.8.4's Nuitka folder had 68+ files (Python runtime, DLLs, `nff.ico` and the `nff-*.txt` texts, which are embedded since Phases 0 and 8).
- Send to points straight at `nvencFFX.exe`; `Send to nvencFFX.bat` (a console that ran `start`) is not needed.

Open before publishing:
- Third-party notices: `nff-license.txt` and the release folder have nvencFFX's and BtbN's licenses only. The exe embeds Roboto (Apache 2.0), egui's default fonts (Hack, Ubuntu Font Licence, Noto Emoji and emoji-icon-font under OFL) and MIT / Apache-2.0 crates, whose licenses ask for their notices in binary distributions (`cargo about` can generate the crate list).
- FFmpeg binaries are added to the release folder by hand, as for 1.8.x; the script does not bundle them.
- `CHANGELOG.md` has no 2.0.0 entry yet.

---

## 6. Current Progress Tracker

| Phase | Status | Key Files | LOC |
|---|---|---|---|
| Phase 0 — Setup | ✅ Done | `Cargo.toml`, `build.rs`, `main.rs` | ~50 |
| Phase 1 — Theme & Widgets | ✅ Done (context menu added in Phase 5) | `ui/theme.rs`, `ui/widgets.rs`, `ui/dialogs.rs` | ~950 |
| Phase 2 — Main UI Layout | ✅ Done (Streams / help windows done in Phase 8) | `app.rs`, `core/filters.rs`, `core/timecode.rs` | ~1,700 |
| Phase 3 — Settings & Presets | ✅ Done | `config/settings.rs`, `config/presets.rs` | ~575 |
| Phase 4 — FFmpeg Core | ✅ Done | `core/command.rs`, `core/probe.rs`, `core/process.rs`, `core/progress.rs`, `win32/process_tree.rs` | ~1,300 |
| Phase 5 — Win32 Integration | ✅ Done (recording commands wired in Phase 7) | `win32/taskbar.rs`, `win32/suspend.rs`, `win32/tray.rs`, `ui/widgets.rs` (context menu) | ~800 |
| Phase 6 — Batch Converter | ✅ Done | `app/batch.rs`, `ui/dialogs.rs` (resize edges), `win32/mod.rs` (dialog parent) | ~750 |
| Phase 7 — Screen Recording | ✅ Done | `recording/mod.rs`, `recording/capture.rs`, `recording/audio.rs`, `app/record.rs`, `win32/mod.rs` (local time, Desktop folder) | ~1,450 |
| Phase 8 — Help, Streams, Preview, VMAF | ✅ Done | `app/help.rs`, `app/streams.rs`, `app/thumbnail.rs`, `app/vmaf.rs`, `core/streams.rs`, `core/thumbnail.rs`, `core/vmaf.rs`, `core/probe.rs` (timeouts, sizes) | ~1,550 |
| Phase 9 — Release | ✅ Done (third-party notices open) | `Cargo.toml`, `build.rs`, `assets/nvencFFX.manifest`, `build-release.ps1`, `config/testdata/` | ~150 |

**Current Rust codebase**: ~10,200 LOC across 34 files (93 tests).

---

## 7. Critical Risks & Notes

1. **egui file dialogs**: egui has no built-in file dialog. The `rfd` crate is **required** for Browse/Save As/Output Folder buttons — this is a hard dependency, not optional.
2. ~~**Audio options gap**~~: resolved; all 9 audio modes are in the UI and the command builder.
3. ~~**Custom command mode**~~: resolved; editing / applying (Phase 2) and running the applied command (Phase 4) are done.
4. **Settings backward compat**: covered since Phase 9: a Python-written settings file round-trips byte for byte (apart from `version`), every shipped preset applies, and a real 1.8.4 `nff_settings.json` went through the packaged exe unchanged. A future schema change must keep 1.8.x able to read the file (it ignores unknown keys, so adding keys is safe; renaming or retyping one is not).
5. ~~**Process tree kill**~~: resolved in Phase 4 (`win32/process_tree.rs`). A recording is stopped with `q` (killed after 5 s) by its own worker; on exit it is saved before the process tree kill runs.
6. **Screen recording, manual checks**: the session (restart limit, stop, join, mux, cleanup) is covered by tests with stand-in commands and with the real ffmpeg (`testsrc`) plus real WASAPI capture, and a full desktop recording (ddagrab + hevc_nvenc + AAC) was checked once through the app layer. Still to try by hand: a restart after a real display mode change (a fullscreen game switching resolution), audio while sound plays, and the minimize / balloons / hotkeys in the running app. A default playback device that changes mid-recording (headphones unplugged) ends the audio there; the WAV stays valid, so the output just has shorter audio.
7. **Work while minimized**: eframe 0.36 calls only `App::logic` (not `App::ui`) while the window is minimized or hidden. Anything that must keep running then (ffmpeg progress, tray / hotkey commands, the recording's events and its delayed minimize) belongs in `update_state`, which runs from `logic`.
8. ~~**Tray / hotkey commands for recording**~~: resolved in Phase 7; Alt+F8 / Alt+F9 and the tray menu drive the recorder, and `Tray::balloon` shows its notifications.
9. **eframe 0.36 + edition 2024**: eframe 0.36 needs rustc ≥ 1.95 (`rust-version` in `Cargo.toml`). It brings hinted text (skrifa + vello_cpu); keep `PresentMode::AutoNoVsync` in `main.rs`, since eframe still repaints on every window move and vsync makes dragging stutter on Windows.
10. **Batch window, manual checks**: the queue run is covered by tests and was checked with the real ffmpeg / NVENC. Resizing by the self-drawn borders, the native file dialogs being owned by the batch window, and drag & drop onto it need a manual run; the dialog parent is the foreground window at click time, which is the batch window unless focus moved meanwhile.
11. **Phase 8, manual checks**: the windows' contents and buttons are covered by UI tests, the frame grab and the VMAF run by tests with the real ffmpeg (testsrc clips). Still to look at in the running app: the help / Streams windows' layout against 1.8.4, the frame preview's position while dragging (also at 125% / 150% scaling and with the window scrolled), and the CUDA → CPU fallback on a file NVDEC cannot decode. `nff-about.txt` was updated for 2.0.0 in Phase 9 (so this branch's copy no longer fits the Python version).
12. **Release, open items**: third-party license notices (Roboto, egui's fonts, the crates) are not shipped yet; the GLES backend is gone, so a machine where neither DX12 (incl. WARP) nor Vulkan works can no longer start the app (not seen on Windows 10 / 11); UPX-packed builds may be flagged by antivirus scanners, which is why `-Upx` is opt-in.
