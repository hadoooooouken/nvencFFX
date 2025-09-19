import os
import subprocess
import sys
import tempfile
from io import BytesIO
from re import sub
from shlex import split
from threading import Thread
from tkinter import filedialog, messagebox
from winsound import MB_ICONASTERISK, MessageBeep

import customtkinter as ctk
from PIL import Image
from win32api import DragFinish, DragQueryFile
from win32con import GWL_WNDPROC, WM_DROPFILES
from win32gui import CallWindowProc, DragAcceptFiles, SetWindowLong

# UI Theme and Colors
ctk.set_appearance_mode("dark")
PRIMARY_BG = "#0e1113"
SECONDARY_BG = "#171b1f"
ACCENT_GREEN = "#4fb62f"
HOVER_GREEN = "#47a32a"
ACCENT_GREY = "#b2b2b2"
HOVER_GREY = "#8e8e8e"
ACCENT_RED = "#FF5555"
TEXT_COLOR_W = "#FFFFFF"
TEXT_COLOR_B = "#000000"
PLACEHOLDER_COLOR = "#A0A0A0"


class DropTarget:
    def __init__(self, hwnd, callback):
        self.hwnd = hwnd
        self.callback = callback
        DragAcceptFiles(self.hwnd, True)

        # Use SetWindowLong for compatibility
        self.old_wnd_proc = SetWindowLong(self.hwnd, GWL_WNDPROC, self._wnd_proc)
        self._self_ref = self  # important

    def _wnd_proc(self, hwnd, msg, wparam, lparam):
        if msg == WM_DROPFILES:
            try:
                hdrop = wparam
                file_path = DragQueryFile(hdrop, 0)
                if self.callback:
                    self.callback(file_path)
                DragFinish(hdrop)
            except Exception as e:
                print(f"Error handling drop: {e}")
            return 0
        return CallWindowProc(self.old_wnd_proc, hwnd, msg, wparam, lparam)

    def __del__(self):
        if hasattr(self, "old_wnd_proc"):
            SetWindowLong(self.hwnd, GWL_WNDPROC, self.old_wnd_proc)


class VideoConverterApp:
    def _handle_dropped_file(self, file_path):
        if file_path.lower().endswith(
            (".mp4", ".mkv", ".avi", ".mov", ".flv", ".wmv", ".webm")
        ):
            normalized_path = os.path.normpath(file_path)
            self.input_file.set(normalized_path)
            self.input_file_entry.configure(text_color=TEXT_COLOR_W)
            base_name = os.path.splitext(os.path.basename(normalized_path))[0]
            codec_suffix = (
                "_hevc"
                if self.video_codec.get() == "hevc"
                else "_h264"
                if self.video_codec.get() == "h264"
                else "_av1"
            )
            output_path = os.path.normpath(
                os.path.join(
                    os.path.dirname(normalized_path),
                    f"{base_name}{codec_suffix}_custom.mp4",
                )
            )
            self.output_file.set(output_path)
            self.trim_start.set("00:00:00")
            self.total_duration = 0
            self.status_text.set("File selected. Ready for conversion.")
            self._calculate_estimated_size()
            self._set_trim_end_to_duration()

        else:
            messagebox.showwarning(
                "Unsupported File", "Please drop a video file (.mp4, .mkv, .avi, etc.)"
            )

    # App window position
    def _center_window(self):
        self.master.update_idletasks()
        width = self.master.winfo_width()
        screen_width = self.master.winfo_screenwidth()
        x = (screen_width // 2) - (width // 2)
        y = 50
        self.master.geometry(f"+{x}+{y}")

    def _update_window_size(self):
        self.master.update_idletasks()
        current_height = self.master.winfo_height()
        required_height = 700

        if self.enable_audio_options.get():
            required_height += self.audio_frame.winfo_reqheight() + 10

        if self.enable_encoder_options.get():
            required_height += self.encoder_options_frame.winfo_reqheight() + 10

        if self.enable_fps_scale_options.get():
            required_height += self.fps_scale_options_frame.winfo_reqheight() + 10

        if self.enable_additional_options.get():
            required_height += self.additional_options_frame.winfo_reqheight() + 10

        if self.enable_presets.get():
            required_height += self.presets_frame.winfo_reqheight() + 10

        if required_height != current_height:
            self.master.geometry(f"800x{required_height}")

    def __init__(self, master):
        self.preview_job = None  # used for debouncing preview creation
        self.video_metadata_cache = {}
        self.master = master
        master.title("nvencFF Toolbox 1.4.2")
        master.geometry("800x700")
        master.minsize(800, 700)
        master.maxsize(800, 900)
        master.resizable(False, True)
        master.configure(fg_color=PRIMARY_BG)

        # Main container
        self.main_container = ctk.CTkScrollableFrame(master, fg_color=PRIMARY_BG)
        self.main_container.pack(fill="both", expand=True, padx=10, pady=10)

        # Content frame
        self.content_frame = ctk.CTkFrame(self.main_container, fg_color=PRIMARY_BG)
        self.content_frame.pack(fill="x", expand=False)

        # Convert frame
        self.button_frame = ctk.CTkFrame(master, fg_color=PRIMARY_BG)
        self.button_frame.pack(fill="x", padx=15, pady=(0, 15))

        self._setup_variables()
        self._create_widgets()
        self._toggle_constant_qp_mode()  # Enabled CQP by default

        # Find FFmpeg executables (critical dependency)
        self.ffmpeg_path = self._find_executable("ffmpeg.exe")
        self.ffprobe_path = None

        # Try to load saved path
        saved_path = self._load_ffmpeg_path()
        if saved_path:
            self.ffmpeg_path = saved_path
        else:
            if self.ffmpeg_path:
                ffprobe_path = os.path.join(
                    os.path.dirname(self.ffmpeg_path), "ffprobe.exe"
                )
                if os.path.exists(ffprobe_path):
                    self.ffprobe_path = ffprobe_path
                else:
                    self.ffprobe_path = self._find_executable("ffprobe.exe")

        # Set the FFmpeg path in the UI if found
        if self.ffmpeg_path:
            self.ffmpeg_custom_path.set(self.ffmpeg_path)

        if self.ffmpeg_path:
            self.ffmpeg_path_entry.configure(text_color=TEXT_COLOR_W)

        self.video_codec.trace_add("write", self._update_output_filename)

        self.conversion_process = None
        self.conversion_thread = None
        self.is_converting = False

        self._create_trim_slider()

        self._center_window()
        self.drop_target = DropTarget(self.master.winfo_id(), self._handle_dropped_file)
        self._create_thumbnail_preview()
        self._setup_keyboard_shortcuts()

        if len(sys.argv) > 1:
            self._handle_dropped_file(sys.argv[1])

        self.preview_temp_files = []  # Add list for preview temporary files

    def _create_10s_preview(self):
        """Create a 10-second preview with current settings"""
        if self.is_converting or self.is_creating_preview:
            messagebox.showerror("Error", "Please wait until current process completes")
            return

        if not self.input_file.get() or self.input_file.get().startswith(
            "Drag and drop"
        ):
            messagebox.showerror("Error", "Please select input file first")
            return

        # Get video duration
        if not hasattr(self, "total_duration") or self.total_duration <= 0:
            self._get_video_duration()

        if not hasattr(self, "total_duration") or self.total_duration <= 0:
            messagebox.showerror("Error", "Could not get video duration")
            return

        # Calculate video midpoint
        duration = self.total_duration
        mid_point = max(0, (duration - 10) / 2)

        # Create temporary files
        input_path = self.input_file.get()
        base_name = os.path.splitext(os.path.basename(input_path))[0]
        temp_dir = tempfile.gettempdir()

        # Clean up previous temporary files
        self._cleanup_preview_files()

        # Temporary file for streamcopy
        temp_streamcopy = os.path.join(temp_dir, f"{base_name}_streamcopy10s.mp4")
        self.preview_temp_files.append(temp_streamcopy)

        # Temporary file for encoded preview
        temp_encoded = os.path.join(temp_dir, f"{base_name}_encoded10s.mp4")
        self.preview_temp_files.append(temp_encoded)

        # Command to create streamcopy
        streamcopy_cmd = [
            self.ffmpeg_path,
            "-ss",
            str(mid_point),
            "-i",
            input_path,
            "-t",
            "10",
            "-c",
            "copy",
            "-y",
            temp_streamcopy,
        ]

        # Execute streamcopy
        try:
            subprocess.run(
                streamcopy_cmd, check=True, creationflags=subprocess.CREATE_NO_WINDOW
            )
        except subprocess.CalledProcessError as e:
            messagebox.showerror("Error", f"Streamcopy failed: {e}")
            return

        # Build encoding command with current settings (without trimming)
        try:
            # Save original values
            original_input = self.input_file.get()
            original_output = self.output_file.get()

            # Temporarily substitute values to build the command
            self.input_file.set(temp_streamcopy)
            self.output_file.set(temp_encoded)

            encode_cmd = self._build_ffmpeg_command(preview=True)

            # Restore original values
            self.input_file.set(original_input)
            self.output_file.set(original_output)

            # Start preview encoding with progress
            self.is_creating_preview = True
            self.status_text.set("Creating 10-second preview...")
            self.ffmpeg_output.set("Starting preview encoding...")
            self.progress_value.set(0.0)
            self.progress_label.configure(text="0%")
            self.progress_frame.grid()

            # Start encoding in separate thread
            preview_thread = Thread(
                target=self._run_preview_encoding, args=(encode_cmd, temp_encoded)
            )
            preview_thread.start()

        except Exception as e:
            messagebox.showerror("Error", f"Preview encoding failed: {e}")
            self.is_creating_preview = False

    def _run_preview_encoding(self, command, output_path):
        """Run preview encoding with progress tracking"""
        startupinfo = None
        creationflags = 0
        if os.name == "nt":
            startupinfo = subprocess.STARTUPINFO()
            startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
            startupinfo.wShowWindow = subprocess.SW_HIDE
            creationflags = subprocess.CREATE_NO_WINDOW

        try:
            process = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                startupinfo=startupinfo,
                creationflags=creationflags,
                encoding="utf-8",
                errors="replace",
            )

            # Process output in real-time
            for line in process.stdout:
                if line:
                    self.master.after(0, lambda: self.ffmpeg_output.set(line))
                    self.master.after(0, lambda: self._update_preview_progress(line))

            process.wait()

            if process.returncode == 0:
                self.master.after(
                    0, lambda: self.status_text.set("Preview created successfully!")
                )
                self.master.after(0, lambda: self.ffmpeg_output.set(""))
                # Play the result
                os.startfile(output_path)
            else:
                self.master.after(
                    0, lambda: self.status_text.set("Preview creation failed!")
                )

        except Exception as e:
            error_message = f"Preview error: {str(e)}"
            self.master.after(0, lambda: self.status_text.set(error_message))
        finally:
            self.master.after(0, lambda: self.progress_frame.grid_remove())
            self.is_creating_preview = False

    def _update_preview_progress(self, line):
        """Update progress for preview encoding"""
        if "time=" in line:
            time_pos = line.find("time=")
            time_str = line[time_pos + 5:].split()[0]
            try:
                h, m, s = time_str.split(":")
                total_seconds = int(h) * 3600 + int(m) * 60 + float(s)
                # Preview is always 10 seconds long
                progress = min(1.0, total_seconds / 10.0)
                self.progress_value.set(progress)
                self.progress_label.configure(text=f"{progress * 100:.1f}%")
            except Exception:
                pass

    def _setup_variables(self):
        # Initialize all Tkinter control variables
        self.input_file = ctk.StringVar()
        self.output_file = ctk.StringVar()
        self.bitrate = ctk.StringVar(value="6000")
        self.ffmpeg_output = ctk.StringVar(value="")
        self.audio_option = ctk.StringVar(value="copy")
        self.custom_abitrate = ctk.StringVar(value="160")
        self.enable_audio_options = ctk.BooleanVar(value=False)
        self.enable_encoder_options = ctk.BooleanVar(value=False)
        self.preset = ctk.StringVar(value="p5")
        self.tune = ctk.StringVar(value="hq")
        self.profile = ctk.StringVar(value="main")
        self.level = ctk.StringVar(value="auto")
        self.tier = ctk.StringVar(value="1")
        self.multipass = ctk.StringVar(value="qres")
        self.rc = ctk.StringVar(value="vbr")
        self.lookahead_level = ctk.StringVar(value="-1")
        self.strict_gop = ctk.BooleanVar(value=False)
        self.spatial_aq = ctk.BooleanVar(value=True)
        self.temporal_aq = ctk.BooleanVar(value=True)
        self.no_scenecut = ctk.BooleanVar(value=False)
        self.weighted_pred = ctk.BooleanVar(value=False)
        self.highbitdepth = ctk.BooleanVar(value=False)
        self.enable_fps_scale_options = ctk.BooleanVar(value=False)
        self.fps_option = ctk.StringVar(value="source")
        self.custom_fps = ctk.StringVar(value="30")
        self.video_format_option = ctk.StringVar(value="source")
        self.custom_video_width = ctk.StringVar(value="1920")
        self.interpolation_algo = ctk.StringVar(value="bicubic")
        self.enable_additional_options = ctk.BooleanVar(value=False)
        self.additional_options = ctk.StringVar(value="")
        self.additional_filter_options = ctk.StringVar(value="")
        self.additional_audio_filter_options = ctk.StringVar(value="")
        self.additional_options_placeholder = (
            "e.g. -aq-strength 8; -cq 0; -bf 0 or -bf 4;"
            " -intra-refresh 1; -forced-idr 1"
        )
        self.additional_filter_options_placeholder = (
            "e.g.setpts=0.5*PTS - speed up video x2; crop=iw:min(ih\\,iw*9/16)"
        )
        self.additional_audio_filter_options_placeholder = "e.g. atempo=2.0, volume=1.5"
        self.status_text = ctk.StringVar(value="Ready for conversion")
        self.estimated_file_size = ctk.StringVar(value="")
        self.progress_value = ctk.DoubleVar(value=0.0)
        self.selected_preset = ctk.StringVar(value="none")
        self.enable_presets = ctk.BooleanVar(value=False)
        self.ffmpeg_custom_path = ctk.StringVar(value="")
        self.ffmpeg_path_placeholder = "Path to ffmpeg.exe (required)"
        self.bitrate.trace_add("write", lambda *args: self._calculate_estimated_size())
        self.audio_option.trace_add(
            "write", lambda *args: self._calculate_estimated_size()
        )
        self.custom_abitrate.trace_add(
            "write", lambda *args: self._calculate_estimated_size()
        )
        self.video_codec = ctk.StringVar(value="hevc")
        self.coder = ctk.StringVar(value="cabac")
        self.trim_start = ctk.StringVar(value="00:00:00")
        self.trim_end = ctk.StringVar(value="00:00:00")
        self.trim_streamcopy = ctk.BooleanVar(value=False)
        self.constant_qp_mode = ctk.BooleanVar(value=True)  # Enabled CQP by default
        self.quality_level = ctk.StringVar(value="30")
        self.quality_level.trace_add(
            "write", lambda *args: self._calculate_estimated_size()
        )
        self.is_creating_preview = False

    def _setup_keyboard_shortcuts(self):
        self.master.bind_all("<Control-KeyPress>", self._handle_key_press)

    def _handle_key_press(self, event):
        # keycode 67 = 'C'
        if event.keycode == 67 and (event.state & 0x0004):  # Ctrl+C
            self._copy_text()
            return "break"

        # keycode 86 = 'V'
        elif event.keycode == 86 and (event.state & 0x0004):  # Ctrl+V
            self._paste_text()
            return "break"

    def _copy_text(self):
        widget = self.master.focus_get()
        if hasattr(widget, "get"):
            self.master.clipboard_clear()
            text = widget.get()
            if hasattr(widget, "selection_get"):
                try:
                    text = widget.selection_get()
                except Exception:
                    pass
            self.master.clipboard_append(text)

    def _paste_text(self):
        widget = self.master.focus_get()
        if hasattr(widget, "insert"):
            try:
                text = self.master.clipboard_get()
                if hasattr(widget, "delete"):
                    widget.delete(0, "end")
                widget.insert("insert", text)
            except ctk.TclError:
                pass

    def _create_widgets(self):
        # Build the entire GUI interface
        main_frame = ctk.CTkFrame(
            self.content_frame, fg_color=PRIMARY_BG, corner_radius=25
        )
        main_frame.pack(fill="both", expand=True, padx=15, pady=15)

        # Input File
        ctk.CTkLabel(main_frame, text="Input File:").grid(
            row=0, column=0, sticky="w", padx=10, pady=5
        )
        self.input_file_entry = ctk.CTkEntry(
            main_frame,
            textvariable=self.input_file,
            width=350,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self.input_file_entry.grid(row=0, column=1, padx=5, pady=5)
        self.input_file_entry.insert(
            0, "Drag and drop a video file here or use the 'Browse' button."
        )
        self.input_file_entry.configure(text_color=PLACEHOLDER_COLOR)
        self.input_file_entry.bind("<FocusIn>", self._on_input_file_focus_in)
        self.input_file_entry.bind("<FocusOut>", self._on_input_file_focus_out)

        # Browse button
        ctk.CTkButton(
            main_frame,
            text="Browse",
            command=self._browse_input,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
        ).grid(row=0, column=2, padx=5, pady=5)

        # Output File
        ctk.CTkLabel(main_frame, text="Output File:").grid(
            row=1, column=0, sticky="w", padx=10, pady=5
        )
        ctk.CTkEntry(
            main_frame,
            textvariable=self.output_file,
            width=350,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        ).grid(row=1, column=1, padx=5, pady=5)

        # Save As button
        ctk.CTkButton(
            main_frame,
            text="Save As",
            command=self._browse_output,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
        ).grid(row=1, column=2, padx=5, pady=5)

        # FFmpeg Path
        ctk.CTkLabel(main_frame, text="FFmpeg Path:").grid(
            row=2, column=0, sticky="w", padx=10, pady=5
        )
        self.ffmpeg_path_entry = ctk.CTkEntry(
            main_frame,
            textvariable=self.ffmpeg_custom_path,
            width=350,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self.ffmpeg_path_entry.grid(row=2, column=1, padx=5, pady=5)
        self.ffmpeg_path_entry.insert(0, self.ffmpeg_path_placeholder)
        self.ffmpeg_path_entry.configure(text_color=PLACEHOLDER_COLOR)
        self.ffmpeg_path_entry.bind("<FocusIn>", self._on_ffmpeg_path_focus_in)
        self.ffmpeg_path_entry.bind("<FocusOut>", self._on_ffmpeg_path_focus_out)

        # Browse button for FFmpeg
        ctk.CTkButton(
            main_frame,
            text="FFmpeg",
            command=self._browse_ffmpeg,
            fg_color=ACCENT_GREY,
            hover_color=HOVER_GREY,
            text_color=TEXT_COLOR_B,
        ).grid(row=2, column=2, padx=5, pady=5)

        # Video Bitrate/Quality Level
        self.bitrate_label = ctk.CTkLabel(main_frame, text="Video Bitrate (k):")
        self.bitrate_label.grid(row=3, column=0, sticky="w", padx=10, pady=5)

        # Frame for bitrate/quality controls
        bitrate_frame = ctk.CTkFrame(main_frame, fg_color="transparent")
        bitrate_frame.grid(row=3, column=1, sticky="w", padx=5, pady=5)

        self.bitrate_entry = ctk.CTkEntry(
            bitrate_frame,
            textvariable=self.bitrate,
            width=80,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self.bitrate_entry.pack(side="left")

        # Constant QP mode checkbox
        self.constant_qp_checkbox = ctk.CTkCheckBox(
            bitrate_frame,
            text="Constant QP mode",
            variable=self.constant_qp_mode,
            command=self._toggle_constant_qp_mode,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        self.constant_qp_checkbox.pack(side="left", padx=(10, 0))

        # Quality Level label and entry (hidden by default)
        self.quality_level_label = ctk.CTkLabel(bitrate_frame, text="Quality Level:")
        self.quality_level_entry = ctk.CTkEntry(
            bitrate_frame,
            textvariable=self.quality_level,
            width=80,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )

        ctk.CTkButton(
            main_frame,
            text="Output",
            command=self._show_output_command,
            fg_color=ACCENT_GREY,
            hover_color=HOVER_GREY,
            text_color=TEXT_COLOR_B,
        ).grid(row=3, column=2, sticky="w", padx=5, pady=5)

        # Video Codec Selection
        ctk.CTkLabel(main_frame, text="Video Codec:").grid(
            row=4, column=0, sticky="w", padx=10, pady=5
        )
        codec_frame = ctk.CTkFrame(main_frame, fg_color="transparent")
        codec_frame.grid(row=4, column=1, sticky="w", padx=5, pady=5)

        hevc_rb = ctk.CTkRadioButton(
            codec_frame,
            text="HEVC",
            variable=self.video_codec,
            value="hevc",
            command=self._update_codec_settings,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        hevc_rb.pack(side="left", padx=5)

        h264_rb = ctk.CTkRadioButton(
            codec_frame,
            text="H.264",
            variable=self.video_codec,
            value="h264",
            command=self._update_codec_settings,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        h264_rb.pack(side="left", padx=5)

        av1_rb = ctk.CTkRadioButton(
            codec_frame,
            text="AV1",
            variable=self.video_codec,
            value="av1",
            command=self._update_codec_settings,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        av1_rb.pack(side="left", padx=5)

        # Encoder Options
        encoder_options_frame_toggle = ctk.CTkCheckBox(
            main_frame,
            text="Advanced Encoder Settings",
            variable=self.enable_encoder_options,
            command=self._toggle_encoder_options_frame,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        encoder_options_frame_toggle.grid(row=5, column=0, sticky="w", padx=10, pady=5)

        self.encoder_options_frame = ctk.CTkFrame(main_frame, fg_color=SECONDARY_BG)

        left_column_frame = ctk.CTkFrame(
            self.encoder_options_frame, fg_color="transparent"
        )
        left_column_frame.grid(
            row=1, column=0, columnspan=2, padx=5, pady=5, sticky="nsew"
        )

        # Preset
        ctk.CTkLabel(left_column_frame, text="Preset:").grid(
            row=0, column=0, sticky="w", padx=5, pady=2
        )
        preset_option_menu = ctk.CTkOptionMenu(
            left_column_frame,
            variable=self.preset,
            values=[f"p{i}" for i in range(1, 8)],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        preset_option_menu.grid(row=0, column=1, sticky="ew", padx=5, pady=2)

        # Tune
        ctk.CTkLabel(left_column_frame, text="Tune:").grid(
            row=1, column=0, sticky="w", padx=5, pady=2
        )
        tune_option_menu = ctk.CTkOptionMenu(
            left_column_frame,
            variable=self.tune,
            values=["hq", "uhq", "ll", "ull", "lossless"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        tune_option_menu.grid(row=1, column=1, sticky="ew", padx=5, pady=2)

        # Profile
        ctk.CTkLabel(left_column_frame, text="Profile:").grid(
            row=2, column=0, sticky="w", padx=5, pady=2
        )
        profile_option_menu = ctk.CTkOptionMenu(
            left_column_frame,
            variable=self.profile,
            values=["main", "main10", "rext"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        profile_option_menu.grid(row=2, column=1, sticky="ew", padx=5, pady=2)

        # Level
        ctk.CTkLabel(left_column_frame, text="Level:").grid(
            row=3, column=0, sticky="w", padx=5, pady=2
        )
        level_option_menu = ctk.CTkOptionMenu(
            left_column_frame,
            variable=self.level,
            values=[
                "auto",
                "1.0",
                "2.0",
                "2.1",
                "3.0",
                "3.1",
                "4.0",
                "4.1",
                "5.0",
                "5.1",
                "5.2",
                "6.0",
                "6.1",
                "6.2",
            ],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        level_option_menu.grid(row=3, column=1, sticky="ew", padx=5, pady=2)

        # Tier Label (for HEVC and AV1)
        self.tier_label = ctk.CTkLabel(left_column_frame, text="Tier:")
        self.tier_label.grid(row=4, column=0, sticky="w", padx=5, pady=2)

        # Tier OptionMenu
        self.tier_option_menu = ctk.CTkOptionMenu(
            left_column_frame,
            variable=self.tier,
            values=["0", "1"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        self.tier_option_menu.grid(row=4, column=1, sticky="ew", padx=5, pady=2)

        # Coder Label (only for H.264)
        self.coder_label = ctk.CTkLabel(left_column_frame, text="Coder:")
        self.coder_label.grid(row=4, column=0, sticky="w", padx=5, pady=2)
        self.coder_label.grid_remove()  # hide by default

        # Coder OptionMenu
        self.coder_option_menu = ctk.CTkOptionMenu(
            left_column_frame,
            variable=self.coder,
            values=["default", "auto", "cabac", "cavlc", "ac", "vlc"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        self.coder_option_menu.grid(row=4, column=1, sticky="ew", padx=5, pady=2)
        self.coder_option_menu.grid_remove()  # hide by default

        right_column_frame = ctk.CTkFrame(
            self.encoder_options_frame, fg_color="transparent"
        )
        right_column_frame.grid(
            row=1, column=2, columnspan=2, padx=5, pady=5, sticky="nsew"
        )

        self.tune_option_menu = tune_option_menu
        self.profile_option_menu = profile_option_menu
        self.level_option_menu = level_option_menu

        # Multipass
        ctk.CTkLabel(right_column_frame, text="Multipass:").grid(
            row=0, column=0, sticky="w", padx=5, pady=2
        )
        multipass_option_menu = ctk.CTkOptionMenu(
            right_column_frame,
            variable=self.multipass,
            values=["disabled", "qres", "fullres"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        multipass_option_menu.grid(row=0, column=1, sticky="ew", padx=5, pady=2)

        # Rate-Control
        ctk.CTkLabel(right_column_frame, text="Rate-Control:").grid(
            row=1, column=0, sticky="w", padx=5, pady=2
        )
        rc_option_menu = ctk.CTkOptionMenu(
            right_column_frame,
            variable=self.rc,
            values=["vbr", "cbr"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        rc_option_menu.grid(row=1, column=1, sticky="ew", padx=5, pady=2)

        # Lookahead Level
        self.lookahead_level_label = ctk.CTkLabel(
            right_column_frame, text="Lookahead Level:"
        )
        self.lookahead_level_menu = ctk.CTkOptionMenu(
            right_column_frame,
            variable=self.lookahead_level,
            values=["-1", "0", "1", "2", "3"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )

        self.lookahead_level_label.grid(row=2, column=0, sticky="w", padx=5, pady=2)
        self.lookahead_level_menu.grid(row=2, column=1, sticky="ew", padx=5, pady=2)

        # Checkboxes
        ctk.CTkCheckBox(
            right_column_frame,
            text="Spatial AQ",
            variable=self.spatial_aq,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        ).grid(row=0, column=2, columnspan=2, sticky="w", padx=5, pady=2)

        ctk.CTkCheckBox(
            right_column_frame,
            text="Temporal AQ",
            variable=self.temporal_aq,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        ).grid(row=1, column=2, columnspan=2, sticky="w", padx=5, pady=2)

        ctk.CTkCheckBox(
            right_column_frame,
            text="Strict GOP",
            variable=self.strict_gop,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        ).grid(row=2, column=2, columnspan=2, sticky="w", padx=5, pady=2)

        ctk.CTkCheckBox(
            right_column_frame,
            text="No-Scenecut",
            variable=self.no_scenecut,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        ).grid(row=3, column=2, columnspan=2, sticky="w", padx=5, pady=2)

        ctk.CTkCheckBox(
            right_column_frame,
            text="Weighted Prediction",
            variable=self.weighted_pred,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        ).grid(row=3, column=2, columnspan=2, sticky="w", padx=5, pady=2)

        ctk.CTkCheckBox(
            right_column_frame,
            text="High Bit Depth",
            variable=self.highbitdepth,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        ).grid(row=4, column=2, columnspan=2, sticky="w", padx=5, pady=2)

        # FPS and Scaling
        fps_scale_frame_toggle = ctk.CTkCheckBox(
            main_frame,
            text="FPS and Scaling Settings",
            variable=self.enable_fps_scale_options,
            command=self._toggle_fps_scale_options_frame,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        fps_scale_frame_toggle.grid(row=7, column=0, sticky="w", padx=10, pady=5)

        self.fps_scale_options_frame = ctk.CTkFrame(main_frame, fg_color=SECONDARY_BG)

        # FPS
        ctk.CTkLabel(self.fps_scale_options_frame, text="FPS:").grid(
            row=1, column=0, sticky="w", padx=10, pady=5
        )
        fps_options = [
            ("Source", "source"),
            ("60", "60"),
            ("50", "50"),
            ("30", "30"),
            ("23.976", "24000/1001"),
        ]

        for i, (text, value) in enumerate(fps_options):
            rb = ctk.CTkRadioButton(
                self.fps_scale_options_frame,
                text=text,
                variable=self.fps_option,
                value=value,
                command=self._toggle_custom_fps_entry,
                fg_color=ACCENT_GREEN,
                hover_color=HOVER_GREEN,
            )
            rb.grid(row=1, column=i + 1, sticky="w", padx=2)

        rb_custom = ctk.CTkRadioButton(
            self.fps_scale_options_frame,
            text="Custom",
            variable=self.fps_option,
            value="custom",
            command=self._toggle_custom_fps_entry,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        rb_custom.grid(row=2, column=1, sticky="w", padx=2, pady=(10, 10))

        self.custom_fps_label = ctk.CTkLabel(
            self.fps_scale_options_frame, text="Custom FPS:"
        )
        self.custom_fps_label.grid(row=2, column=1, sticky="w", padx=2)

        self.custom_fps_entry = ctk.CTkEntry(
            self.fps_scale_options_frame,
            textvariable=self.custom_fps,
            width=80,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self.custom_fps_entry.grid(row=2, column=2, sticky="w", padx=5)
        self._toggle_custom_fps_entry()

        # Video Format
        ctk.CTkLabel(self.fps_scale_options_frame, text="Video Format:").grid(
            row=3, column=0, sticky="w", padx=10, pady=2
        )
        video_format_options = [
            ("Source", "source"),
            ("HD", "1280"),
            ("FHD", "1920"),
            ("QHD", "2560"),
            ("4K", "3840"),
            ("Custom", "custom"),
        ]

        for i, (text, value) in enumerate(video_format_options):
            rb = ctk.CTkRadioButton(
                self.fps_scale_options_frame,
                text=text,
                variable=self.video_format_option,
                value=value,
                command=self._toggle_custom_video_width_entry,
                fg_color=ACCENT_GREEN,
                hover_color=HOVER_GREEN,
            )
            if value == "custom":
                rb.grid(row=4, column=1, sticky="w", padx=2, pady=(10, 2))
            else:
                rb.grid(row=3, column=i + 1, sticky="w", padx=2)

        self.custom_video_width_label = ctk.CTkLabel(
            self.fps_scale_options_frame, text="Custom Width:"
        )
        self.custom_video_width_entry = ctk.CTkEntry(
            self.fps_scale_options_frame,
            textvariable=self.custom_video_width,
            width=80,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self._toggle_custom_video_width_entry()

        # Interpolation
        ctk.CTkLabel(self.fps_scale_options_frame, text="Interpolation Algo:").grid(
            row=5, column=0, sticky="w", padx=10, pady=10
        )

        self.interpolation_description = ctk.StringVar(
            value="Smooth and balanced quality."
        )
        self.interpolation_algo.trace_add(
            "write", lambda *args: self._update_interpolation_description()
        )

        interp_option_menu = ctk.CTkOptionMenu(
            self.fps_scale_options_frame,
            variable=self.interpolation_algo,
            values=["bilinear", "bicubic", "neighbor", "area", "lanczos", "spline"],
            fg_color=PRIMARY_BG,
            button_color=ACCENT_GREEN,
            button_hover_color=HOVER_GREEN,
            dropdown_fg_color=SECONDARY_BG,
            dropdown_hover_color=ACCENT_GREEN,
        )
        interp_option_menu.grid(row=5, column=1, sticky="ew", padx=5, pady=2)

        ctk.CTkLabel(
            self.fps_scale_options_frame,
            textvariable=self.interpolation_description,
            text_color=PLACEHOLDER_COLOR,
            wraplength=100,
            anchor="w",
            justify="left",
        ).grid(row=5, column=2, sticky="w", padx=5)

        # Audio Settings Toggle
        audio_frame_toggle = ctk.CTkCheckBox(
            main_frame,
            text="Audio Settings",
            variable=self.enable_audio_options,
            command=self._toggle_audio_options_frame,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        audio_frame_toggle.grid(row=9, column=0, sticky="w", padx=10, pady=5)

        # Audio Frame
        self.audio_frame = ctk.CTkFrame(main_frame, fg_color=SECONDARY_BG)
        audio_options_subframe = ctk.CTkFrame(self.audio_frame, fg_color="transparent")
        audio_options_subframe.pack(fill="x", padx=10, pady=5)

        audio_options = [
            ("Disable", "disable"),
            ("Source", "copy"),
            ("AAC 96k", "aac_96k"),
            ("AAC 160k", "aac_160k"),
            ("AAC 256k", "aac_256k"),
        ]

        for i, (text, value) in enumerate(audio_options):
            rb = ctk.CTkRadioButton(
                audio_options_subframe,
                text=text,
                variable=self.audio_option,
                value=value,
                command=self._toggle_custom_abitrate,
                fg_color=ACCENT_GREEN,
                hover_color=HOVER_GREEN,
            )
            rb.grid(row=0, column=i, sticky="w", padx=10, pady=10)

        self.custom_audio_rb = ctk.CTkRadioButton(
            audio_options_subframe,
            text="Custom",
            variable=self.audio_option,
            value="custom",
            command=self._toggle_custom_abitrate,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        self.custom_audio_rb.grid(row=1, column=0, sticky="w", padx=10, pady=2)

        self.custom_abitrate_entry = ctk.CTkEntry(
            audio_options_subframe,
            textvariable=self.custom_abitrate,
            width=80,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self._toggle_custom_abitrate()

        # Additional Options
        additional_options_toggle = ctk.CTkCheckBox(
            main_frame,
            text="Additional Options (Trimming)",
            variable=self.enable_additional_options,
            command=self._toggle_additional_options_frame,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        additional_options_toggle.grid(row=11, column=0, sticky="w", padx=10, pady=5)
        self.additional_options_frame = ctk.CTkFrame(main_frame, fg_color=SECONDARY_BG)

        # Options
        ctk.CTkLabel(self.additional_options_frame, text="Add FF Options:").grid(
            row=1, column=0, sticky="w", padx=10, pady=10
        )
        self.additional_options_entry = ctk.CTkEntry(
            self.additional_options_frame,
            textvariable=self.additional_options,
            width=450,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self.additional_options_entry.grid(row=1, column=1, sticky="ew", padx=5, pady=2)
        self.additional_options_entry.insert(0, self.additional_options_placeholder)
        self.additional_options_entry.configure(text_color=PLACEHOLDER_COLOR)
        self.additional_options_entry.bind("<FocusIn>", self._on_options_entry_focus_in)
        self.additional_options_entry.bind(
            "<FocusOut>", self._on_options_entry_focus_out
        )

        ctk.CTkButton(
            self.additional_options_frame,
            text="?",
            width=30,
            command=lambda: self._show_help_window("NVENC Encoder Options", "encoder"),
        ).grid(row=1, column=2, padx=(0, 10))

        # Trimming controls
        trim_frame = ctk.CTkFrame(self.additional_options_frame, fg_color="transparent")
        trim_frame.grid(row=2, column=0, columnspan=3, sticky="w")

        ctk.CTkLabel(trim_frame, text="Trimming:").grid(
            row=2, column=1, sticky="w", padx=10, pady=10
        )

        # Start time
        ctk.CTkLabel(trim_frame, text="From:").grid(row=2, column=2, padx=(85, 5))
        start_entry = ctk.CTkEntry(
            trim_frame,
            textvariable=self.trim_start,
            width=80,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        start_entry.grid(row=2, column=3, padx=5)
        start_entry.bind(
            "<Return>",
            lambda e: self._validate_and_update_trim_time(self.trim_start, True),
        )

        # End time
        ctk.CTkLabel(trim_frame, text="To:").grid(row=2, column=4, padx=10)
        end_entry = ctk.CTkEntry(
            trim_frame,
            textvariable=self.trim_end,
            width=80,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        end_entry.grid(row=2, column=5)
        end_entry.bind(
            "<Return>",
            lambda e: self._validate_and_update_trim_time(self.trim_end, False),
        )

        # Add validation on focus out
        start_entry.bind(
            "<FocusOut>", lambda e: self._validate_trim_time(self.trim_start, True)
        )
        end_entry.bind(
            "<FocusOut>", lambda e: self._validate_trim_time(self.trim_end, False)
        )

        # Trim button
        ctk.CTkButton(
            trim_frame,
            text="Trim",
            command=self._add_trim_options,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).grid(row=2, column=6, padx=15)

        # Streamcopy checkbox
        ctk.CTkCheckBox(
            trim_frame,
            text="Streamcopy",
            command=lambda: self.audio_option.set("copy"),
            variable=self.trim_streamcopy,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        ).grid(row=2, column=7)

        # Video Filters
        ctk.CTkLabel(
            self.additional_options_frame, text="Add Video Filters (-vf):"
        ).grid(row=4, column=0, sticky="w", padx=10, pady=2)
        self.additional_filter_options_entry = ctk.CTkEntry(
            self.additional_options_frame,
            textvariable=self.additional_filter_options,
            width=450,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self.additional_filter_options_entry.grid(
            row=4, column=1, sticky="ew", padx=5, pady=10
        )
        self.additional_filter_options_entry.insert(
            0, self.additional_filter_options_placeholder
        )
        self.additional_filter_options_entry.configure(text_color=PLACEHOLDER_COLOR)
        self.additional_filter_options_entry.bind(
            "<FocusIn>", self._on_filters_entry_focus_in
        )
        self.additional_filter_options_entry.bind(
            "<FocusOut>", self._on_filters_entry_focus_out
        )

        ctk.CTkButton(
            self.additional_options_frame,
            text="?",
            width=30,
            command=lambda: self._show_help_window("FFmpeg Video Filters", "filters"),
        ).grid(row=4, column=2, padx=(0, 10))

        # Audio Filters
        ctk.CTkLabel(
            self.additional_options_frame, text="Add Audio Filters (-af):"
        ).grid(row=5, column=0, sticky="w", padx=10, pady=2)
        self.additional_audio_filter_options_entry = ctk.CTkEntry(
            self.additional_options_frame,
            textvariable=self.additional_audio_filter_options,
            width=450,
            fg_color=SECONDARY_BG,
            text_color=TEXT_COLOR_W,
        )
        self.additional_audio_filter_options_entry.grid(
            row=5, column=1, sticky="ew", padx=5, pady=10
        )
        self.additional_audio_filter_options_entry.insert(
            0, self.additional_audio_filter_options_placeholder
        )
        self.additional_audio_filter_options_entry.configure(
            text_color=PLACEHOLDER_COLOR
        )
        self.additional_audio_filter_options_entry.bind(
            "<FocusIn>", self._on_audio_filters_entry_focus_in
        )
        self.additional_audio_filter_options_entry.bind(
            "<FocusOut>", self._on_audio_filters_entry_focus_out
        )

        # Quick filter buttons
        quick_buttons_frame = ctk.CTkFrame(
            self.additional_options_frame, fg_color="transparent"
        )
        quick_buttons_frame.grid(
            row=6, column=0, columnspan=3, sticky="w", padx=10, pady=(0, 10)
        )

        ctk.CTkButton(
            quick_buttons_frame,
            text="Speed up X2",
            command=lambda: self._set_speed_filter("2.0"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(0, 10))

        ctk.CTkButton(
            quick_buttons_frame,
            text="Slow down X2",
            command=lambda: self._set_speed_filter("0.5"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left")

        # Sharpness button
        ctk.CTkButton(
            quick_buttons_frame,
            text="Sharpness",
            command=lambda: self._add_video_filter("unsharp=5:5:1.15:3:3:0.0"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(10, 10))

        # Saturation button
        ctk.CTkButton(
            quick_buttons_frame,
            text="Saturation",
            command=lambda: self._add_video_filter("eq=saturation=1.15"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left")

        # Denoise button
        ctk.CTkButton(
            quick_buttons_frame,
            text="Denoise",
            command=lambda: self._add_video_filter("hqdn3d=2:1:3:3"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(10, 0))

        # Reset button
        ctk.CTkButton(
            quick_buttons_frame,
            text="Clear",
            command=self._clear_all_filters,
            fg_color=ACCENT_RED,
            hover_color="#FF3333",
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(10, 0))

        quick_buttons_frame_2 = ctk.CTkFrame(
            self.additional_options_frame, fg_color="transparent"
        )
        quick_buttons_frame_2.grid(
            row=7, column=0, columnspan=3, sticky="w", padx=10, pady=(0, 10)
        )

        # FPS passtrough button
        ctk.CTkButton(
            quick_buttons_frame_2,
            text="FPS Pass",
            command=lambda: self._add_additional_option("-fps_mode passthrough"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(0, 10))

        # Frame drop threshold button
        ctk.CTkButton(
            quick_buttons_frame_2,
            text="Drop thresh",
            command=lambda: self._add_additional_option("-frame_drop_threshold 0.5"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left")

        # Gamma RGB
        ctk.CTkButton(
            quick_buttons_frame_2,
            text="Gamma RGB",
            command=lambda: self._add_video_filter(
                "eq=gamma_r=1.0:gamma_g=1.0:gamma_b=1.0:gamma_weight=1.0"
            ),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(10, 10))

        # Brightness button
        ctk.CTkButton(
            quick_buttons_frame_2,
            text="Brightness",
            command=lambda: self._add_video_filter("eq=brightness=-0.15"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left")

        # Audio fix button
        ctk.CTkButton(
            quick_buttons_frame_2,
            text="Audio fix",
            command=lambda: self._add_audio_filter("loudnorm=I=-16:TP=-1.5:LRA=11"),
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(10, 0))

        # Add HDR to SDR button here
        ctk.CTkButton(
            quick_buttons_frame_2,
            text="HDR to SDR",
            command=self._apply_hdr_to_sdr,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            width=100,
        ).pack(side="left", padx=(10, 0))

        # Presets Section
        presets_frame_toggle = ctk.CTkCheckBox(
            main_frame,
            text="Presets",
            variable=self.enable_presets,
            command=self._toggle_presets_frame,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
        )
        presets_frame_toggle.grid(row=13, column=0, sticky="w", padx=10, pady=5)
        self.presets_frame = ctk.CTkFrame(main_frame, fg_color=SECONDARY_BG)

        preset_options = [
            ("Default (Reset)", "none"),
            ("FHD Fast", "fhdf"),
            ("FHD Quality", "fhdq"),
            ("HD Fast", "hdf"),
            ("HD Quality", "hdq"),
        ]

        for i, (text, value) in enumerate(preset_options):
            rb = ctk.CTkRadioButton(
                self.presets_frame,
                text=text,
                variable=self.selected_preset,
                value=value,
                command=lambda v=value: self._apply_preset(v),
                fg_color=ACCENT_GREEN,
                hover_color=HOVER_GREEN,
            )
            rb.grid(row=1, column=i, padx=10, pady=5, sticky="w")

        # Preset Indicator
        self.preset_indicator = ctk.CTkLabel(
            self.presets_frame, text="No preset selected", text_color=PLACEHOLDER_COLOR
        )
        self.preset_indicator.grid(
            row=3, column=0, columnspan=2, padx=10, pady=5, sticky="w"
        )

        ctk.CTkLabel(main_frame, textvariable=self.estimated_file_size).grid(
            row=16, column=0, sticky="w", padx=5, pady=2
        )

        # Output
        ctk.CTkLabel(
            main_frame, textvariable=self.ffmpeg_output, wraplength=600, justify="left"
        ).grid(row=18, column=0, columnspan=4, pady=(10, 0), padx=10)

        # Progress
        self.progress_frame = ctk.CTkFrame(main_frame, fg_color="transparent")
        self.progress_frame.grid(
            row=17, column=0, columnspan=4, pady=(10, 0), padx=10, sticky="ew"
        )
        self.progress_bar = ctk.CTkProgressBar(
            self.progress_frame,
            variable=self.progress_value,
            fg_color=SECONDARY_BG,
            progress_color=ACCENT_GREEN,
        )
        self.progress_bar.pack(fill="x", expand=True)
        self.progress_label = ctk.CTkLabel(self.progress_frame, text="0%")
        self.progress_label.pack()
        self.progress_frame.grid_remove()

        # Play buttons
        self.play_buttons_frame = ctk.CTkFrame(self.button_frame, fg_color=PRIMARY_BG)
        self.play_buttons_frame.pack(fill="x", pady=(0, 5))

        self.play_input_button = ctk.CTkButton(
            self.play_buttons_frame,
            text="Play Input File",
            fg_color=ACCENT_GREY,
            hover_color=HOVER_GREY,
            text_color=TEXT_COLOR_B,
            height=40,
        )
        self.play_input_button.pack(side="left", expand=True, fill="x", padx=(0, 2))
        self.play_input_button.configure(command=self._play_input_file)

        self.play10s_button = ctk.CTkButton(
            self.play_buttons_frame,
            text="Play 10s Preview",
            fg_color=ACCENT_GREY,
            hover_color=HOVER_GREY,
            text_color=TEXT_COLOR_B,
            height=40,
        )
        self.play10s_button.pack(side="left", expand=True, fill="x", padx=2)
        self.play10s_button.configure(command=self._create_10s_preview)

        self.play_output_button = ctk.CTkButton(
            self.play_buttons_frame,
            text="Play Output File",
            fg_color=ACCENT_GREY,
            hover_color=HOVER_GREY,
            text_color=TEXT_COLOR_B,
            height=40,
        )
        self.play_output_button.pack(side="left", expand=True, fill="x", padx=(2, 0))
        self.play_output_button.configure(command=self._play_output_file)

        # Convert Button
        self.convert_button = ctk.CTkButton(
            self.button_frame,
            text="Convert",
            command=self._toggle_conversion,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
            height=40,
            font=("", 14, "bold"),
        )
        self.convert_button.pack(fill="x", pady=5)

        # Status
        ctk.CTkLabel(main_frame, textvariable=self.status_text).grid(
            row=16, column=0, columnspan=4, pady=5
        )

        # Initialize
        self._toggle_encoder_options_frame()
        self._toggle_fps_scale_options_frame()
        self._toggle_audio_options_frame()
        self._toggle_additional_options_frame()
        self._toggle_presets_frame()

    def _play_input_file(self):
        input_path = self.input_file.get()

        # Check if file is selected
        if (
            not input_path
            or input_path
            == "Drag and drop a video file here or use the 'Browse' button."
        ):
            messagebox.showerror(
                "Error", "Please select input file using Browse button"
            )
            return

        # Normalize path and check if file exists
        normalized_path = os.path.normpath(input_path)
        if not os.path.exists(normalized_path):
            messagebox.showerror("Error", "Input file does not exist")
            return

        try:
            # Open file in system default player
            os.startfile(normalized_path)
        except Exception as e:
            messagebox.showerror("Error", f"Could not play file: {str(e)}")

    def _play_output_file(self):
        output_path = self.output_file.get()

        # Check if output file is specified
        if not output_path:
            messagebox.showerror("Error", "Output file is not specified")
            return

        # Normalize path and check if file exists
        normalized_path = os.path.normpath(output_path)
        if not os.path.exists(normalized_path):
            messagebox.showerror("Error", "Output file does not exist")
            return

        try:
            # Open file in system default player
            os.startfile(normalized_path)
        except Exception as e:
            messagebox.showerror("Error", f"Could not play file: {str(e)}")

    def _toggle_constant_qp_mode(self):
        if self.constant_qp_mode.get():
            # Switch to Constant QP mode - replace bitrate with quality
            self.bitrate_label.configure(text="Quality Level:")
            self.bitrate_entry.configure(textvariable=self.quality_level)
            self.bitrate_entry.delete(0, "end")
            self.bitrate_entry.insert(0, "30")

            # Disable file size estimation
            self.estimated_file_size.set("Estimated size: Not available for CQP")
        else:
            # Switch back to normal mode - replace quality with bitrate
            self.bitrate_label.configure(text="Video Bitrate (k):")
            self.bitrate_entry.configure(textvariable=self.bitrate)
            self.bitrate_entry.delete(0, "end")
            self.bitrate_entry.insert(0, "6000")

            # Enable file size estimation
            self._calculate_estimated_size()

    def _show_output_command(self):
        if not self.input_file.get():
            messagebox.showerror("Error", "Please select an input file first.")
            return

        try:
            command = self._build_ffmpeg_command(preview=True)
            output_window = ctk.CTkToplevel(self.master)
            output_window.title("FFmpeg Command Preview - Editable")
            output_window.geometry("850x550")
            output_window.transient(self.master)
            output_window.grab_set()
            output_window.configure(fg_color=PRIMARY_BG)

            text_frame = ctk.CTkFrame(output_window)
            text_frame.pack(fill="both", expand=True, padx=10, pady=10)

            ctk.CTkLabel(
                text_frame,
                text="You can edit the command below:",
            ).pack(pady=(0, 5))
            text_frame.configure(fg_color=PRIMARY_BG)

            self.command_textbox = ctk.CTkTextbox(
                text_frame, wrap="word", font=("Consolas", 14), height=300
            )
            self.command_textbox.pack(fill="both", expand=True, padx=5, pady=5)
            self.command_textbox.insert("1.0", " ".join(command))

            button_frame = ctk.CTkFrame(text_frame)
            button_frame.pack(fill="x", pady=10)
            button_frame.configure(fg_color=PRIMARY_BG)

            ctk.CTkButton(
                button_frame,
                text="Copy to Clipboard",
                command=self._copy_command_to_clipboard,
                fg_color=ACCENT_GREEN,
                hover_color=HOVER_GREEN,
                text_color=TEXT_COLOR_B,
                width=150,
            ).pack(side="left", padx=5)

            ctk.CTkButton(
                button_frame,
                text="Apply Changes",
                command=lambda: self._apply_command_changes(output_window),
                fg_color=ACCENT_GREEN,
                hover_color=HOVER_GREEN,
                text_color=TEXT_COLOR_B,
                width=150,
            ).pack(side="left", padx=5)

            ctk.CTkButton(
                button_frame,
                text="Close",
                command=output_window.destroy,
                fg_color=ACCENT_GREY,
                hover_color=HOVER_GREY,
                text_color=TEXT_COLOR_B,
                width=100,
            ).pack(side="right", padx=5)

        except Exception as e:
            messagebox.showerror("Error", f"Could not generate command: {str(e)}")

    def _copy_command_to_clipboard(self):
        command = self.command_textbox.get("1.0", "end-1c")

        try:
            # Parse command into parts while preserving quotes
            parts = split(command, posix=False)

            # Rebuild with quotes around file paths
            quoted_parts = []
            i = 0
            while i < len(parts):
                part = parts[i]

                # Check if this is a flag (starts with -)
                if part.startswith("-"):
                    quoted_parts.append(part)
                    i += 1
                    continue

                # Check if previous part was -i flag (input file)
                if i > 0 and parts[i - 1] == "-i":
                    # Add quotes around input file path
                    if not (part.startswith('"') and part.endswith('"')):
                        part = f'"{part}"'
                    quoted_parts.append(part)
                    i += 1
                    continue

                # Check if this looks like an output file path
                # (last argument or contains path separators)
                is_output_file = (i == len(parts) - 1) or (
                    "\\" in part
                    or "/" in part
                    or part.endswith(
                        (".mp4", ".mkv", ".avi", ".mov", ".flv", ".wmv", ".webm")
                    )
                )

                if is_output_file:
                    # Add quotes around output file path
                    if not (part.startswith('"') and part.endswith('"')):
                        part = f'"{part}"'
                    quoted_parts.append(part)
                    i += 1
                else:
                    # Regular argument (not a file path)
                    quoted_parts.append(part)
                    i += 1

            command_with_quotes = " ".join(quoted_parts)
            self.master.clipboard_clear()
            self.master.clipboard_append(command_with_quotes)

        except Exception:
            # Fallback: simple regex-based quoting for file paths

            command_with_quotes = sub(
                r'(-i\s+)([^"\s]+)',
                r'\1"\2"',
                command,  # Quote input files
            )
            command_with_quotes = sub(
                r"(\s)([A-Za-z]:\\[^ ]+\.\w{2,4}|/[^ ]+\.\w{2,4})(\s|$)",
                r'\1"\2"\3',
                command_with_quotes,  # Quote output files
            )
            self.master.clipboard_clear()
            self.master.clipboard_append(command_with_quotes)
            messagebox.showinfo("Copied", "Command copied to clipboard!")

    def _apply_command_changes(self, window):
        new_command = self.command_textbox.get("1.0", "end-1c").strip()
        if not new_command:
            messagebox.showwarning("Warning", "Command is empty!")
            return

        try:
            args = split(new_command)

            if len(args) < 3:
                raise ValueError("Command too short - not a valid FFmpeg command")

            has_input = False
            has_output = False
            for i, arg in enumerate(args):
                if arg == "-i" and i < len(args) - 1:
                    has_input = True
                if not arg.startswith("-") and i > 0 and args[i - 1] != "-i":
                    has_output = True

            if not has_input:
                raise ValueError("Missing input file (-i option)")
            if not has_output:
                raise ValueError("Missing output file")

            self.ffmpeg_output.set("Custom command applied: " + " ".join(args))
            window.destroy()

        except Exception as e:
            messagebox.showerror(
                "Error",
                f"Invalid command: {str(e)}\n\nPlease check:\n"
                "1. Input file (-i option)\n"
                "2. Output file path\n"
                "3. Valid FFmpeg arguments",
            )

    def _build_ffmpeg_command(self, preview=False):
        # Use custom path if specified, otherwise use found path
        ffmpeg_path = (
            self.ffmpeg_custom_path.get()
            if self.ffmpeg_custom_path.get()
            and self.ffmpeg_custom_path.get() != self.ffmpeg_path_placeholder
            else self.ffmpeg_path
        )
        if not ffmpeg_path:
            raise ValueError("FFmpeg path is not specified")

        input_f = self.input_file.get()
        if not self.output_file.get():
            raise ValueError("Please set an output file")
        output_f = self.output_file.get()

        if not input_f or not output_f:
            raise ValueError("Please fill in all main fields.")

        # Basic command structure
        command = [
            ffmpeg_path,
            "-hwaccel",
            "cuda",
            "-hwaccel_output_format",
            "p010le" if hasattr(self, "hdr_mode") and self.hdr_mode else "nv12",
            "-threads",
            "4",
            "-y",
            "-i",
            input_f,
        ]

        # Handle Streamcopy option
        if self.trim_streamcopy.get() and not preview:
            command.extend(["-c:v", "copy"])

            # Audio handling - allow user selection even in copy mode
            audio_opt = self.audio_option.get()
            if audio_opt == "disable":
                command.append("-an")
            elif audio_opt == "copy":
                command.extend(["-c:a", "copy"])
            elif audio_opt == "aac_96k":
                command.extend(["-c:a", "aac", "-b:a", "96k"])
            elif audio_opt == "aac_160k":
                command.extend(["-c:a", "aac", "-b:a", "160k"])
            elif audio_opt == "aac_256k":
                command.extend(["-c:a", "aac", "-b:a", "256k"])
            elif audio_opt == "custom":
                abitrate_val = self.custom_abitrate.get()
                try:
                    int(abitrate_val)
                    command.extend(["-c:a", "aac", "-b:a", f"{abitrate_val}k"])
                except ValueError:
                    raise ValueError("Custom audio bitrate must be a number.")

            # Add FF Options field should still be appended
            if self.enable_additional_options.get() and not preview:
                add_val = self.additional_options.get().strip()
                if add_val and add_val != self.additional_options_placeholder:
                    command.extend(add_val.split())

            command.append(output_f)
            return command

        # Normal encoding path
        if self.constant_qp_mode.get():
            # Constant QP mode
            quality_val = self.quality_level.get()
            try:
                quality_int = int(quality_val)
                if not (0 <= quality_int <= 51):
                    raise ValueError("Quality level must be between 0 and 51")
            except ValueError:
                raise ValueError("Quality level must be a number between 0 and 51")
        else:
            # Normal VBR/CBR mode
            bitrate_val = self.bitrate.get()
            try:
                bitrate_int = int(bitrate_val)
                maxrate_val = (bitrate_int * 12 + 9) // 10
                bufsize_val = maxrate_val * 2
            except ValueError:
                raise ValueError("Video bitrate must be a number.")

        profile_map = {
            "main": "nv12",
            "main10": "p010le",
            "rext": "none",
        }
        pix_fmt_val = profile_map.get(self.profile.get(), "nv12")
        if pix_fmt_val != "none":
            command.extend(["-pix_fmt:v", pix_fmt_val])

        vf_filters = []
        if self.enable_fps_scale_options.get():
            fps_num = self.fps_option.get()
            if fps_num == "custom":
                fps_num = self.custom_fps.get()
                if not fps_num:
                    raise ValueError("Please specify custom FPS.")
            if fps_num != "source":
                vf_filters.append(f"fps={fps_num}")
            scale_width = self.video_format_option.get()
            if scale_width == "custom":
                scale_width = self.custom_video_width.get()
                if not scale_width:
                    raise ValueError("Please specify custom video width.")
            if scale_width != "source":
                interp_flag = self.interpolation_algo.get()
                vf_filters.append(f"scale={scale_width}:-2:flags={interp_flag}")

        if self.enable_additional_options.get():
            addvf_val = self.additional_filter_options.get().strip()
            if addvf_val and addvf_val != self.additional_filter_options_placeholder:
                vf_filters.append(addvf_val)

        if vf_filters:
            command.extend(["-vf", ",".join(vf_filters)])

        # Add encoder settings based on mode
        command.extend(
            [
                "-c:v",
                (
                    "hevc_nvenc"
                    if self.video_codec.get() == "hevc"
                    else (
                        "av1_nvenc" if self.video_codec.get() == "av1" else "h264_nvenc"
                    )
                ),
                "-preset:v",
                self.preset.get(),
                "-tune:v",
                self.tune.get(),
            ]
        )

        if self.level.get() != "auto":
            command.extend(["-level:v", self.level.get()])

        if self.video_codec.get() in ("hevc", "av1"):
            command.extend(["-tier:v", self.tier.get()])

        else:
            command.extend(["-coder:v", self.coder.get()])

        # Add rate control parameters based on mode
        if self.constant_qp_mode.get():
            command.extend(
                [
                    "-rc:v",
                    "constqp",
                    "-qp:v",
                    quality_val,
                ]
            )
        else:
            command.extend(
                [
                    "-multipass:v",
                    self.multipass.get(),
                    "-rc:v",
                    self.rc.get(),
                    "-b:v",
                    f"{bitrate_val}k",
                    "-maxrate:v",
                    f"{maxrate_val}k",
                    "-bufsize:v",
                    f"{bufsize_val}k",
                ]
            )

        command.extend(
            [
                "-profile:v",
                self.profile.get(),
                "-strict_gop:v",
                "1" if self.strict_gop.get() else "0",
                "-spatial-aq:v",
                "1" if self.spatial_aq.get() else "0",
                "-temporal-aq:v",
                "1" if self.temporal_aq.get() else "0",
                "-no-scenecut:v",
                "1" if self.no_scenecut.get() else "0",
                "-weighted_pred:v",
                "1" if self.weighted_pred.get() else "0",
                *(["-bf", "0"] if self.weighted_pred.get() else []),
                "-highbitdepth:v",
                "1" if self.highbitdepth.get() else "0",
                "-lookahead_level:v",
                self.lookahead_level.get(),
            ]
        )

        if self.enable_additional_options.get():
            add_val = self.additional_options.get().strip()
            if add_val and add_val != self.additional_options_placeholder:
                command.extend(add_val.split())

            add_af_val = self.additional_audio_filter_options.get().strip()
            if (
                add_af_val
                and add_af_val != self.additional_audio_filter_options_placeholder
            ):
                command.extend(["-af", add_af_val])

        audio_opt = self.audio_option.get()
        if audio_opt == "disable":
            command.append("-an")
        elif audio_opt == "copy":
            command.extend(["-c:a", "copy"])
        elif audio_opt == "aac_96k":
            command.extend(["-c:a", "aac", "-b:a", "96k"])
        elif audio_opt == "aac_160k":
            command.extend(["-c:a", "aac", "-b:a", "160k"])
        elif audio_opt == "aac_256k":
            command.extend(["-c:a", "aac", "-b:a", "256k"])
        elif audio_opt == "custom":
            abitrate_val = self.custom_abitrate.get()
            try:
                int(abitrate_val)
                command.extend(["-c:a", "aac", "-b:a", f"{abitrate_val}k"])
            except ValueError:
                raise ValueError("Custom audio bitrate must be a number.")

        command.append(output_f)

        return command

    def _cleanup_preview_files(self):
        """Clean up preview temporary files"""
        for file_path in self.preview_temp_files:
            try:
                if os.path.exists(file_path):
                    os.remove(file_path)
            except Exception as e:
                print(f"Error deleting temp file {file_path}: {e}")
        self.preview_temp_files = []

    def _on_close(self):
        """Application close handler"""
        self._cleanup_preview_files()
        self.master.quit()

    def _start_conversion(self):
        try:
            command = self._build_ffmpeg_command()
        except Exception as e:
            messagebox.showerror("Error", str(e))
            return

        self.progress_value.set(0.0)
        self.progress_label.configure(text="0%")
        self.progress_frame.grid()
        self.convert_button.configure(
            text="Cancel", fg_color=ACCENT_RED, hover_color="#FF3333"
        )
        self.is_converting = True

        self.status_text.set("Conversion in progress...")
        self.ffmpeg_output.set("Starting conversion...")
        self.conversion_thread = Thread(target=self._run_ffmpeg, args=(command,))
        self.conversion_thread.start()

    def _set_speed_filter(self, speed_factor):
        try:
            speed = float(speed_factor)
            if speed <= 0:
                raise ValueError("Speed factor must be positive")

            video_filter = f"setpts={1 / speed}*PTS"
            self.additional_filter_options.set(video_filter)
            self.additional_filter_options_entry.configure(text_color=TEXT_COLOR_W)
            if self.audio_option.get() in ("disable", "copy"):
                self.audio_option.set("aac_96k")

            if self.audio_option.get() != "disable":
                audio_filter = f"atempo={speed}"
                self.additional_audio_filter_options.set(audio_filter)
                self.additional_audio_filter_options_entry.configure(
                    text_color=TEXT_COLOR_W
                )

        except ValueError as e:
            messagebox.showerror("Error", f"Invalid speed value: {e}")

    def _add_video_filter(self, filter_str):
        current_filters = self.additional_filter_options.get()
        if current_filters == self.additional_filter_options_placeholder:
            current_filters = ""

        existing_filters = [f.strip() for f in current_filters.split(",") if f.strip()]

        if filter_str in existing_filters:
            return

        if existing_filters:
            new_filters = ",".join(existing_filters + [filter_str])
        else:
            new_filters = filter_str

        self.additional_filter_options.set(new_filters)
        self.additional_filter_options_entry.configure(text_color=TEXT_COLOR_W)

    def _add_audio_filter(self, filter_str):
        if self.audio_option.get() in ("disable", "copy"):
            self.audio_option.set("aac_160k")

        current_filters = self.additional_audio_filter_options.get()
        if current_filters == self.additional_audio_filter_options_placeholder:
            current_filters = ""

        existing_filters = [f.strip() for f in current_filters.split(",") if f.strip()]

        if filter_str in existing_filters:
            return

        if existing_filters:
            new_filters = ",".join(existing_filters + [filter_str])
        else:
            new_filters = filter_str

        self.additional_audio_filter_options.set(new_filters)
        self.additional_audio_filter_options_entry.configure(text_color=TEXT_COLOR_W)

    def _add_additional_option(self, option_str):
        current_options = self.additional_options.get()

        if current_options == self.additional_options_placeholder:
            current_options = ""

        existing_options = current_options.split()

        option_name = option_str.split()[0]
        for i, opt in enumerate(existing_options):
            if opt == option_name:
                existing_options[i: i + 2] = option_str.split()
                break
        else:
            existing_options.extend(option_str.split())

        new_options = " ".join(existing_options).strip()
        self.additional_options.set(new_options)
        self.additional_options_entry.configure(text_color=TEXT_COLOR_W)

    def _clear_all_filters(self):
        self.additional_options.set("")
        self.additional_filter_options.set("")
        self.additional_audio_filter_options.set("")

        self.additional_options_entry.delete(0, "end")
        self.additional_options_entry.insert(0, self.additional_options_placeholder)
        self.additional_options_entry.configure(text_color=PLACEHOLDER_COLOR)

        self.additional_filter_options_entry.delete(0, "end")
        self.additional_filter_options_entry.insert(
            0, self.additional_filter_options_placeholder
        )
        self.additional_filter_options_entry.configure(text_color=PLACEHOLDER_COLOR)

        self.additional_audio_filter_options_entry.delete(0, "end")
        self.additional_audio_filter_options_entry.insert(
            0, self.additional_audio_filter_options_placeholder
        )
        self.additional_audio_filter_options_entry.configure(
            text_color=PLACEHOLDER_COLOR
        )

    def _on_input_file_focus_in(self, event):
        current_text = self.input_file.get()
        if (
            current_text
            == "Drag and drop a video file here or use the 'Browse' button."
        ):
            self.input_file_entry.delete(0, "end")
            self.input_file_entry.configure(text_color=TEXT_COLOR_W)

    def _on_input_file_focus_out(self, event):
        current_text = self.input_file.get()
        if not current_text.strip():
            self.input_file_entry.insert(
                0, "Drag and drop a video file here or use the 'Browse' button."
            )
            self.input_file_entry.configure(text_color=PLACEHOLDER_COLOR)

    def _on_ffmpeg_path_focus_in(self, event):
        current_text = self.ffmpeg_custom_path.get()
        if current_text == self.ffmpeg_path_placeholder:
            self.ffmpeg_path_entry.delete(0, "end")
            self.ffmpeg_path_entry.configure(text_color=TEXT_COLOR_W)

    def _on_ffmpeg_path_focus_out(self, event):
        current_text = self.ffmpeg_custom_path.get()
        if not current_text.strip():
            self.ffmpeg_path_entry.insert(0, self.ffmpeg_path_placeholder)
            self.ffmpeg_path_entry.configure(text_color=PLACEHOLDER_COLOR)
        elif current_text == self.ffmpeg_path_placeholder:
            self.ffmpeg_path_entry.configure(text_color=PLACEHOLDER_COLOR)
        else:
            self.ffmpeg_path_entry.configure(text_color=TEXT_COLOR_W)
            # Save the path if it exists
            if os.path.exists(current_text) and os.path.isfile(current_text):
                self._save_ffmpeg_path(current_text)
                self.ffmpeg_path = current_text
                self.ffprobe_path = os.path.join(
                    os.path.dirname(current_text), "ffprobe.exe"
                )
                # Look for ffprobe.exe in the same directory
                ffprobe_path = os.path.join(
                    os.path.dirname(current_text), "ffprobe.exe"
                )
                if os.path.exists(ffprobe_path):
                    self.ffprobe_path = ffprobe_path
                    self.status_text.set("FFmpeg and FFprobe found successfully")
                else:
                    self.ffprobe_path = None
                    self.status_text.set(
                        "FFmpeg found but FFprobe not found in the same directory"
                    )

    def _on_options_entry_focus_in(self, event):
        current_text = self.additional_options.get()
        if current_text == self.additional_options_placeholder:
            self.additional_options_entry.delete(0, "end")
            self.additional_options_entry.configure(text_color=TEXT_COLOR_W)

    def _on_options_entry_focus_out(self, event):
        current_text = self.additional_options.get()
        if not current_text.strip():
            self.additional_options_entry.insert(0, self.additional_options_placeholder)
            self.additional_options_entry.configure(text_color=PLACEHOLDER_COLOR)

    def _on_filters_entry_focus_in(self, event):
        current_text = self.additional_filter_options.get()
        if current_text == self.additional_filter_options_placeholder:
            self.additional_filter_options_entry.delete(0, "end")
            self.additional_filter_options_entry.configure(text_color=TEXT_COLOR_W)

    def _on_filters_entry_focus_out(self, event):
        current_text = self.additional_filter_options.get()
        if not current_text.strip():
            self.additional_filter_options_entry.insert(
                0, self.additional_filter_options_placeholder
            )
            self.additional_filter_options_entry.configure(text_color=PLACEHOLDER_COLOR)

    def _on_audio_filters_entry_focus_in(self, event):
        current_text = self.additional_audio_filter_options.get()
        if current_text == self.additional_audio_filter_options_placeholder:
            self.additional_audio_filter_options_entry.delete(0, "end")
            self.additional_audio_filter_options_entry.configure(text_color=TEXT_COLOR_W)

    def _on_audio_filters_entry_focus_out(self, event):
        current_text = self.additional_audio_filter_options.get()
        if not current_text.strip():
            self.additional_audio_filter_options_entry.insert(
                0, self.additional_audio_filter_options_placeholder
            )
            self.additional_audio_filter_options_entry.configure(
                text_color=PLACEHOLDER_COLOR
            )

    def _update_interpolation_description(self):
        descriptions = {
            "bilinear": "Fast, low-quality scaling.",
            "bicubic": "Smooth and balanced quality.",
            "neighbor": "Blocky, pixelated output.",
            "area": "Soft result for downscaling.",
            "lanczos": "Sharp and high-quality.",
            "spline": "Visually best in theory.",
        }
        current = self.interpolation_algo.get()
        self.interpolation_description.set(descriptions.get(current, ""))

    def _find_executable(self, name):
        script_dir = os.path.dirname(os.path.abspath(__file__))
        exe_in_script_dir = os.path.join(script_dir, name)
        if os.path.exists(exe_in_script_dir):
            return exe_in_script_dir
        try:
            result = subprocess.run(
                ["where", name],
                check=True,
                capture_output=True,
                text=True,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            )
            return result.stdout.strip().split("\n")[0]
        except subprocess.CalledProcessError:
            return None
        except FileNotFoundError:
            return None

    def _save_ffmpeg_path(self, path):
        """Save FFmpeg path to a file in the program directory"""
        try:
            script_dir = os.path.dirname(os.path.abspath(__file__))
            path_file = os.path.join(script_dir, "ffmpeg_path.txt")
            with open(path_file, "w") as file:
                file.write(path)
        except Exception as e:
            print(f"Error saving FFmpeg path: {e}")

    def _load_ffmpeg_path(self):
        """Load FFmpeg path from file if it exists and is valid"""
        try:
            script_dir = os.path.dirname(os.path.abspath(__file__))
            path_file = os.path.join(script_dir, "ffmpeg_path.txt")

            if os.path.exists(path_file):
                with open(path_file, "r") as file:
                    saved_path = file.read().strip()

                if os.path.exists(saved_path) and os.path.isfile(saved_path):
                    self.ffmpeg_path = saved_path
                    ffprobe_path = os.path.join(
                        os.path.dirname(saved_path), "ffprobe.exe"
                    )
                    if os.path.exists(ffprobe_path):
                        self.ffprobe_path = ffprobe_path
                    else:
                        self.ffprobe_path = None
                    return saved_path
        except Exception as e:
            print(f"Error loading FFmpeg path: {e}")
        return None

    def _set_trim_end_to_duration(self):
        if (
            not self.ffprobe_path
            or not self.input_file.get()
            or self.input_file.get().startswith("Drag and drop")
        ):
            return
        try:
            command = [
                self.ffprobe_path,
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                self.input_file.get(),
            ]
            result = subprocess.run(
                command,
                check=True,
                capture_output=True,
                text=True,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            )
            duration = float(result.stdout.strip())
            # convert seconds to hh:mm:ss
            h = int(duration // 3600)
            m = int((duration % 3600) // 60)
            s = int(duration % 60)
            self.trim_end.set(f"{h:02d}:{m:02d}:{s:02d}")
            if hasattr(self, "trim_canvas"):
                self.master.after(100, self._update_trim_slider)
        except Exception as e:
            print(f"Error setting trim_end: {e}")
            self.trim_end.set("00:10:00")

    def _browse_input(self):
        filename = filedialog.askopenfilename(
            title="Select Video File",
            filetypes=(
                ("Video Files", "*.mp4 *.mkv *.avi *.mov *.flv *.wmv *.webm"),
                ("All Files", "*.*"),
            ),
        )
        if filename:
            normalized_path = os.path.normpath(filename)
            self.input_file.set(normalized_path)
            self.input_file_entry.configure(text_color=TEXT_COLOR_W)
            base_name = os.path.splitext(os.path.basename(normalized_path))[0]
            codec_suffix = (
                "_hevc"
                if self.video_codec.get() == "hevc"
                else "_av1"
                if self.video_codec.get() == "av1"
                else "_h264"
            )
            output_path = os.path.normpath(
                os.path.join(
                    os.path.dirname(normalized_path),
                    f"{base_name}{codec_suffix}_custom.mp4",
                )
            )
            self.output_file.set(output_path)
            self.trim_start.set("00:00:00")
            self.total_duration = 0
            self.status_text.set("File selected. Ready for conversion.")
            self._calculate_estimated_size()
            self._set_trim_end_to_duration()

    def _browse_output(self):
        default_name = (
            self.output_file.get()
            if self.output_file.get()
            else "output_hevc_custom.mp4"
        )
        filename = filedialog.asksaveasfilename(
            title="Save As...",
            defaultextension=".mp4",
            initialfile=os.path.basename(default_name),
            initialdir=(
                os.path.dirname(default_name)
                if os.path.dirname(default_name)
                else os.getcwd()
            ),
            filetypes=(
                ("MP4 Files", "*.mp4"),
                ("MKV Files", "*.mkv"),
                ("MOV Files", "*.mov"),
                ("All Files", "*.*"),
            ),
        )
        if filename:
            self.output_file.set(os.path.normpath(filename))

    def _browse_ffmpeg(self):
        filename = filedialog.askopenfilename(
            title="Select FFmpeg Executable",
            filetypes=(("Executable Files", "*.exe"), ("All Files", "*.*")),
        )
        if filename:
            normalized_path = os.path.normpath(filename)
            self.ffmpeg_custom_path.set(normalized_path)
            self.ffmpeg_path_entry.configure(text_color=TEXT_COLOR_W)
            self.ffmpeg_path = normalized_path

            self._save_ffmpeg_path(normalized_path)

            ffprobe_path = os.path.join(os.path.dirname(normalized_path), "ffprobe.exe")
            if os.path.exists(ffprobe_path):
                self.ffprobe_path = ffprobe_path
            else:
                self.ffprobe_path = None

    def _toggle_encoder_options_frame(self):
        if self.enable_encoder_options.get():
            self.encoder_options_frame.grid(
                row=6, column=0, columnspan=4, sticky="ew", padx=10, pady=10
            )
        else:
            self.encoder_options_frame.grid_forget()
        self._update_window_size()

    def _toggle_custom_fps_entry(self):
        if self.fps_option.get() == "custom":
            self.custom_fps_label.grid(row=2, column=0, sticky="w", padx=10)
            self.custom_fps_entry.grid(row=2, column=1, sticky="w", padx=5)
        else:
            self.custom_fps_label.grid_forget()
            self.custom_fps_entry.grid_forget()
        self._update_window_size()

    def _toggle_custom_video_width_entry(self):
        if self.video_format_option.get() == "custom":
            self.custom_video_width_label.grid(row=4, column=0, sticky="w", padx=10)
            self.custom_video_width_entry.grid(row=4, column=1, sticky="w", padx=5)
        else:
            self.custom_video_width_label.grid_forget()
            self.custom_video_width_entry.grid_forget()
        self._update_window_size()

    def _toggle_fps_scale_options_frame(self):
        if self.enable_fps_scale_options.get():
            self.fps_scale_options_frame.grid(
                row=8, column=0, columnspan=4, sticky="ew", padx=10, pady=10
            )
        else:
            self.fps_scale_options_frame.grid_forget()
        self._update_window_size()

    def _toggle_custom_abitrate(self):
        if self.audio_option.get() == "custom":
            self.custom_audio_rb.configure(text="Bitrate (k):")
            self.custom_abitrate_entry.grid(row=1, column=1, sticky="w", padx=5)
        else:
            self.custom_audio_rb.configure(text="Custom")
            self.custom_abitrate_entry.grid_forget()

    def _toggle_audio_options_frame(self):
        if self.enable_audio_options.get():
            self.audio_frame.grid(
                row=10, column=0, columnspan=4, sticky="ew", padx=10, pady=10
            )
        else:
            self.audio_frame.grid_forget()
        self._update_window_size()

    def _toggle_additional_options_frame(self):
        if self.enable_additional_options.get():
            self.additional_options_frame.grid(
                row=12, column=0, columnspan=4, sticky="ew", padx=10, pady=10
            )
            if hasattr(self, "trim_canvas"):
                self.master.after(100, self._update_trim_slider)
        else:
            self.additional_options_frame.grid_forget()
        self._update_window_size()

    def _toggle_conversion(self):
        if self.is_creating_preview:
            messagebox.showerror(
                "Error", "Please wait until preview creation completes"
            )
            return

        if self.is_converting:
            self._cancel_conversion()
        else:
            self._start_conversion()

    def _cancel_conversion(self):
        if self.conversion_process and self.is_converting:
            self.conversion_process.terminate()
            self.is_converting = False
            self.status_text.set("Conversion cancelled")
            self.progress_frame.grid_remove()
            self.convert_button.configure(
                text="Convert", fg_color=ACCENT_GREEN, hover_color=HOVER_GREEN
            )

    def _update_progress(self, line):
        if "time=" in line:
            time_pos = line.find("time=")
            time_str = line[time_pos + 5:].split()[0]
            try:
                h, m, s = time_str.split(":")
                total_seconds = int(h) * 3600 + int(m) * 60 + float(s)
                if not hasattr(self, "total_duration"):
                    self._get_video_duration()
                if hasattr(self, "total_duration") and self.total_duration > 0:
                    progress = total_seconds / self.total_duration
                    self.progress_value.set(progress)
                    self.progress_label.configure(text=f"{progress * 100:.1f}%")
            except Exception:
                pass

    def _get_video_duration(self):
        if not self.ffprobe_path:
            return

        input_file = self.input_file.get()
        if not input_file or input_file.startswith("Drag and drop"):
            return

        # Check cache first
        if input_file in self.video_metadata_cache:
            self.total_duration = self.video_metadata_cache[input_file]
            if hasattr(self, "trim_canvas"):
                self.master.after(100, self._update_trim_slider)
            return

        command = [
            self.ffprobe_path,
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input_file,
        ]

        try:
            result = subprocess.run(
                command,
                check=True,
                capture_output=True,
                text=True,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            )
            self.total_duration = float(result.stdout.strip())
            # Cache the result
            self.video_metadata_cache[input_file] = self.total_duration

            if hasattr(self, "trim_canvas"):
                self.master.after(100, self._update_trim_slider)
        except Exception:
            self.total_duration = 0

    def _calculate_estimated_size(self):
        if self.constant_qp_mode.get():
            self.estimated_file_size.set("Estimated size: Not available for CQP")
            return

        input_f = self.input_file.get()
        bitrate_val = self.bitrate.get()

        if not self.ffprobe_path:
            self.estimated_file_size.set("")
            return

        if not input_f or not os.path.exists(input_f):
            self.estimated_file_size.set("")
            return

        try:
            bitrate_int = int(bitrate_val)
        except ValueError:
            self.estimated_file_size.set("")
            return

        audio_opt = self.audio_option.get()
        custom_ab = self.custom_abitrate.get()

        Thread(
            target=self._run_ffprobe_for_size,
            args=(input_f, bitrate_int, audio_opt, custom_ab),
        ).start()

    def _run_ffprobe_for_size(
        self, input_f, bitrate_int, audio_option, custom_abitrate
    ):
        command = [
            self.ffprobe_path,
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input_f,
        ]
        startupinfo = None
        creationflags = 0
        if os.name == "nt":
            startupinfo = subprocess.STARTUPINFO()
            startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
            startupinfo.wShowWindow = subprocess.SW_HIDE
            creationflags = subprocess.CREATE_NO_WINDOW
        try:
            process = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                startupinfo=startupinfo,
                creationflags=creationflags,
            )
            duration_str = process.stdout.read().strip()
            process.wait()
            if process.returncode == 0 and duration_str:
                try:
                    duration = float(duration_str)
                    audio_bitrate_for_estimation = 160

                    if audio_option == "custom":
                        try:
                            audio_bitrate_for_estimation = int(custom_abitrate)
                        except ValueError:
                            pass
                    elif audio_option == "aac_256k":
                        audio_bitrate_for_estimation = 256
                    elif audio_option == "aac_96k":
                        audio_bitrate_for_estimation = 96
                    elif audio_option == "disable":
                        audio_bitrate_for_estimation = 0

                    filesize_mb = (
                        (bitrate_int + audio_bitrate_for_estimation)
                        * duration
                        / 8
                        / 1024
                    )
                    self.master.after(
                        0,
                        lambda: self.estimated_file_size.set(
                            f"Estimated size: {filesize_mb:.2f} MB"
                        ),
                    )
                except ValueError:
                    self.master.after(0, lambda: self.estimated_file_size.set(""))
            else:
                self.master.after(
                    0,
                    lambda: self.estimated_file_size.set(
                        "Estimated size: Could not get duration"
                    ),
                )
                self.master.after(
                    0, lambda: self.status_text.set("Ready for conversion")
                )
        except FileNotFoundError:
            self.master.after(
                0, lambda: self.status_text.set("Error: ffprobe.exe not found.")
            )
            self.master.after(
                0,
                lambda: messagebox.showerror(
                    "Error",
                    "ffprobe.exe not found. Ensure it's in the program folder "
                    "or system PATH.",
                ),
            )
        except Exception:
            self.master.after(
                0,
                lambda: self.status_text.set(
                    "Error estimating size: Could not get duration"
                ),
            )
            self.master.after(
                0,
                lambda: messagebox.showerror(
                    "Unexpected Error",
                    "An unexpected error occurred during size estimation",
                ),
            )

    def _run_ffmpeg(self, command):
        startupinfo = None
        creationflags = 0
        if os.name == "nt":
            startupinfo = subprocess.STARTUPINFO()
            startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
            startupinfo.wShowWindow = subprocess.SW_HIDE
            creationflags = subprocess.CREATE_NO_WINDOW
        try:
            self.conversion_process = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                startupinfo=startupinfo,
                creationflags=creationflags,
                encoding="utf-8",
                errors="replace",
            )
            last_line = ""
            for line in self.conversion_process.stdout:
                line = line.strip()
                if line:
                    last_line = line
                    self.master.after(0, lambda: self.ffmpeg_output.set(line))
                    self.master.after(0, lambda: self._update_progress(line))
            self.conversion_process.wait()
            if self.conversion_process.returncode == 0:
                self.master.after(
                    0, lambda: self.status_text.set("Conversion complete!")
                )
                self.master.after(0, lambda: self.ffmpeg_output.set(""))
                self.master.after(0, lambda: self.progress_frame.grid_remove())
                self.master.after(
                    0,
                    lambda: self.convert_button.configure(
                        text="Convert", fg_color=ACCENT_GREEN, hover_color=HOVER_GREEN
                    ),
                )
                self.master.after(0, lambda: MessageBeep(MB_ICONASTERISK))
            elif not self.is_converting:
                self.master.after(
                    0, lambda: self.status_text.set("Conversion cancelled by user")
                )
                self.master.after(0, lambda: self.ffmpeg_output.set(""))
                self.master.after(0, lambda: self.progress_frame.grid_remove())
                self.master.after(
                    0,
                    lambda: self.convert_button.configure(
                        text="Convert", fg_color=ACCENT_GREEN, hover_color=HOVER_GREEN
                    ),
                )
                self.master.after(
                    0,
                    lambda: messagebox.showinfo(
                        "Cancelled", "Conversion was cancelled"
                    ),
                )
            else:
                self.master.after(0, lambda: self.status_text.set("Conversion error!"))
                self.master.after(0, lambda: self.ffmpeg_output.set(""))
                self.master.after(0, lambda: self.progress_frame.grid_remove())
                self.master.after(
                    0,
                    lambda: self.convert_button.configure(
                        text="Convert", fg_color=ACCENT_GREEN, hover_color=HOVER_GREEN
                    ),
                )
                self.master.after(
                    0,
                    lambda: messagebox.showerror(
                        "Error",
                        (
                            f"FFmpeg exited with error code "
                            f"{self.conversion_process.returncode}.\n"
                            f"Last output: {last_line}"
                        ),
                    ),
                )
            self.is_converting = False
        except FileNotFoundError:
            self.master.after(
                0, lambda: self.status_text.set("Error: ffmpeg.exe not found.")
            )
            self.master.after(0, lambda: self.ffmpeg_output.set(""))
            self.master.after(0, lambda: self.progress_frame.grid_remove())
            self.master.after(
                0,
                lambda: self.convert_button.configure(
                    text="Convert", fg_color=ACCENT_GREEN, hover_color=HOVER_GREEN
                ),
            )
            self.master.after(
                0,
                lambda: messagebox.showerror(
                    "Error",
                    (
                        "ffmpeg.exe not found. Ensure it's in the program folder "
                        "or system PATH."
                    ),
                ),
            )
            self.is_converting = False
        except Exception:
            self.master.after(
                0,
                lambda: self.status_text.set(
                    "An unexpected error occurred during conversion"
                ),
            )
            self.master.after(0, lambda: self.ffmpeg_output.set(""))
            self.master.after(0, lambda: self.progress_frame.grid_remove())
            self.master.after(
                0,
                lambda: self.convert_button.configure(
                    text="Convert", fg_color=ACCENT_GREEN, hover_color=HOVER_GREEN
                ),
            )
            self.master.after(
                0,
                lambda: messagebox.showerror(
                    "Unexpected Error", "An unexpected error occurred during conversion"
                ),
            )
            self.is_converting = False

    def _show_help_window(self, title, help_type):
        help_window = ctk.CTkToplevel(self.master)
        help_window.title(title)
        help_window.geometry("800x600")
        help_window.transient(self.master)
        help_window.grab_set()
        help_window.configure(fg_color=SECONDARY_BG)

        content_frame = ctk.CTkFrame(help_window, fg_color=SECONDARY_BG)
        content_frame.pack(fill="both", expand=True, padx=10, pady=10)

        text_scroll = ctk.CTkScrollableFrame(content_frame, fg_color=PRIMARY_BG)
        text_scroll.pack(fill="both", expand=True, padx=5, pady=5)

        help_text = ctk.CTkLabel(
            text_scroll,
            text="Loading help information...",
            justify="left",
            anchor="nw",
            wraplength=750,
        )
        help_text.pack(fill="both", expand=True, padx=10, pady=10)

        close_btn = ctk.CTkButton(
            content_frame,
            text="Close",
            command=help_window.destroy,
            fg_color=ACCENT_GREEN,
            hover_color=HOVER_GREEN,
            text_color=TEXT_COLOR_B,
        )
        close_btn.pack(pady=10)

        Thread(
            target=self._fetch_help_info,
            args=(help_type, help_text, help_window),
            daemon=True,
        ).start()

    def _fetch_help_info(self, help_type, text_widget, window):
        try:
            if help_type == "encoder":
                encoder_name = (
                    "h264_nvenc"
                    if self.video_codec.get() == "h264"
                    else (
                        "hevc_nvenc"
                        if self.video_codec.get() == "hevc"
                        else "av1_nvenc"
                    )
                )
                cmd = [self.ffmpeg_path, "-h", f"encoder={encoder_name}"]
            elif help_type == "filters":
                cmd = [self.ffmpeg_path, "-filters"]
            else:
                cmd = [self.ffmpeg_path, "-h"]

            startupinfo = None
            if os.name == "nt":
                startupinfo = subprocess.STARTUPINFO()
                startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
                startupinfo.wShowWindow = subprocess.SW_HIDE
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                startupinfo=startupinfo,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            )
            output = result.stdout or result.stderr
            window.after(0, lambda: text_widget.configure(text=output))
        except Exception as e:
            error_msg = f"Error retrieving help information:\n{str(e)}"
            window.after(0, lambda: text_widget.configure(text=error_msg))

    def _update_codec_settings(self):
        if self.video_codec.get() == "hevc":
            # HEVC settings
            self.profile.set("main")
            self.tune_option_menu.configure(
                values=["hq", "uhq", "ll", "ull", "lossless"]
            )
            self.profile_option_menu.configure(values=["main", "main10", "rext"])
            self.level_option_menu.configure(
                values=[
                    "auto",
                    "1.0",
                    "2.0",
                    "2.1",
                    "3.0",
                    "3.1",
                    "4.0",
                    "4.1",
                    "5.0",
                    "5.1",
                    "5.2",
                    "6.0",
                    "6.1",
                    "6.2",
                ]
            )

            self.tier_label.grid()
            self.tier_option_menu.grid()
            self.coder_label.grid_remove()
            self.coder_option_menu.grid_remove()

            # Show profile menu for HEVC
            self.profile_option_menu.grid()
            ctk.CTkLabel(
                self.encoder_options_frame.winfo_children()[0], text="Profile:"
            ).grid(row=2, column=0, sticky="w", padx=5, pady=2)

            # Show lookahead controls for HEVC
            self.lookahead_level_label.grid()
            self.lookahead_level_menu.grid()

        elif self.video_codec.get() == "av1":
            # AV1 settings
            self.profile.set("")
            self.tune_option_menu.configure(
                values=["hq", "uhq", "ll", "ull", "lossless"]
            )
            self.profile_option_menu.configure(values=[])
            self.level_option_menu.configure(
                values=[
                    "auto",
                    "2.0",
                    "2.1",
                    "2.2",
                    "2.3",
                    "3.0",
                    "3.1",
                    "3.2",
                    "3.3",
                    "4.0",
                    "4.1",
                    "4.2",
                    "4.3",
                    "5.0",
                    "5.1",
                    "5.2",
                    "5.3",
                    "6.0",
                    "6.1",
                    "6.2",
                    "6.3",
                    "7.0",
                    "7.1",
                    "7.2",
                    "7.3",
                ]
            )

            self.tier_label.grid()
            self.tier_option_menu.grid()
            self.coder_label.grid_remove()
            self.coder_option_menu.grid_remove()

            # Hide profile menu for AV1
            self.profile_option_menu.grid_remove()
            for child in self.encoder_options_frame.winfo_children()[
                0
            ].winfo_children():
                if isinstance(child, ctk.CTkLabel) and child.cget("text") == "Profile:":
                    child.grid_remove()

            # Show lookahead controls for AV1
            self.lookahead_level_label.grid()
            self.lookahead_level_menu.grid()

        else:
            # H.264 settings
            self.profile.set("main")
            self.tune_option_menu.configure(values=["hq", "ll", "ull", "lossless"])
            self.profile_option_menu.configure(
                values=["baseline", "main", "high", "high10", "high422", "high444p"]
            )
            self.level_option_menu.configure(
                values=[
                    "auto",
                    "1b",
                    "1.0b",
                    "1.1",
                    "1.2",
                    "1.3",
                    "2.0",
                    "2.1",
                    "2.2",
                    "3.0",
                    "3.1",
                    "3.2",
                    "4.0",
                    "4.1",
                    "4.2",
                    "5.0",
                    "5.1",
                    "5.2",
                    "6.0",
                ]
            )

            self.coder_label.grid()
            self.coder_option_menu.grid()
            self.tier_label.grid_remove()
            self.tier_option_menu.grid_remove()

            # Show profile menu for H.264
            self.profile_option_menu.grid()
            ctk.CTkLabel(
                self.encoder_options_frame.winfo_children()[0], text="Profile:"
            ).grid(row=2, column=0, sticky="w", padx=5, pady=2)

            # Hide lookahead controls for H.264 and force -1
            self.lookahead_level_label.grid_remove()
            self.lookahead_level_menu.grid_remove()
            self.lookahead_level.set("-1")

    def _update_output_filename(self, *args):
        if self.input_file.get() and not self.input_file.get().startswith(
            "Drag and drop"
        ):
            current_output = self.output_file.get()

            if (
                "_hevc_custom." in current_output
                or "_h264_custom." in current_output
                or "_av1_custom." in current_output
            ):
                base = (
                    current_output.split("_hevc_custom.")[0]
                    .split("_h264_custom.")[0]
                    .split("_av1_custom.")[0]
                )
                base_name = os.path.basename(base)
            else:
                input_path = self.input_file.get()
                base_name = os.path.splitext(os.path.basename(input_path))[0]

            if os.path.isdir(os.path.dirname(current_output)):
                dir_name = os.path.dirname(current_output)
            else:
                dir_name = os.path.dirname(self.input_file.get())

            codec_suffix = (
                "_hevc"
                if self.video_codec.get() == "hevc"
                else "_h264"
                if self.video_codec.get() == "h264"
                else "_av1"
            )
            new_filename = f"{base_name}{codec_suffix}_custom.mp4"
            new_output = os.path.normpath(os.path.join(dir_name, new_filename))

            self.output_file.set(new_output)

    def _toggle_presets_frame(self):
        if self.enable_presets.get():
            self.presets_frame.grid(
                row=14, column=0, columnspan=4, sticky="ew", padx=10, pady=10
            )
        else:
            self.presets_frame.grid_forget()
        self._update_window_size()

    def _apply_preset(self, preset_name):
        if preset_name == "none":
            # Reset to default settings
            self.enable_encoder_options.set(False)
            self.enable_fps_scale_options.set(False)
            self.enable_additional_options.set(False)

            # Reset all settings to their initial values
            self.bitrate.set("6000")
            self.quality_level.set("30")
            self.audio_option.set("copy")
            self.custom_abitrate.set("160")

            # Reset encoder options
            self.preset.set("p5")
            self.tune.set("hq")
            if self.video_codec.get() == "hevc":
                self.profile.set("main")
            else:
                self.profile.set("main")
            self.level.set("auto")
            self.tier.set("1")
            self.multipass.set("qres")
            self.rc.set("vbr")
            self.lookahead_level.set("-1")
            self.spatial_aq.set(True)
            self.temporal_aq.set(True)
            self.no_scenecut.set(False)
            self.weighted_pred.set(False)
            self.highbitdepth.set(False)
            self.strict_gop.set(False)

            # Reset FPS and scaling options
            self.fps_option.set("source")
            self.custom_fps.set("30")
            self.video_format_option.set("source")
            self.custom_video_width.set("1920")
            self.interpolation_algo.set("bicubic")

            # Clear additional options
            self.additional_options.set("")
            self.additional_filter_options.set("")
            self.additional_audio_filter_options.set("")

            # Reset entry placeholders
            self.additional_options_entry.delete(0, "end")
            self.additional_options_entry.insert(0, self.additional_options_placeholder)
            self.additional_options_entry.configure(text_color=PLACEHOLDER_COLOR)

            self.additional_filter_options_entry.delete(0, "end")
            self.additional_filter_options_entry.insert(
                0, self.additional_filter_options_placeholder
            )
            self.additional_filter_options_entry.configure(text_color=PLACEHOLDER_COLOR)

            self.additional_audio_filter_options_entry.delete(0, "end")
            self.additional_audio_filter_options_entry.insert(
                0, self.additional_audio_filter_options_placeholder
            )
            self.additional_audio_filter_options_entry.configure(
                text_color=PLACEHOLDER_COLOR
            )

            self.preset_indicator.configure(
                text="Default settings applied", text_color=ACCENT_GREEN
            )

        elif preset_name == "fhdf":
            # FHD Fast preset
            self.enable_encoder_options.set(True)
            self.enable_fps_scale_options.set(True)

            # Video settings
            self.bitrate.set("6000")
            self.quality_level.set("30")
            self.video_format_option.set("1920")
            self.interpolation_algo.set("bicubic")

            # Encoder settings
            self.preset.set("p3")
            self.tune.set("hq")
            if self.video_codec.get() == "hevc":
                self.profile.set("main")
            else:
                self.profile.set("high")
            self.tier.set("1")
            self.multipass.set("disabled")
            self.rc.set("vbr")
            self.lookahead_level.set("-1")

            # Flags
            self.spatial_aq.set(False)
            self.temporal_aq.set(False)
            self.no_scenecut.set(False)
            self.weighted_pred.set(False)
            self.highbitdepth.set(False)
            self.strict_gop.set(False)

            # Audio
            self.audio_option.set("aac_160k")

            self.preset_indicator.configure(
                text="FHD Fast preset applied", text_color=ACCENT_GREEN
            )

        elif preset_name == "fhdq":
            # FHD Quality preset
            self.enable_encoder_options.set(True)
            self.enable_fps_scale_options.set(True)

            # Video settings
            self.bitrate.set("8000")
            self.quality_level.set("27")
            self.video_format_option.set("1920")
            self.interpolation_algo.set("spline")

            # Encoder settings
            self.preset.set("p7")
            self.tune.set("hq")
            if self.video_codec.get() == "hevc":
                self.profile.set("main")
            else:
                self.profile.set("high")
            self.tier.set("1")
            self.multipass.set("fullres")
            self.rc.set("vbr")
            if self.video_codec.get() != "h264":
                self.lookahead_level.set("3")

            # Flags
            self.spatial_aq.set(True)
            self.temporal_aq.set(True)
            self.no_scenecut.set(False)
            self.weighted_pred.set(False)
            self.highbitdepth.set(False)
            self.strict_gop.set(False)

            # Audio
            self.audio_option.set("aac_160k")

            self.preset_indicator.configure(
                text="FHD Quality preset applied", text_color=ACCENT_GREEN
            )

        elif preset_name == "hdf":
            # HD Fast preset
            self.enable_encoder_options.set(True)
            self.enable_fps_scale_options.set(True)

            # Video settings
            self.bitrate.set("4000")
            self.quality_level.set("30")
            self.video_format_option.set("1280")
            self.interpolation_algo.set("bicubic")

            # Encoder settings
            self.preset.set("p3")
            self.tune.set("hq")
            if self.video_codec.get() == "hevc":
                self.profile.set("main")
            else:
                self.profile.set("high")
            self.tier.set("1")
            self.multipass.set("disabled")
            self.rc.set("vbr")
            self.lookahead_level.set("-1")

            # Flags
            self.spatial_aq.set(False)
            self.temporal_aq.set(False)
            self.no_scenecut.set(False)
            self.weighted_pred.set(False)
            self.highbitdepth.set(False)
            self.strict_gop.set(False)

            # Audio
            self.audio_option.set("aac_160k")

            self.preset_indicator.configure(
                text="HD Fast preset applied", text_color=ACCENT_GREEN
            )

        elif preset_name == "hdq":
            # HD Quality preset
            self.enable_encoder_options.set(True)
            self.enable_fps_scale_options.set(True)

            # Video settings
            self.bitrate.set("6000")
            self.quality_level.set("27")
            self.video_format_option.set("1280")
            self.interpolation_algo.set("spline")

            # Encoder settings
            self.preset.set("p7")
            self.tune.set("hq")
            if self.video_codec.get() == "hevc":
                self.profile.set("main")
            else:
                self.profile.set("high")
            self.tier.set("1")
            self.multipass.set("fullres")
            self.rc.set("vbr")
            if self.video_codec.get() != "h264":
                self.lookahead_level.set("3")

            # Flags
            self.spatial_aq.set(True)
            self.temporal_aq.set(True)
            self.no_scenecut.set(False)
            self.weighted_pred.set(False)
            self.highbitdepth.set(False)
            self.strict_gop.set(False)

            # Audio
            self.audio_option.set("aac_160k")

            self.preset_indicator.configure(
                text="HD Quality preset applied", text_color=ACCENT_GREEN
            )

        # Update UI
        self._toggle_encoder_options_frame()
        self._toggle_fps_scale_options_frame()
        self._toggle_additional_options_frame()
        self._update_window_size()

    def _add_trim_options(self):
        """Add trim options to the additional options field"""
        # Validate both times first
        if not self._validate_trim_time(
            self.trim_start, True
        ) or not self._validate_trim_time(self.trim_end, False):
            return

        start_time = self.trim_start.get()
        end_time = self.trim_end.get()

        trim_options = f"-ss {start_time} -to {end_time}"

        current_options = self.additional_options.get()
        if current_options == self.additional_options_placeholder:
            current_options = ""

        current_options = self._remove_existing_trim_options(current_options)

        if current_options:
            new_options = f"{current_options} {trim_options}"
        else:
            new_options = trim_options

        self.additional_options.set(new_options)
        self.additional_options_entry.configure(text_color=TEXT_COLOR_W)

    def _validate_trim_time(self, time_var, is_start):
        """Validate time input only when field loses focus"""
        time_str = time_var.get()

        # Skip validation if field is empty or doesn't contain colons
        if not time_str or ":" not in time_str:
            return True

        # Validate time format
        if not self._validate_time_format(time_str):
            messagebox.showerror("Error", "Time format should be HH:MM:SS")
            return False

        # Convert to seconds
        time_seconds = self._time_str_to_seconds(time_str)

        # Get video duration
        duration = self._get_video_duration_safe()

        # Check if time exceeds video duration
        if duration > 0 and time_seconds > duration:
            messagebox.showerror(
                "Error",
                f"Time exceeds video duration ({self._seconds_to_time_str(duration)})",
            )
            return False

        # Get the other time value
        other_time_str = self.trim_end.get() if is_start else self.trim_start.get()

        # Only validate against other time if it's also valid
        if (
            other_time_str
            and ":" in other_time_str
            and self._validate_time_format(other_time_str)
        ):
            other_time_seconds = self._time_str_to_seconds(other_time_str)

            if is_start and time_seconds >= other_time_seconds:
                messagebox.showerror("Error", "Start time must be before end time")
                return False
            elif not is_start and time_seconds <= other_time_seconds:
                messagebox.showerror("Error", "End time must be after start time")
                return False

        # Update slider if it exists
        if hasattr(self, "trim_canvas"):
            self.master.after(100, self._draw_trim_slider)

        return True

    def _remove_existing_trim_options(self, options_str):
        """Remove any existing trim-related options from the string"""
        options_str = sub(r"-ss\s+\S+", "", options_str)
        options_str = sub(r"-to\s+\S+", "", options_str)
        options_str = sub(r"-c\s+copy", "", options_str)
        options_str = " ".join(options_str.split())
        return options_str.strip()

    def _create_trim_slider(self):
        """Create a slider for visual video trimming"""
        # Frame for the trim slider
        self.trim_slider_frame = ctk.CTkFrame(
            self.additional_options_frame, fg_color="transparent"
        )
        self.trim_slider_frame.grid(
            row=3, column=0, columnspan=3, sticky="ew", padx=10, pady=(0, 10)
        )

        # Container for canvas and button in one line
        slider_container = ctk.CTkFrame(self.trim_slider_frame, fg_color="transparent")
        slider_container.pack(fill="x", expand=True)

        # Canvas for slider (takes main space)
        self.trim_canvas = ctk.CTkCanvas(
            slider_container, height=30, bg=SECONDARY_BG, highlightthickness=0
        )
        self.trim_canvas.pack(side="left", fill="x", expand=True, padx=(0, 10))

        # Reset button (fixed width)
        self.trim_reset_btn = ctk.CTkButton(
            slider_container,
            text="⟲",
            width=30,
            height=30,
            command=self._reset_trim_slider,
            fg_color="#1f6aa5",
            hover_color="#2a7ab9",
            text_color=TEXT_COLOR_W,
        )
        self.trim_reset_btn.pack(side="right", padx=(0, 4))

        # Draw the initial slider
        self._draw_trim_slider()

        # Bind mouse events
        self.trim_canvas.bind("<Button-1>", self._on_slider_click)
        self.trim_canvas.bind("<B1-Motion>", self._on_slider_drag)

        # Add preview bindings
        self.trim_canvas.bind("<ButtonRelease-1>", self._on_slider_release)
        self.trim_canvas.bind("<Leave>", lambda e: self._hide_thumbnail_preview())

    def _create_thumbnail_preview(self):
        """Create a thumbnail preview window"""
        self.preview_window = None
        self.preview_label = None
        self.preview_visible = False

    def _show_thumbnail_preview(self, x_pos, time_seconds):
        """Show thumbnail preview at specified position and time"""
        self.preview_job = None
        if not self.input_file.get() or self.input_file.get().startswith(
            "Drag and drop"
        ):
            return

        if not self.ffmpeg_path:
            return

        # Create preview window if it doesn't exist
        if not self.preview_window:
            self.preview_window = ctk.CTkToplevel(self.master)
            self.preview_window.title("Preview")
            self.preview_window.overrideredirect(True)
            self.preview_window.attributes("-topmost", True)
            self.preview_window.configure(fg_color=PRIMARY_BG)

            self.preview_label = ctk.CTkLabel(
                self.preview_window,
                text="",
                width=352,
                height=198,
                fg_color=SECONDARY_BG,
                corner_radius=0,
            )
            self.preview_label.pack(padx=0, pady=0)
            self.preview_window.geometry("352x198")

        # Generate thumbnail in memory
        try:
            cmd = [
                self.ffmpeg_path,
                "-ss",
                str(time_seconds),
                "-i",
                self.input_file.get(),
                "-vframes",
                "1",
                "-vf",
                "scale=352:-1",
                "-q:v",
                "2",
                "-f",
                "mjpeg",
                "pipe:1",
            ]

            process = subprocess.run(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=5,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            )

            if process.returncode == 0 and process.stdout:
                img_buffer = BytesIO(process.stdout)
                thumb_image = Image.open(img_buffer)
                ctk_thumb = ctk.CTkImage(light_image=thumb_image, size=(352, 198))
                self.preview_label.configure(image=ctk_thumb, text="")

                # Position preview above slider handle
                slider_x = self.trim_canvas.winfo_rootx() + x_pos - 176
                slider_y = self.trim_canvas.winfo_rooty() - 208

                self.preview_window.geometry(f"352x198+{slider_x}+{slider_y}")
                self.preview_window.deiconify()
                self.preview_visible = True
            else:
                self.preview_label.configure(text="Preview\nunavailable")

        except Exception as e:
            print(f"Error generating preview: {e}")
            self.preview_label.configure(text="Preview\nunavailable")

    def _schedule_thumbnail_preview(self, x_pos, time_seconds, delay=200):
        """Debounce thumbnail generation: cancel previous and schedule new one."""
        try:
            # cancel any pending scheduled preview
            if getattr(self, "preview_job", None):
                self.master.after_cancel(self.preview_job)
        except Exception:
            pass

        # schedule a new preview after `delay` ms
        self.preview_job = self.master.after(
            delay, lambda: self._show_thumbnail_preview(x_pos, time_seconds)
        )

    def _hide_thumbnail_preview(self):
        """Hide the thumbnail preview"""
        if self.preview_window and self.preview_visible:
            self.preview_window.withdraw()
            self.preview_visible = False

    def _draw_trim_slider(self):
        """Draw the trim slider with current positions"""
        self.trim_canvas.delete("all")
        width = self.trim_canvas.winfo_width()
        if width < 10:  # Minimum width
            width = 800

        # Draw the track
        self.trim_canvas.create_line(10, 15, width - 10, 15, fill="#555555", width=3)

        # Calculate handle positions based on current time values
        start_pos, end_pos = self._time_to_slider_positions()

        # Draw handles
        self.start_handle = self.trim_canvas.create_oval(
            start_pos - 8, 15 - 8, start_pos + 8, 15 + 8, fill=ACCENT_GREEN, outline=""
        )
        self.end_handle = self.trim_canvas.create_oval(
            end_pos - 8, 15 - 8, end_pos + 8, 15 + 8, fill=ACCENT_GREEN, outline=""
        )

        # Draw current selection
        if start_pos < end_pos:
            self.trim_canvas.create_rectangle(
                start_pos, 12, end_pos, 18, fill=ACCENT_GREEN, outline=""
            )

    def _time_to_slider_positions(self):
        """Convert time values to slider positions"""
        try:
            # Get total duration
            if not hasattr(self, "total_duration") or self.total_duration <= 0:
                self._get_video_duration()

            if not hasattr(self, "total_duration") or self.total_duration <= 0:
                return 10, (
                    self.trim_canvas.winfo_width() - 10
                    if self.trim_canvas.winfo_width() > 20
                    else 290
                )

            # Convert current time strings to seconds
            start_seconds = self._time_str_to_seconds(self.trim_start.get())
            end_seconds = self._time_str_to_seconds(self.trim_end.get())

            # Calculate positions
            width = self.trim_canvas.winfo_width()
            if width < 20:
                width = 400

            start_pos = 10 + (start_seconds / self.total_duration) * (width - 20)
            end_pos = 10 + (end_seconds / self.total_duration) * (width - 20)

            return max(10, min(width - 10, start_pos)), max(
                10, min(width - 10, end_pos)
            )

        except Exception:
            return 10, (
                self.trim_canvas.winfo_width() - 10
                if self.trim_canvas.winfo_width() > 20
                else 290
            )

    def _time_str_to_seconds(self, time_str):
        """Convert HH:MM:SS to seconds"""
        try:
            parts = time_str.split(":")
            if len(parts) != 3:
                return 0
            hours = int(parts[0])
            minutes = int(parts[1])
            seconds = int(parts[2])
            return hours * 3600 + minutes * 60 + seconds
        except Exception:
            return 0

    def _validate_time_format(self, time_str):
        """Simple validation for HH:MM:SS format"""
        if not time_str or not isinstance(time_str, str):
            return False

        parts = time_str.split(":")
        if len(parts) != 3:
            return False

        try:
            hours = int(parts[0])
            minutes = int(parts[1])
            seconds = int(parts[2])
            return 0 <= hours < 24 and 0 <= minutes < 60 and 0 <= seconds < 60
        except ValueError:
            return False

    def _get_video_duration_safe(self):
        """Safely get video duration without blocking the UI"""
        if (
            not self.ffprobe_path
            or not self.input_file.get()
            or self.input_file.get().startswith("Drag and drop")
        ):
            return 0

        # Check cache first
        input_file = self.input_file.get()
        if input_file in self.video_metadata_cache:
            return self.video_metadata_cache[input_file]

        # If not in cache, try to get it
        try:
            command = [
                self.ffprobe_path,
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                input_file,
            ]

            result = subprocess.run(
                command,
                check=True,
                capture_output=True,
                text=True,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            )

            duration = float(result.stdout.strip())
            self.video_metadata_cache[input_file] = duration
            return duration
        except Exception:
            return 0

    def _validate_and_update_trim_time(self, time_var, is_start):
        """Validate time input when Enter is pressed and update slider"""
        time_str = time_var.get()

        # Skip validation if field is empty or doesn't contain colons
        if not time_str or ":" not in time_str:
            return

        # Validate time format
        if not self._validate_time_format(time_str):
            messagebox.showerror("Error", "Time format should be HH:MM:SS")
            return

        # Convert to seconds
        time_seconds = self._time_str_to_seconds(time_str)

        # Get video duration
        duration = self._get_video_duration_safe()

        # Check if time exceeds video duration
        if duration > 0 and time_seconds > duration:
            messagebox.showerror(
                "Error",
                f"Time exceeds video duration ({self._seconds_to_time_str(duration)})",
            )
            return

        # Get the other time value
        other_time_str = self.trim_end.get() if is_start else self.trim_start.get()

        # Only validate against other time if it's also valid
        if (
            other_time_str
            and ":" in other_time_str
            and self._validate_time_format(other_time_str)
        ):
            other_time_seconds = self._time_str_to_seconds(other_time_str)

            if is_start and time_seconds >= other_time_seconds:
                messagebox.showerror("Error", "Start time must be before end time")
                return
            elif not is_start and time_seconds <= other_time_seconds:
                messagebox.showerror("Error", "End time must be after start time")
                return

        # Update slider if it exists
        if hasattr(self, "trim_canvas"):
            self.master.after(100, self._draw_trim_slider)

    def _seconds_to_time_str(self, seconds):
        """Convert seconds to HH:MM:SS"""
        hours = int(seconds // 3600)
        minutes = int((seconds % 3600) // 60)
        seconds = int(seconds % 60)
        return f"{hours:02d}:{minutes:02d}:{seconds:02d}"

    def _on_slider_click(self, event):
        """Handle click on slider"""
        x, y = event.x, event.y

        # Check if click is near start handle
        start_pos, end_pos = self._time_to_slider_positions()
        if abs(x - start_pos) < 10 and abs(y - 15) < 10:
            self.dragging_handle = "start"
        elif abs(x - end_pos) < 10 and abs(y - 15) < 10:
            self.dragging_handle = "end"
        else:
            self.dragging_handle = None

        # Show preview immediately
        fraction = (x - 10) / (self.trim_canvas.winfo_width() - 20)
        time_seconds = fraction * (getattr(self, "total_duration", 0) or 0)
        self._show_thumbnail_preview(x, time_seconds)

    def _on_slider_release(self, event):
        # Mark slider as released
        self.is_slider_dragging = False

        # Cancel any scheduled preview job
        if getattr(self, "preview_job", None):
            try:
                self.master.after_cancel(self.preview_job)
            except Exception:
                pass
            self.preview_job = None

        # Hide preview popup
        self._hide_thumbnail_preview()

    def _on_slider_drag(self, event):
        """Handle drag on slider"""
        if not hasattr(self, "dragging_handle") or not self.dragging_handle:
            return

        if not hasattr(self, "total_duration") or self.total_duration <= 0:
            return

        x = max(10, min(self.trim_canvas.winfo_width() - 10, event.x))
        width = self.trim_canvas.winfo_width()
        if width < 20:
            return

        # Calculate new time value
        fraction = (x - 10) / (width - 20)
        new_seconds = fraction * self.total_duration
        # Show preview
        self._schedule_thumbnail_preview(x, new_seconds)

        if self.dragging_handle == "start":
            # Ensure start doesn't go past end
            end_seconds = self._time_str_to_seconds(self.trim_end.get())
            new_seconds = min(new_seconds, end_seconds - 1)  # At least 1 second gap
            self.trim_start.set(self._seconds_to_time_str(new_seconds))
        else:
            # Ensure end doesn't go before start
            start_seconds = self._time_str_to_seconds(self.trim_start.get())
            new_seconds = max(new_seconds, start_seconds + 1)  # At least 1 second gap
            self.trim_end.set(self._seconds_to_time_str(new_seconds))

        # Redraw slider and update trim options
        self._draw_trim_slider()

    def _reset_trim_slider(self):
        """Reset trim slider to full duration"""
        self.trim_start.set("00:00:00")

        if self.input_file.get() and not self.input_file.get().startswith(
            "Drag and drop"
        ):
            self._set_trim_end_to_duration()
        else:
            self.trim_end.set("00:00:00")

        self._draw_trim_slider()

    def _update_trim_slider(self):
        """Update slider when video duration changes"""
        if hasattr(self, "trim_canvas"):
            self._draw_trim_slider()

    def _apply_hdr_to_sdr(self):
        """Apply HDR to SDR conversion settings"""
        hdr_filter = (
            "zscale=transfer=linear:npl=100,"
            "tonemap=tonemap=hable:desat=0,"
            "zscale=transfer=bt709:matrix=bt709:primaries=bt709"
        )
        self._add_video_filter(hdr_filter)
        self.hdr_mode = True


def get_icon_path():
    if getattr(sys, "frozen", False):
        base_path = sys._MEIPASS
    else:
        base_path = os.path.dirname(__file__)
    return os.path.join(base_path, "icon.ico")


root = ctk.CTk()
app = VideoConverterApp(root)

icon_path = get_icon_path()
if os.path.exists(icon_path):
    root.after(201, lambda: root.iconbitmap(icon_path))
else:
    print(f"icon not found: {icon_path}")

root.protocol("WM_DELETE_WINDOW", app._on_close)
root.mainloop()
