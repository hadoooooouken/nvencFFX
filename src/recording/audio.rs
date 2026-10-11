//! Desktop audio of a screen recording (`_record_audio_loop`): WASAPI loopback of the
//! default playback device, written to a 16-bit PCM WAV file.
//!
//! Raw COM through hand-written vtables like the taskbar overlay; `cpal` would pull
//! in the `windows` crate for this one stream.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// Sample layout of the device's mix format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sample {
    F32,
    I16,
    I24,
    /// Also 24 valid bits in a 32-bit container (left-aligned)
    I32,
}

impl Sample {
    fn bytes(self) -> usize {
        match self {
            Self::I16 => 2,
            Self::I24 => 3,
            Self::F32 | Self::I32 => 4,
        }
    }

    /// Appends the samples of `data` as 16-bit (`paInt16`).
    fn convert(self, data: &[u8], out: &mut Vec<i16>) {
        out.extend(data.chunks_exact(self.bytes()).map(|b| match self {
            Self::F32 => (f32::from_le_bytes([b[0], b[1], b[2], b[3]]).clamp(-1.0, 1.0) * 32767.0).round() as i16,
            Self::I16 => i16::from_le_bytes([b[0], b[1]]),
            Self::I24 => i16::from_le_bytes([b[1], b[2]]),
            Self::I32 => i16::from_le_bytes([b[2], b[3]]),
        }));
    }
}

/// 16-bit PCM WAV file (`wave.open(..., "wb")`); [`Wav::finish`] fills in the sizes.
pub struct Wav {
    file: BufWriter<File>,
    block_align: u16,
    data_len: u64,
}

impl Wav {
    pub fn create(path: &Path, channels: u16, rate: u32) -> io::Result<Self> {
        let block_align = channels * 2;
        let mut file = BufWriter::new(File::create(path)?);
        file.write_all(b"RIFF\0\0\0\0WAVEfmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?; // PCM
        file.write_all(&channels.to_le_bytes())?;
        file.write_all(&rate.to_le_bytes())?;
        file.write_all(&(rate * u32::from(block_align)).to_le_bytes())?;
        file.write_all(&block_align.to_le_bytes())?;
        file.write_all(&16u16.to_le_bytes())?;
        file.write_all(b"data\0\0\0\0")?;
        Ok(Self { file, block_align, data_len: 0 })
    }

    /// Whole frames written so far.
    pub fn frames(&self) -> u64 {
        self.data_len / u64::from(self.block_align)
    }

    pub fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        for s in samples {
            self.file.write_all(&s.to_le_bytes())?;
        }
        self.data_len += samples.len() as u64 * 2;
        Ok(())
    }

    pub fn silence(&mut self, frames: u64) -> io::Result<()> {
        let mut left = frames * u64::from(self.block_align);
        self.data_len += left;
        let zeros = [0u8; 4096];
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            self.file.write_all(&zeros[..n])?;
            left -= n as u64;
        }
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<()> {
        let data = u32::try_from(self.data_len).unwrap_or(u32::MAX).min(u32::MAX - 36);
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&(36 + data).to_le_bytes())?;
        self.file.seek(SeekFrom::Start(40))?;
        self.file.write_all(&data.to_le_bytes())?;
        self.file.flush()
    }
}

/// The audio thread of one recording (`audio_thread`).
pub struct Capture {
    running: Arc<AtomicBool>,
    /// Disconnected once the thread is done with the file.
    done: Receiver<()>,
}

impl Capture {
    /// Records the desktop audio into `path` until [`Capture::stop`]. Errors only
    /// leave the recording without audio, as in the original.
    pub fn start(path: PathBuf) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let (done_tx, done) = channel::<()>();
        let flag = running.clone();
        let spawned = std::thread::Builder::new().name("audio".to_owned()).spawn(move || {
            if let Err(e) = record(&path, &flag) {
                eprintln!("Audio recording error: {e}");
            }
            drop(done_tx);
        });
        if let Err(e) = spawned {
            eprintln!("Audio recording error: {e}");
        }
        Self { running, done }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Stops and waits up to `timeout` until the file is complete
    /// (`audio_thread.join(timeout=2)`).
    pub fn finish(self, timeout: Duration) {
        self.stop();
        let _ = self.done.recv_timeout(timeout);
    }
}

#[cfg(windows)]
use wasapi::record;

#[cfg(not(windows))]
fn record(_: &Path, _: &AtomicBool) -> Result<(), String> {
    Err("WASAPI is not supported.".to_owned())
}

#[cfg(windows)]
mod wasapi {
    use std::ffi::c_void;
    use std::marker::PhantomData;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use windows_sys::Win32::{
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_SHAREMODE,
            AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, EDataFlow, ERole,
            MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eConsole, eRender,
        },
        System::{
            Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize},
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        },
    };
    use windows_sys::core::{GUID, HRESULT};
    use super::{Sample, Wav};

    const IID_IMMDEVICEENUMERATOR: GUID = GUID::from_u128(0xa95664d2_9614_4f35_a746_de8db63617e6);
    const IID_IAUDIOCLIENT: GUID = GUID::from_u128(0x1cb9ad4c_dbfa_4c32_b178_c2f568a703b2);
    const IID_IAUDIOCAPTURECLIENT: GUID = GUID::from_u128(0xc8adbd64_e71e_48a0_a4de_185c395cd317);
    const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
    const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
    const WAVE_FORMAT_PCM: u16 = 1;
    const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
    const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;

    /// Shared-mode buffer length: 200 ms in 100-ns units.
    const BUFFER_DURATION: i64 = 2_000_000;
    /// How often the capture buffer is emptied.
    const POLL: Duration = Duration::from_millis(10);
    /// Loopback delivers no packets while nothing plays. A packet stamped this much
    /// later than the end of the audio written so far follows such a stretch, which
    /// is then filled with silence so the audio stays in step with the video
    /// (100 ms in 100-ns units).
    const GAP: u64 = 1_000_000;
    const HNS_PER_SECOND: u64 = 10_000_000;

    #[repr(C)]
    struct IUnknownVtbl {
        _query_interface: usize,
        _add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
    }

    /// `IMMDeviceEnumerator`, up to the method used here.
    #[repr(C)]
    struct EnumeratorVtbl {
        _base: IUnknownVtbl,
        _enum_audio_endpoints: usize,
        get_default_audio_endpoint: unsafe extern "system" fn(*mut c_void, EDataFlow, ERole, *mut *mut c_void) -> HRESULT,
    }

    /// `IMMDevice`
    #[repr(C)]
    struct DeviceVtbl {
        _base: IUnknownVtbl,
        activate: unsafe extern "system" fn(*mut c_void, *const GUID, u32, *const c_void, *mut *mut c_void) -> HRESULT,
    }

    /// `IAudioClient`
    #[repr(C)]
    struct ClientVtbl {
        _base: IUnknownVtbl,
        initialize: unsafe extern "system" fn(*mut c_void, AUDCLNT_SHAREMODE, u32, i64, i64, *const WAVEFORMATEX, *const GUID) -> HRESULT,
        _get_buffer_size: usize,
        _get_stream_latency: usize,
        _get_current_padding: usize,
        _is_format_supported: usize,
        get_mix_format: unsafe extern "system" fn(*mut c_void, *mut *mut WAVEFORMATEX) -> HRESULT,
        _get_device_period: usize,
        start: unsafe extern "system" fn(*mut c_void) -> HRESULT,
        stop: unsafe extern "system" fn(*mut c_void) -> HRESULT,
        _reset: usize,
        _set_event_handle: usize,
        get_service: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    }

    /// `IAudioCaptureClient`
    #[repr(C)]
    struct CaptureVtbl {
        _base: IUnknownVtbl,
        get_buffer: unsafe extern "system" fn(*mut c_void, *mut *mut u8, *mut u32, *mut u32, *mut u64, *mut u64) -> HRESULT,
        release_buffer: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
        get_next_packet_size: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    }

    /// An interface pointer with vtable `V`, released on drop.
    struct Com<V> {
        ptr: *mut c_void,
        _vtbl: PhantomData<V>,
    }

    impl<V> Com<V> {
        /// Owns `ptr`, filled in by the call `what` that returned `hr`.
        fn new(hr: HRESULT, ptr: *mut c_void, what: &str) -> Result<Self, String> {
            check(hr, what)?;
            if ptr.is_null() {
                return Err(format!("{what} returned nothing"));
            }
            Ok(Self { ptr, _vtbl: PhantomData })
        }

        fn vtbl(&self) -> &V {
            unsafe { &**(self.ptr as *const *const V) }
        }
    }

    impl<V> Drop for Com<V> {
        fn drop(&mut self) {
            unsafe { ((**(self.ptr as *const *const IUnknownVtbl)).release)(self.ptr) };
        }
    }

    fn check(hr: HRESULT, what: &str) -> Result<(), String> {
        if hr < 0 { Err(format!("{what} failed (0x{:08X})", hr as u32)) } else { Ok(()) }
    }

    fn same(a: &GUID, b: &GUID) -> bool {
        (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
    }

    /// The performance counter in 100-ns units, the clock of the packet timestamps.
    fn now() -> u64 {
        let (mut count, mut freq) = (0i64, 0i64);
        unsafe {
            QueryPerformanceCounter(&mut count);
            QueryPerformanceFrequency(&mut freq);
        }
        (count as u128 * u128::from(HNS_PER_SECOND) / freq.max(1) as u128) as u64
    }

    /// Channels, sample rate and layout of the mix format.
    fn parse_format(format: *const WAVEFORMATEX) -> Result<(u16, u32, Sample), String> {
        let f = unsafe { std::ptr::read_unaligned(format) };
        let (mut tag, bits) = (f.wFormatTag, f.wBitsPerSample);
        if tag == WAVE_FORMAT_EXTENSIBLE && f.cbSize >= 22 {
            let ext = unsafe { std::ptr::read_unaligned(format.cast::<WAVEFORMATEXTENSIBLE>()) };
            let sub = ext.SubFormat;
            if same(&sub, &KSDATAFORMAT_SUBTYPE_IEEE_FLOAT) {
                tag = WAVE_FORMAT_IEEE_FLOAT;
            } else if same(&sub, &KSDATAFORMAT_SUBTYPE_PCM) {
                tag = WAVE_FORMAT_PCM;
            }
        }
        let sample = match (tag, bits) {
            (WAVE_FORMAT_IEEE_FLOAT, 32) => Sample::F32,
            (WAVE_FORMAT_PCM, 16) => Sample::I16,
            (WAVE_FORMAT_PCM, 24) => Sample::I24,
            (WAVE_FORMAT_PCM, 32) => Sample::I32,
            _ => return Err(format!("unsupported mix format (tag {tag}, {bits} bits)")),
        };
        if f.nChannels == 0 || f.nSamplesPerSec == 0 {
            return Err("invalid mix format".to_owned());
        }
        Ok((f.nChannels, f.nSamplesPerSec, sample))
    }

    pub fn record(path: &Path, running: &AtomicBool) -> Result<(), String> {
        // S_FALSE (already initialised) needs the matching CoUninitialize too
        let initialised = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) } >= 0;
        let result = capture(path, running);
        if initialised {
            unsafe { CoUninitialize() };
        }
        result
    }

    fn capture(path: &Path, running: &AtomicBool) -> Result<(), String> {
        let null = std::ptr::null_mut;
        let mut ptr = null();
        let hr = unsafe { CoCreateInstance(&MMDeviceEnumerator, null(), CLSCTX_ALL, &IID_IMMDEVICEENUMERATOR, &mut ptr) };
        let enumerator = Com::<EnumeratorVtbl>::new(hr, ptr, "MMDeviceEnumerator")?;

        // The loopback stream of the default speakers
        let mut ptr = null();
        let hr = unsafe { (enumerator.vtbl().get_default_audio_endpoint)(enumerator.ptr, eRender, eConsole, &mut ptr) };
        let device = Com::<DeviceVtbl>::new(hr, ptr, "loopback device").map_err(|_| "loopback device not found.".to_owned())?;
        let mut ptr = null();
        let hr = unsafe { (device.vtbl().activate)(device.ptr, &IID_IAUDIOCLIENT, CLSCTX_ALL, std::ptr::null(), &mut ptr) };
        let client = Com::<ClientVtbl>::new(hr, ptr, "IMMDevice::Activate")?;

        let mut format: *mut WAVEFORMATEX = std::ptr::null_mut();
        check(unsafe { (client.vtbl().get_mix_format)(client.ptr, &mut format) }, "GetMixFormat")?;
        let parsed = parse_format(format);
        let hr = match parsed {
            Ok(_) => unsafe {
                (client.vtbl().initialize)(
                    client.ptr,
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    BUFFER_DURATION,
                    0,
                    format,
                    std::ptr::null(),
                )
            },
            Err(_) => 0,
        };
        unsafe { CoTaskMemFree(format as *const c_void) };
        let (channels, rate, sample) = parsed?;
        check(hr, "IAudioClient::Initialize")?;

        let mut ptr = null();
        let hr = unsafe { (client.vtbl().get_service)(client.ptr, &IID_IAUDIOCAPTURECLIENT, &mut ptr) };
        let capture = Com::<CaptureVtbl>::new(hr, ptr, "IAudioClient::GetService")?;

        let mut wav = Wav::create(path, channels, rate).map_err(|e| e.to_string())?;
        let result = check(unsafe { (client.vtbl().start)(client.ptr) }, "IAudioClient::Start")
            .and_then(|()| pump(&capture, &mut wav, channels, rate, sample, running));
        unsafe { (client.vtbl().stop)(client.ptr) };
        // What was captured stays usable after a device error (e.g. unplugged headphones)
        let finished = wav.finish().map_err(|e| e.to_string());
        result.and(finished)
    }

    /// Copies packets into `wav` until `running` is cleared.
    fn pump(
        capture: &Com<CaptureVtbl>,
        wav: &mut Wav,
        channels: u16,
        rate: u32,
        sample: Sample,
        running: &AtomicBool,
    ) -> Result<(), String> {
        let rate = u64::from(rate);
        let start = now();
        // Fills the time up to `until` with silence if it lies more than GAP ahead
        let fill = |wav: &mut Wav, until: u64| {
            let end = start + wav.frames() * HNS_PER_SECOND / rate;
            if until > end + GAP {
                wav.silence((until - end) * rate / HNS_PER_SECOND).map_err(|e| e.to_string())?;
            }
            Ok::<(), String>(())
        };
        let block = usize::from(channels) * sample.bytes();
        let mut samples = Vec::new();
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(POLL);
            loop {
                let mut frames = 0u32;
                check(unsafe { (capture.vtbl().get_next_packet_size)(capture.ptr, &mut frames) }, "GetNextPacketSize")?;
                if frames == 0 {
                    break;
                }
                let (mut data, mut flags, mut time) = (std::ptr::null_mut(), 0u32, 0u64);
                let hr = unsafe {
                    (capture.vtbl().get_buffer)(capture.ptr, &mut data, &mut frames, &mut flags, std::ptr::null_mut(), &mut time)
                };
                check(hr, "IAudioCaptureClient::GetBuffer")?;
                let mut written = Ok(());
                if flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR as u32 == 0 {
                    written = fill(wav, time);
                }
                if written.is_ok() {
                    written = if flags & AUDCLNT_BUFFERFLAGS_SILENT as u32 != 0 || data.is_null() {
                        wav.silence(u64::from(frames)).map_err(|e| e.to_string())
                    } else {
                        let bytes = unsafe { std::slice::from_raw_parts(data, frames as usize * block) };
                        samples.clear();
                        sample.convert(bytes, &mut samples);
                        wav.write(&samples).map_err(|e| e.to_string())
                    };
                }
                unsafe { (capture.vtbl().release_buffer)(capture.ptr, frames) };
                written?;
            }
        }
        // Nothing may have played up to the stop
        fill(wav, now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_become_16_bit() {
        let mut out = Vec::new();
        let floats: Vec<u8> = [0.5f32, -1.0, 2.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        Sample::F32.convert(&floats, &mut out);
        assert_eq!(out, [16384, -32767, 32767]);
        out.clear();
        Sample::I24.convert(&[0x11, 0x34, 0x12, 0xff, 0xff, 0x80], &mut out);
        Sample::I32.convert(&0x1234_5678i32.to_le_bytes(), &mut out);
        assert_eq!(out, [0x1234, -0x7f01, 0x1234]);
    }

    #[test]
    fn wav_header_gets_the_sizes() {
        let path = std::env::temp_dir().join(format!("nvencffx-wav-{}.wav", std::process::id()));
        let mut wav = Wav::create(&path, 2, 48000).unwrap();
        wav.write(&[1, -1, 2, -2]).unwrap();
        wav.silence(3).unwrap();
        assert_eq!(wav.frames(), 5);
        wav.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(bytes.len(), 44 + 20);
        assert_eq!((&bytes[..4], u32_at(4), &bytes[8..16]), (&b"RIFF"[..], 36 + 20, &b"WAVEfmt "[..]));
        assert_eq!((u32_at(24), u32_at(28), u32_at(40)), (48000, 48000 * 4, 20));
        assert_eq!(&bytes[44..48], &[1, 0, 0xff, 0xff]);
    }

    /// Records the default playback device for a moment. Machines without one
    /// leave no file, which only skips the checks.
    #[cfg(windows)]
    #[test]
    fn loopback_fills_silence_up_to_the_stop() {
        let path = std::env::temp_dir().join(format!("nvencffx-loopback-{}.wav", std::process::id()));
        let capture = Capture::start(path.clone());
        std::thread::sleep(Duration::from_millis(600));
        capture.finish(Duration::from_secs(5));
        let Ok(bytes) = std::fs::read(&path) else { return };
        let _ = std::fs::remove_file(&path);
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let seconds = f64::from(u32_at(40)) / f64::from(u32_at(28));
        assert!((0.45..1.0).contains(&seconds), "{seconds} s of audio for 0.6 s");
    }
}
