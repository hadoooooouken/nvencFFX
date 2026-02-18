# nvencFFX - Changelog

## [1.6.4] - 2026-02-18
- **"Screen Record"** now respects the selected **FPS Mode**. My recommendations:  
  **Auto/Variable (VFR)** – if you want a smaller file size;  
  **Constant (CFR)** – if you plan to use the recorded video for editing in a video editor.

- **Additional Options:** improved text paste handling (custom filters) regardless of the current system keyboard layout.

- **Presets:** updated existing presets and added new `scale_cuda` presets (as examples).


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
