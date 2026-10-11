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
│   │   ├── batch_view.rs     # Batch Converter window/modal
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
│   │   ├── tray.rs           # System tray icon, notification balloons, and context menu
│   │   ├── hotkeys.rs        # Global hotkeys (Alt+F8 start, Alt+F9 stop recording)
│   │   ├── process_tree.rs   # Kill ffmpeg.exe child tree via CreateToolhelp32Snapshot
│   │   └── clipboard.rs      # Copy-to-clipboard (command string, log text)
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

# Clipboard (Copy Command, Copy Log)
arboard = "3.6"

# Win32 API (ITaskbarList3, NtSuspendProcess, RegisterHotKey, CreateToolhelp32Snapshot)
windows-sys = { version = "0.61", features = [
    "Win32_System_Com",
    "Win32_UI_Shell",
    "Win32_System_Threading",
    "Win32_System_Diagnostics_ToolHelp",
    "Win32_UI_Input_KeyboardAndMouse",
] }

# System tray icon with notifications
tray-icon = "0.26"

# Global hotkeys (Alt+F8, Alt+F9)
global-hotkey = "0.8"

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

Remaining:
- [ ] Context menu widget (right-click on Convert = Pause; right-click on log = Copy)

### Phase 2: Complete Main UI Layout ✅ DONE

Every control of the 1.8.4 main window is on screen and wired to the original's logic, apart from those listed under Remaining below. The UI now binds to one `config::Settings` value (the preset / `nff_settings.json` model), so presets and the command builder read the same state.

- [x] **Quick filter buttons** (`core/filters.rs`): append-once `-vf`/`-af` chains, speed x2 / x0.5 (with `atempo` + AAC fallback), Stereo out (`-ac 2`), Clear
- [x] **Trim / Auto / Default / Save / Save As / Delete**: Trim validates `HH:MM:SS` against the duration and the other field (also on focus loss / Enter) and writes `-ss … -to …`; Auto sets every "auto"-capable encoder menu to auto and clears the flags; Default and the FHD/HD radios apply `_apply_preset`; preset files are saved in the original key order with 4-space indent and merged on load like `_apply_settings_dict`
- [x] **Trim slider**: handles map to `trim_start`/`trim_end` via the ffprobe duration (1 s minimum gap, end stops 0.1 s before the last frame); reset restores the full range; a new input sets `trim_end` to its duration
- [x] **Output auto-naming**: `<stem>_<codec>_custom.<ext>` in `last_output_dir` (if it exists) or next to the input, container kept for mp4/mkv/mov/ts/mts/m2ts/webm; a codec switch renames only auto-named outputs
- [x] **Estimated file size**: `(video + audio kbps) × duration / 8 / 1024` from a background, cached ffprobe run (`core/probe.rs`, `core/estimate.rs`)
- [x] **Output line + progress bar** under the status row (parity with 1.8.4's `ffmpeg_output` label and hidden progress frame; the full log inspector stays in Phase 8)
- [x] **Custom command mode**: Output opens an editable preview window (Copy / Apply / Reset / Close); Apply validates `-i` + output file; right-click on Output copies the command with spaced paths quoted
- [x] Parity fixes: H.264 hides Lookahead Level (as the original), CUDA Output Format is disabled unless HW Accel = cuda, tune = lossless turns AQ off, `Send to nvencFFX` argument is loaded at startup, ffmpeg / ffprobe auto-detected (exe folder, then `PATH`)

Remaining (moved to later phases):
- [ ] **Streams** button (stream map window) and the `?` / Help buttons → Phase 8
- [ ] Thumbnail preview while dragging the trim handles → Phase 8
- [ ] Play / Preview / Batch / Screen Record / Convert actions → Phases 4, 6, 7

Deliberate differences from 1.8.4:
- Loading a preset no longer forces the profile back to `main` (the original's `_update_codec_settings` overwrote e.g. `main10` from a preset); invalid values for the current codec are still reset.
- The command preview quotes arguments with spaces and its parser removes double quotes, so `-metadata title="a b"` style arguments round-trip (Python's non-POSIX `shlex` kept the quote characters).

### Phase 3: Configuration Persistence & Presets

1. ✅ `config::Settings` holds all 46 keys of `_get_current_settings` (same order); `apply_json` merges like `_apply_settings_dict` (missing / empty keys keep the current value)
2. ✅ Missing keys in old files are handled by the merge
3. Load `nff_settings.json` on startup; auto-save on every settings change with debounced writes (~500ms)
4. ✅ (done in Phase 2) Preset loading from `presets/*.json`:
   - Scan preset directory at startup, populate preset list
   - Save/Load/Delete presets
   - Preset files use the same field schema as settings (minus paths)
5. Persist `last_input_dir` and `last_output_dir` for file dialog defaults
6. Handle `version` field for future format migrations

### Phase 4: FFmpeg Core Engine & Process Runner

1. **Command Line Builder** (`command.rs`) ✅ ported in Phase 2 for the command preview, with tests against the original's output:
   - 3 operating modes: **Encoding** (NVENC), **Stream Copy** (`-c:v copy`), **Custom Command** (raw CLI)
   - NVENC codecs: `h264_nvenc`, `hevc_nvenc`, `av1_nvenc`
   - Full parameter matrix: preset (p1–p7), tune (hq/ll/ull), profile, level, tier, coder, multipass, rc (constqp/vbr/cbr), lookahead_level, split_encode_mode
   - AQ flags: spatial_aq, temporal_aq, strict_gop, no_scenecut, weighted_pred (with `-bf 0`)
   - Rate control: CQP mode (`-rc constqp -qp <val>`) vs VBR/CBR with maxrate/bufsize auto-calculation
   - Video filters chain: fps, scale with interpolation algo, additional_filter_options
   - Audio filter chain: `-af` from additional_audio_filter_options
   - Stream mapping: `-map 0:v? -map 0:a? -map 0:s?` (MP4) vs `-map 0` (MKV), `-map_metadata 0 -map_chapters 0`
   - Subtitle codec: auto-insert `-c:s mov_text` for non-MKV outputs
   - Trim: `-ss`/`-to` positioning — before `-i` (fast seek) vs after (precise), streamcopy mode
   - Post-process: retarget all video encoder options from `:v` to `:v:0` to avoid applying to cover art streams
   - Hardware acceleration: `-hwaccel:v cuda/d3d11va/vulkan`, `-hwaccel_output_format cuda`
   - Additional raw options injection and `-filter_complex` detection (skip `-vf` if present)

2. **ffprobe Integration** (`probe.rs`):
   - Derive ffprobe path from ffmpeg path (`ffprobe.exe` in same directory)
   - Run `ffprobe -hide_banner <file>` and parse stderr output
   - LRU cache (max 50 entries) with `OrderedDict`-like behavior
   - Extract: duration, codec, resolution, bitrate, audio tracks for tooltip display
   - JSON mode: `-print_format json -show_streams -show_format` for size estimation

3. **Process Runner** (`process.rs`):
   - Spawn with `CREATE_NO_WINDOW` + `STARTF_USESHOWWINDOW` on Windows
   - Line-by-line stderr/stdout reader in a worker thread, no UI blocking
   - Parse progress: `fps=`, `q=`, `size=`, `time=`, `bitrate=`, `speed=`
   - Calculate percentage from `time=` vs known duration
   - Send progress snapshots to UI via `std::sync::mpsc` or `crossbeam-channel`

4. **Process Tree Kill** (`process_tree.rs`):
   - `CreateToolhelp32Snapshot` + `Process32FirstW`/`Process32NextW` to find ffmpeg child PIDs
   - Kill only our ffmpeg.exe children (match by parent PID)
   - Use on Abort, window close, and recording stop

5. **Size Estimation** (`estimate.rs`) ✅ done in Phase 2:
   - Run ffprobe to get input duration in seconds
   - Calculate: `(video_bitrate + audio_bitrate) × duration / 8 / 1024` → MB
   - Display in status bar as "Estimated size: ~X MB"
   - Show "Not available for CQP" when in constant QP mode

### Phase 5: Win32 Platform Integration

1. **Taskbar Progress Overlay** (`taskbar.rs`):
   - `CoCreateInstance` → `ITaskbarList3`
   - `SetProgressValue(hwnd, completed, total)` for 0–100% progress bar on taskbar icon
   - `SetProgressState`: `TBPF_NORMAL` (green), `TBPF_PAUSED` (yellow), `TBPF_ERROR` (red), `TBPF_NOPROGRESS` (clear)

2. **Process Pause / Resume** (`suspend.rs`):
   - `OpenProcess(PROCESS_SUSPEND_RESUME)` → `NtSuspendProcess` / `NtResumeProcess`
   - Triggered by right-click context menu on Convert button
   - Update taskbar state to `TBPF_PAUSED` (yellow) when paused

3. **System Tray** (`tray.rs`):
   - `tray-icon` crate: icon, tooltip "nvencFFX", context menu (Start Recording / Stop Recording / Open / Exit)
   - Notification balloons for recording start/stop events
   - Minimize-to-tray behavior

4. **Global Hotkeys** (`hotkeys.rs`):
   - `RegisterHotKey`: Alt+F8 = Start Recording, Alt+F9 = Stop Recording
   - Hidden message-only window (`CreateWindowExW`) for WM_HOTKEY dispatch
   - `UnregisterHotKey` on shutdown

5. **Clipboard** (`clipboard.rs`):
   - `arboard` crate or raw Win32 (`OpenClipboard`/`SetClipboardData`)
   - "Copy Command" button copies the built FFmpeg command line
   - Log context menu "Copy" copies selected log text

### Phase 6: Batch Converter & Task Queue

1. Port `BatchConverterWindow` as a separate egui Window/modal:
   - Queue model: `Vec<BatchTask>` with status (Pending / Converting / Done / Error)
   - Drag & Drop folder ingestion with recursive video extension filtering (`.mp4`, `.mkv`, `.avi`, `.mov`, `.webm`, `.ts`, `.flv`, `.wmv`)
   - Output folder selection, optional container change (mp4/mkv/avi/mov/webm/ts)
   - Per-file progress bar + overall progress bar
   - Sequential execution: pick next Pending task, build command from main app settings, run, update status
   - Pause/Resume/Abort controls per-item and for entire queue
   - Persist `batch_output_folder`, `batch_change_container`, `batch_output_container` in settings

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
| Phase 1 — Theme & Widgets | 🟢 ~98% (context menu left) | `ui/theme.rs`, `ui/widgets.rs`, `ui/dialogs.rs` | ~950 |
| Phase 2 — Main UI Layout | ✅ Done (Streams / help windows moved to Phase 8) | `app.rs`, `core/filters.rs`, `core/timecode.rs` | ~1,700 |
| Phase 3 — Settings & Presets | 🟡 Model + presets done; `nff_settings.json` load/auto-save left | `config/settings.rs`, `config/presets.rs` | ~470 |
| Phase 4 — FFmpeg Core | 🟡 Builder, duration probe, estimate done; runner left | `core/command.rs`, `core/probe.rs`, `core/estimate.rs` | ~600 |
| Phase 5 — Win32 Integration | ⬜ Not started | — | — |
| Phase 6 — Batch Converter | ⬜ Not started | — | — |
| Phase 7 — Screen Recording | ⬜ Not started | — | — |
| Phase 8 — Polish & Help | ⬜ Not started | — | — |
| Phase 9 — Release | 🟡 Config done | `Cargo.toml`, `build.rs` | — |

**Current Rust codebase**: ~4,000 LOC across 17 files (29 tests).

---

## 7. Critical Risks & Notes

1. **egui file dialogs**: egui has no built-in file dialog. The `rfd` crate is **required** for Browse/Save As/Output Folder buttons — this is a hard dependency, not optional.
2. ~~**Audio options gap**~~: resolved; all 9 audio modes are in the UI and the command builder.
3. **Custom command mode**: editing / applying is done (Phase 2); executing the stored `custom_command` comes with the Phase 4 runner.
4. **Settings backward compat**: The Rust version must read and write `nff_settings.json` that the Python version can also read. Test with real user settings files.
5. **Process tree kill**: Simply calling `process.kill()` on Windows may leave orphan ffmpeg.exe processes. Must implement toolhelp32 snapshot-based child process enumeration (same as Python's `PROCESSENTRY32W` approach).
6. **Screen recording complexity**: The segmented recording system with WASAPI audio muxing is the most complex feature (~650 LOC in Python). Consider deferring to a later release if needed.
7. **eframe 0.36 + edition 2024**: eframe 0.36 needs rustc ≥ 1.95 (`rust-version` in `Cargo.toml`). It brings hinted text (skrifa + vello_cpu); keep `PresentMode::AutoNoVsync` in `main.rs`, since eframe still repaints on every window move and vsync makes dragging stutter on Windows.
