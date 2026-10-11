# nvencFFX Migration Plan to Rust (egui + eframe + wgpu)

This document outlines the step-by-step roadmap for migrating **nvencFFX** from Python (CustomTkinter + ctypes + Nuitka) to **Rust** using the **egui / eframe / wgpu** graphics stack.

---

## 1. Migration Goals & Benchmarks

| Metric | Current Python Build (Nuitka) | Target Rust Build (egui + wgpu) |
|---|---|---|
| **Distribution Format** | Directory with 68+ files (Python runtime, DLLs, `.pyd`) | **Single standalone `.exe`** |
| **Distribution Size** | ~51.5 MB (without FFmpeg) / ~374 MB build | **~12–15 MB** (UPX-compressed: ~4–6 MB) |
| **Startup Time** | 1.5–2.5 seconds | **< 50 ms (near-instant)** |
| **RAM Footprint** | ~80–200 MB | **~25–45 MB** |
| **UI Rendering** | Software / Tkinter Canvas | **Hardware-accelerated GPU** (DirectX 12 / Vulkan via `wgpu`) |
| **Win32 Integration** | Fragile `ctypes` bindings | Type-safe, native Rust crates (`windows-sys`, `tray-icon`) |

---

## 2. Target Rust Architecture

The current monolithic ~8,900-line Python file will be decomposed into decoupled, modular crates and modules:

```text
nvencffx/
├── Cargo.toml
├── build.rs                  # Embed nff.ico and Windows manifest via winres
├── src/
│   ├── main.rs               # Entry point, winit/eframe setup, windows_subsystem attribute
│   ├── app.rs                # Main App state (eframe::App implementation)
│   ├── ui/                   # egui interface components
│   │   ├── mod.rs
│   │   ├── theme.rs          # Color palette (#2ECC71 accent, dark mode, styles)
│   │   ├── widgets.rs        # Custom widgets (folding TextCheckbox, tooltip helpers)
│   │   ├── main_view.rs      # Main layout (Input/Output, actions, preset selector)
│   │   ├── video_tab.rs      # Video settings (NVENC/QSV/AMF/CPU codecs, bitrate, CQ, filters)
│   │   ├── audio_tab.rs      # Audio configuration (track mapping, codecs, bitrates)
│   │   ├── batch_view.rs     # Batch Converter window/modal
│   │   └── log_view.rs       # Real-time streaming FFmpeg output log
│   ├── core/                 # FFmpeg business logic
│   │   ├── mod.rs
│   │   ├── command.rs        # FFmpeg CLI argument builder
│   │   ├── probe.rs          # Media inspection (ffprobe JSON / metadata extraction)
│   │   ├── process.rs        # Process management (spawn, suspend, resume, kill tree)
│   │   └── progress.rs       # Stream parser for progress (time, fps, speed, percentage)
│   ├── presets/              # Encoding presets
│   │   ├── mod.rs
│   │   └── model.rs          # JSON serde models, backward-compatible preset loading
│   ├── win32/                # Native Windows API integration
│   │   ├── mod.rs
│   │   ├── taskbar.rs        # ITaskbarList3 (green/yellow/red progress overlay)
│   │   ├── suspend.rs        # NtSuspendProcess / NtResumeProcess (pause/resume conversion)
│   │   ├── tray.rs           # System tray icon, notifications, and context menu
│   │   └── hotkeys.rs        # Global hotkeys (Alt+F8, Alt+F9 for recording)
│   └── audio/                # Audio capture
│       ├── mod.rs
│       └── loopback.rs       # Desktop audio loopback via WASAPI
```

---

## 3. Phased Implementation Plan

### Phase 0: Environment Setup & Project Initialization
1. Ensure Rust toolchain (`rustup`, `cargo`, `rustc`) is installed (`winget install Rustlang.Rustup` or rustup.rs).
2. Initialize Cargo workspace in the repository.
3. Add initial dependencies in `Cargo.toml`:
   - `eframe = { version = "0.29", default-features = false, features = ["wgpu", "default_fonts"] }`
   - `egui = "0.29"`
   - `serde = { version = "1.0", features = ["derive"] }`, `serde_json = "1.0"`
   - `winres` (build dependency for icon & manifest)
4. Verify a minimal running window using `wgpu` backend.

### Phase 1: Design System & UI Theme (egui)
1. Port the custom nvencFFX visual identity:
   - Deep dark palette (`#1E1E1E` background, `#2ECC71` emerald accent).
   - Consistent typography, margins, and border radii.
2. Build custom reusable widgets:
   - `TextCheckbox`: collapsible section headers with `▼` / `▲` indicators.
   - Drag & Drop target areas using native egui file drop events (`ctx.input(|i| i.raw.dropped_files)`).
   - Quick preset selector buttons and sliders with fine-grained value labels.

### Phase 2: Configuration Models & Presets
1. Define serializable data structures:
   - `VideoConfig`, `AudioConfig`, `AppSettings`.
2. Implement 1:1 compatibility with existing `nff_settings.json` and `presets/*.json` using `serde`.
3. Auto-save user settings on change with debounced file writes.

### Phase 3: FFmpeg Core Engine & Async Process Runner
1. **Command Line Builder**:
   - Hardware-accelerated encoders: `h264_nvenc`, `hevc_nvenc`, `av1_nvenc`, `qsv`, `amf`.
   - Software/CPU encoders: `libx264`, `libx265`, `libsvtav1`, `prores`, `libvpx-vp9`.
   - NVENC quality tuning (`p1`–`p7`, CQ, CBR, VBR, multipass, b-frames).
   - Advanced stream mapping (`-map`, audio downmixing, subtitle preservation).
2. **Background Execution**:
   - Spawn FFmpeg in dedicated worker threads (`std::process::Command`).
   - Line-by-line `stderr` reader without UI thread blocking.
   - Parse encoding metrics: `fps=`, `q=`, `size=`, `time=`, `bitrate=`, `speed=`.
   - Dispatch progress snapshots to the egui render loop via lock-free `crossbeam-channel` or `std::sync::mpsc`.

### Phase 4: Win32 Platform Integration
1. **Taskbar Progress Overlay (`ITaskbarList3`)**:
   - Display progress (0–100%) and states (Normal green, Paused yellow, Error red) directly on the taskbar icon.
2. **Process Pause / Resume**:
   - Use `windows-sys` to invoke `NtSuspendProcess` / `NtResumeProcess` on the FFmpeg child process.
3. **Tray Icon & Global Hotkeys**:
   - Integrate `tray-icon` for background minimization, notification balloons, and tray menus.
   - Integrate `global-hotkey` for instant screen recording hotkeys (e.g. Alt+F8 / Alt+F9).

### Phase 5: Batch Converter & Task Queue
1. Port `BatchConverterWindow`:
   - Queue model (`VecDeque<BatchTask>`).
   - Drag & Drop folder ingestion with recursive video extension filtering.
   - Batch preset application.
   - Sequential execution pipeline with item-level and overall progress bars.

### Phase 6: Screen Recording & WASAPI Audio Capture
1. Replace `pyaudiowpatch`:
   - Use `cpal` (configured for WASAPI loopback) or raw WASAPI bindings to capture system/game audio without virtual audio cables.
2. Synchronize desktop screen recording with FFmpeg using `ddagrab` (Desktop Duplication API) or `gdigrab`.

### Phase 7: UI Polish, Tooltips & Documentation Modals
1. Port descriptive tooltips to all encoder settings via `.on_hover_text(...)`.
2. Real-time log inspector with search, auto-scroll, and "Copy Command" button.
3. Integrated Help and About dialogs (porting `nff-help.txt` and `nff-about.txt`).

### Phase 8: Binary Optimization & Release Packaging
1. Configure release profile in `Cargo.toml`:
   ```toml
   [profile.release]
   opt-level = "z"       # Optimize for size
   lto = true            # Link-Time Optimization
   codegen-units = 1     # Maximize cross-crate optimizations
   panic = "abort"       # Remove stack unwinding tables
   strip = true          # Strip all debug symbols
   ```
2. Disable Windows console allocation in production (`#![windows_subsystem = "windows"]`).
3. Embed version metadata, copyright, and `nff.ico` into the executable using `winres`.
4. Output: **Single executable (~12–15 MB)** with native DX12/Vulkan hardware acceleration.
