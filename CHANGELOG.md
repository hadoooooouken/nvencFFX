# nvencFFX - Changelog

## [2.0.0] - Unreleased
- **Rewritten in Rust**:
    - nvencFFX has been rewritten from Python (CustomTkinter) in Rust (egui / wgpu). The interface, settings, presets and FFmpeg command lines stay the same;
    - The release is a single `nvencFFX.exe` (about 8.5 MB) instead of a folder with 68+ files. Help texts, fonts and the icon are embedded; the release folder holds `nvencFFX.exe`, `presets`, `LICENSE.txt` and `THIRD-PARTY-NOTICES.txt`;
    - The window opens almost instantly and is drawn on the GPU (Vulkan or DirectX 12, with the Windows software renderer as a fallback);
    - `nff_settings.json` and presets from 1.7.x / 1.8.x load unchanged, and files saved by 2.0.0 can still be read by 1.8.x;
    - The "Send to" shortcut points straight at `nvencFFX.exe`; `Send to nvencFFX.bat` is no longer needed.
- **Conversion**:
    - Progress of a trimmed conversion is measured against the trimmed range, so it now reaches 100%;
    - **Play 10s Preview** no longer freezes the window while the clip is cut;
    - On exit, only FFmpeg processes started by nvencFFX are closed (1.8.4 also closed other programs' `ffmpeg.exe` started from the same path);
    - The command preview quotes arguments with spaces, so options like `-metadata title="a b"` survive editing and applying the command;
    - `tune = lossless` from a preset or saved settings now always turns Spatial / Temporal AQ off (HEVC / AV1 presets could keep AQ on);
    - Loading a preset keeps its profile (e.g. `main10`) instead of resetting it to `main`;
    - Stream mapping follows the input's streams (read with ffprobe): a file with one video and one audio stream and no subtitles gets a plain command without `-map` / `-c:v copy` / `-c:s mov_text`, and the other options are only added when the file needs them. Picture-based subtitles (PGS, VobSub, DVB) are left out of MP4 / MOV outputs instead of failing the conversion, the encoder targets the main video stream even when cover art comes first, and MKV outputs copy subtitles instead of converting them to ASS and leave cover art out (MKV would store it as extra one-frame video tracks). Without ffprobe the full mapping is used as before.
- **Interface**:
    - Input / Output / FFmpeg Path fields start right after their labels and are wider; the Presets section uses its full width; the right-hand buttons line up with the right edge;
    - The main scroll bar appears only when the content does not fit, and the window never opens taller than the screen above the taskbar;
    - Help and `?` windows open centered on the main window, come to the front when clicked again, and the `?` windows scroll horizontally for long lines;
    - The input file tooltip loads on first hover instead of on every input change;
    - The frame preview above the trim slider no longer freezes the window, keeps the video's aspect ratio, stays inside the main window at any display scaling, and shows "Preview unavailable" in place instead of in a popup;
    - Clicking the trim slider (not a handle) moves the nearer handle, and the frame preview follows it.
- **Batch Converter**:
    - An applied custom command now gets each file's input and output, instead of running the identical command for every file;
    - Removing files during a batch no longer marks the wrong file or skips one;
    - A batch and **Play 10s Preview** can no longer run at the same time;
    - The Convert button text always matches what is running;
    - Dropping several unsupported files shows one warning instead of one per file;
    - The next file starts immediately; durations are read in the background;
    - With `-map 0:<type>:N` options (as written by the Streams window), the files' streams are compared with the main window's input before the batch starts. Files whose stream layout differs (other track count, language or order) are listed with a choice to continue, since those options would pick other streams or fail there.
- **Screen Recording**:
    - **Alt+F8** / tray Start is ignored while a recording runs (1.8.4 started a second session and orphaned the first); **Alt+F9** / tray Stop without a recording does nothing;
    - Closing nvencFFX during a recording now saves it first (1.8.4 lost the recording);
    - Silent stretches are recorded as silence, so the audio stays in sync with the video;
    - An invalid custom audio bitrate is reported before recording starts instead of failing the save afterwards;
    - The default output goes to the actual Desktop folder, also when it was moved (e.g. into OneDrive);
    - The window title no longer stays at "Screen is recording now" when the FFmpeg path is empty;
    - Holding a hotkey no longer repeats it; the tray icon comes back after Explorer restarts, and its menu opens reliably on every right-click.

## [1.8.4] - 2026-08-07
- Added Windows taskbar progress indicator for conversions;
- Added pause/resume support for FFmpeg encoding tasks. Right-click the Cancel button to pause or resume the current conversion;
- Batch conversion now shows overall progress in the taskbar;
- Improved optional stream mapping handling. Missing or unsupported streams are now skipped instead of causing conversion failures.

## [1.8.3] - 2026-08-02
- **FFmpeg & NVENC Encoding**:
  - Fixed unrecognized option error by updating NVENC Adaptive Quantization flags syntax (`-spatial_aq` ➔ `-spatial-aq`, `-temporal_aq` ➔ `-temporal-aq`) for modern FFmpeg compatibility.

## [1.8.2] - 2026-07-22
- **Screen Recording Reliability**:
    - Screen recordings are now written to numbered temporary segments;
    - FFmpeg automatically restarts in a new segment when `ddagrab` loses access after a display topology change, such as switching projection modes with **Win+P**;
    - Added a three-attempt restart limit to prevent infinite recovery loops;
    - Stopping the recording manually now joins all captured segments with FFmpeg stream copy (`-c copy`) before muxing audio, without re-encoding or quality loss;
    - Temporary segments are removed after the final recording is saved successfully and preserved if finalization fails.

## [1.8.1] - 2026-06-23
- **Default output container**: Improved automatic output file naming. When a video file with an extension outside the preserved list (`.mp4`, `.mkv`, `.mov`, `.ts`, `.m2ts`, `.webm`) is selected, the output now defaults to `.mp4` for better HEVC/AV1 compatibility. Extensions from the preserved list remain unchanged.

## [1.8.0] - 2026-06-06
- **FFmpeg Command Generation**: Resolved a crash when transcoding files with embedded cover art or thumbnails. 
    - Implemented a robust fallback that copies all secondary video streams (attached pictures) while encoding only the primary video stream (v:0);
    - Added post-processing to explicitly target encoder-specific options (preset, profile, rate control, etc.) to the primary video stream (:v:0 instead of :v), preventing compatibility errors on copied stream contexts.

## [1.7.9] - 2026-05-12
- **Save As**: Fixed Save As dialog so the selected container extension (MP4/MKV/MOV) is applied correctly and placeholder text is not offered as the default file name;
- **Tooltip**: Cleaned up file information tooltips by filtering out verbose Chapters blocks.

## [1.7.8] - 2026-05-11
- **Maintenance**: Code cleanup and removal of redundant method definitions.

## [1.7.7] - 2026-05-10
- **Stability**: Minor bug fixes and improvements.

## [1.7.6] - 2026-05-09
- **Screen Recording**: 
    - Screen recording now captures audio (unless "Disable audio" is selected);
    - Added global hotkeys: **Alt+F8** to Start, **Alt+F9** to Stop;
    - Implemented persistent tray icon for easier recording control when minimized;
    - Added Windows balloon notifications and sound alerts for recording status;
    - Added "Open nvencFFX" and "Exit" options to tray context menu;
    - Added double-click on tray icon to restore application window;
    - Changed behavior: application no longer automatically restores the window when recording stops.


## [1.7.5] - 2026-05-08
- **Streams Selection**: Attached pictures (cover art) are now separated from video streams;
- **UI Fix**: Fixed layout shifting in the Presets section when using custom presets with long names.

## [1.7.4] - 2026-05-07
- **Batch Converter Window**: Updated the interface with new persistent "Output Folder" selection and "Change output container" (MP4/MKV/MOV override) features;
- **Default Behavior Change**: Unlike standard FFmpeg, which only picks one track per type, the application now includes ALL streams from the source file by default (implemented `-map 0 -ignore_unknown` logic);
- **Streams Selection**: Added a new Stream Mapping window (opened via the "Streams" button in the "Additional Options" section) for manual selection of video, audio, and subtitle tracks.

## [1.7.3] - 2026-05-05
- **Encoding Fix**: Enforced UTF-8 encoding across all internal system calls (metadata retrieval, executable discovery, duration checks, etc.) to ensure stability across different regional system settings and prevent crashes with special characters;
- **Tooltip Optimization**: Metadata tooltips are now more compact (verbose tags filtered) and feature vertical screen boundary clamping to ensure they remain fully visible;
- **Improved Stability**: Added robust null-safety checks for subprocess outputs to prevent "NoneType" attribute errors during command execution.

## [1.7.2] - 2026-04-19
- The ability to stop screen recording from the system tray icon menu or using the Alt+F9 hotkey;
- Better memory management and cleaning of processes (ffmpeg.exe) after closing the program.

## [1.7.1] - 2026-03-31
- Code cleanup.

## [1.7.0] - 2026-03-10
- Extended input video format support: .mp4, .mkv, .avi, .mov, .flv, .wmv, .webm, .ts, .m4v, .mpg, .mpeg, .m2ts, .mts, .3gp, .ogv, .ogm, .vob, .f4v, .asf, .divx;
- Right-click **Play Output File** after a single convert to run a simplified VMAF comparison.

## [1.6.9] - 2026-03-07
Added right-click handlers for the following buttons:
- **Browse** — opens Windows Explorer in the folder where the file specified in the **Input File** field is located.
- **Save As** — opens Windows Explorer in the folder where the file specified in the **Output File** field is located.
- **FFmpeg** — opens Windows Explorer in the folder where the file specified in the **FFmpeg Path** field is located.
- **Output** — copies the full FFmpeg command to the clipboard.

## [1.6.8] - 2026-03-04
- The **Input File** field now shows a tooltip with metadata of the opened video file when hovering the mouse cursor over it;
- Text fields now support a right-click context menu for text operations (Cut / Copy / Paste / Delete / Select All);
- The **Batch Converter** window now supports drag-and-drop of multiple files directly from Windows Explorer;
- Closing the **Batch Converter** window no longer stops the conversion process (it can now be opened and closed at any time);
- Parameters from the **Add FF Options** field are now always applied, regardless of whether the **Additional Options** section is collapsed or expanded;
- Built-in preset settings have been updated;
- Various UI/UX improvements and fixes.

## [1.6.7] - 2026-03-02
- Code cleanup and optimization + bug fixes;
- Using -filter_complex in Add FF Options automatically ignores -vf filters (simple filters / scaling / FPS / etc.).

## [1.6.6] - 2026-03-01
- Added Ctrl+A handler (select all text);
- Presets: switching between built-in presets (Fast/Quality) now automatically clears filters from the Additional Options section;
- Additional Options: removed the "Drop thresh" button, added "Stereo out" — downmixes the audio track to 2 channels.

## [1.6.5] - 2026-02-22
- Additional Options: Save/Load buttons have been removed, as this functionality is now available via custom presets;
- Additional Options: Added H-Flip and V-Flip filter buttons (mirror video horizontally/vertically);
- UX/UI improvements for working with custom presets;
- Added Ctrl+X handler (cut text);
- Added new presets and cleaned up old ones.

## [1.6.4] - 2026-02-18
- "Screen Record" now respects the selected FPS Mode. My recommendations:  
  Auto/Variable (VFR) – if you want a smaller file size;  
  Constant (CFR) – if you plan to use the recorded video for editing in a video editor.
- Additional Options: improved text paste handling (custom filters) regardless of the current system keyboard layout.
- Presets: updated existing presets and added new `scale_cuda` presets (as examples);
- Fixed BatchConverter.


## [1.6.3] - 2026-02-13
- Added "Advanced Encoder Settings / CUDA Output Format" (-hwaccel_output_format cuda) - It is required when using CUDA-based filters;
- UI fixes in Presets section.

## [1.6.2] - 2026-01-23
- Fixed a bug when switching built-in presets to custom ones.

## [1.6.1] - 2026-01-22
- Added the ability to create custom presets.

## [1.6.0] - 2026-01-06
- FFmpeg Command Preview: fixed "Copy to Clipboard";
- FFmpeg Command Preview: fixed "Apply Changes";
- Trimming: Added previews for HighDPI modes.

## [1.5.9] - 2026-01-05
- Default preset is tweaked for RTX5000.
- Added HighDPI version.

## [1.5.8] - 2026-01-02
- AV1 fixes.

## [1.5.7] - 2025-11-15
- UI/UX fixes and updates.

## [1.5.6] - 2025-10-28
- Additional Options: Added "Save/Load" buttons for custom filter settings;
- UI fixes.

## [1.5.5] - 2025-10-25
- Added "FPS Mode";
- Added "Cancel" for "Play 10s Preview" button;
- Added "Deshake" filter button.

## [1.5.4] - 2025-10-18
- Screen recording now respects the "Preset" parameter value;
- The clipboard is now preserved even after closing the program (for example, a copied ffmpeg command).

## [1.5.3] - 2025-10-15
- Added "Batch Convert";
- Added "Screen Record";
- Build with Python: 3.13.8 + Nuitka 2.9rc2.

## [1.5.2] - 2025-10-11
- Fixed saving of manually selected extensions (MKV, MOV);
- Build with Python: 3.13.8 + Nuitka 2.9rc1.

## [1.5.1] - 2025-10-09
- Added "About" info tab;
- Added "License" info tab;
- Build with Python: 3.13.8 + Nuitka 2.8rc16.

## [1.5.0] - 2025-10-07
- Bug fixes;
- UI improvements;
- Opus audio presets.

## [1.4.9] - 2025-10-04
- Startup UI fixes for some systems;
- Better CPU Fallback handling.

## [1.4.8] - 2025-10-02
- Fixed and updated trim;
- Added new checkboxes: Copy (same as old Streamcopy) and Precise (-ss after -i):
- Added "Default" button in Encoder section (copy of Default preset):
- Presets updated;
- Some UI/UX improvements.

## [1.4.7] - 2025-09-29
- Renamed to nvencFFX;
- Added automatic session saving (ffmpeg path, last input directory, last output directory, codec selection, encoder settings);
- UI cosmetic fixes.

## [1.4.6] - 2025-09-29
- The preview generation mechanism in the Trimming section has been updated.

## [1.4.5] - 2025-09-29
- UI improvements;
- Help file updated;
- Auto Button - Resets all encoder settings to automatic (auto) values and disables all checkboxes (custom settings);
- Clean output of the ffmpeg command if no custom settings are used.

## [1.4.4] - 2025-09-23
- UI improvements;
- Help file information corrections.

## [1.4.3] - 2025-09-22
- Fixed Drag n Drop;
- Removed "highbitdepth";
- Added "Split Encode Mode" option;
- Added "HW Accel" option;
- Improved copypaste (Ctrl+C/V) handling;
- Added "Help" button with a file describing the program parameters;
- Added "Force 8 bit" button;
- Added "Force 10 bit" button;
- Added "Crop to 16:9" button;
- Added "Rotate" button;
- Presets updated.

## [1.4.2] - 2025-09-20
- Cosmetic UI fixes

## [1.4.1] - 2025-09-18
- Presets updated.

## [1.4.0] - 2025-09-17
- Enabled CQP 30 by default;
- Added new buttons: Play Input File, Play 10s Preview, Play Output File;
- Faster generation of preview thumbnails;
- Added a Denoise filter button.

## [1.3.4] - 2025-09-15
- Added MKV and MOV containers for saving in the Save As dialog box.

## [1.3.3] - 2025-09-14
- Fixed the output of the "Copy to clipboard" button.

## [1.3.2] - 2025-09-13
- Added video previews for trim controls.

## [1.3.1] - 2025-09-11
- Added trim slider.

## [1.3.0] - 2025-09-07
- Added Constant QP mode;
- The "Copy to Clipboard" button now automatically quotes file paths in clipboard.

## [1.2.2] - 2025-08-26
- Removed the lookahead_level setting for h264_nvenc, because despite its presence in ffmpeg -h encoder=h264_nvenc - in fact, this parameter does not work with this encoder and causes a conversion error.

## [1.2.1] - 2025-08-26
- Added a button for converting HDR to SDR;
- Added the ability to manually edit the entire generated command for ffmpeg. (Output button).

## [1.2.0] - 2025-08-25
- Automatic saving and loading of the user path to ffmpeg.exe;
- Mechanism for copying/pasting text in fields with a non-English layout;
- Improved mechanism for automatic selection of an audio preset, if this condition is mandatory for some filters;
- Optimization of parameters for Fast and Quality presets;
- New buttons for quick insertion of such parameters as: -fps_mode passthrough, -frame_drop_threshold, gamma, brightness, loudnorm.

## [1.1.3] - 2025-08-23
- Initial release
