use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use tracing::info;

mod audio;
mod clip_video;

pub use audio::{MAX_SOUND_SECONDS, SoundContainer};

use crate::clip::{Clip, Frame, bytes_by_frame};
use crate::error::{ClipError, DecodingError};
use crate::framing_spec::FramingSpec;
use crate::image::{RgbaImage, image_bytes};
use crate::options::Fps;
use crate::process::{
    STDERR_MAX_BYTES, WatchedProcess, launch_thread, read_final_bounded, summarize_stderr,
};
use crate::time::{TimePlan, frame_duration_ms};

pub const MAX_VIDEO_SIZE_BYTES: u64 = 2_000_000_000;

pub const MAX_VIDEO_SIDE_PX: u32 = 16_384;

pub const MAX_IMAGE_SIZE_BYTES: u64 = 50_000_000;

pub const MAX_DECODED_SHORT_SIDE: u32 = 1024;

pub const DEFAULT_TIME_LIMIT: Duration = Duration::from_secs(120);

const VIDEO_FORMATS: &str = "mov,matroska,avi,gif,flv,mpegts,mpeg,ogg,asf";

const IMAGE_FORMATS: &str = "png_pipe,jpeg_pipe,webp_pipe,webp_anim,bmp_pipe";

const IMAGE_MAX_PIXELS: u32 = MAX_VIDEO_SIDE_PX * MAX_VIDEO_SIDE_PX;

const MAX_PAM_HEADER_BYTES: usize = 128;

const CANCELLATION_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoInfo {
    pub duration_s: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
}

pub trait Decoder {
    fn inspect(&self) -> Result<VideoInfo, DecodingError>;

    fn inspect_cancelable(&self, _cancellation: &AtomicBool) -> Result<VideoInfo, DecodingError> {
        self.inspect()
    }

    fn decode(
        &self,
        plan: &TimePlan,
        fps: Fps,
        side: u32,
        cancellation: &AtomicBool,
    ) -> Result<Clip, DecodingError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct FfmpegDecoder {
    video_path: PathBuf,
    ffmpeg_path: PathBuf,
    ffprobe_path: PathBuf,
    time_limit: Duration,
    max_size_bytes: u64,
    framing: FramingSpec,
}

impl FfmpegDecoder {
    pub fn new(video_path: impl Into<PathBuf>) -> Self {
        FfmpegDecoder {
            video_path: video_path.into(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            ffprobe_path: PathBuf::from("ffprobe"),
            time_limit: DEFAULT_TIME_LIMIT,
            max_size_bytes: MAX_VIDEO_SIZE_BYTES,
            framing: FramingSpec::default(),
        }
    }

    pub fn with_executables(self, ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> Self {
        FfmpegDecoder {
            ffmpeg_path: ffmpeg.into(),
            ffprobe_path: ffprobe.into(),
            ..self
        }
    }

    pub fn with_time_limit(self, time_limit: Duration) -> Self {
        FfmpegDecoder { time_limit, ..self }
    }

    pub fn with_framing(self, framing: FramingSpec) -> Self {
        FfmpegDecoder { framing, ..self }
    }

    pub fn with_max_size(self, bytes: u64) -> Self {
        FfmpegDecoder {
            max_size_bytes: bytes,
            ..self
        }
    }

    pub fn validate_file(&self) -> Result<PathBuf, DecodingError> {
        self.validate_file_of_until(self.max_size_bytes)
    }

    fn validate_file_of_until(&self, max_bytes: u64) -> Result<PathBuf, DecodingError> {
        let path = &self.video_path;
        let reading_error = |source: io::Error| {
            if source.kind() == ErrorKind::NotFound {
                DecodingError::FileNotFound { path: path.clone() }
            } else {
                DecodingError::UnreadableFile {
                    path: path.clone(),
                    source,
                }
            }
        };
        let canonical = fs::canonicalize(path).map_err(reading_error)?;
        let metadata = fs::metadata(&canonical).map_err(reading_error)?;
        if !metadata.is_file() {
            return Err(DecodingError::NotRegularFile { path: path.clone() });
        }
        if metadata.len() > max_bytes {
            return Err(DecodingError::FileTooLarge {
                bytes: metadata.len(),
                max_bytes,
            });
        }
        Ok(canonical)
    }

    pub fn inspect_image(&self) -> Result<ImageInfo, DecodingError> {
        self.probe_image(None)
    }

    pub fn inspect_image_cancelable(
        &self,
        cancellation: &AtomicBool,
    ) -> Result<ImageInfo, DecodingError> {
        self.probe_image(Some(cancellation))
    }

    pub fn decode_image(&self, cancellation: &AtomicBool) -> Result<RgbaImage, DecodingError> {
        self.probe_image(Some(cancellation))?;

        let mut command = Command::new(&self.ffmpeg_path);
        command.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        image_input(&mut command);
        command
            .args(["-an", "-sn", "-dn"])
            .args(["-vf", &image_filters()])
            .args(["-frames:v", "1"])
            .args(["-f", "image2pipe", "-c:v", "pam", "pipe:1"]);
        let image = run(
            command,
            self.open_image()?.into(),
            &self.ffmpeg_path,
            self.time_limit,
            Some(cancellation),
            read_pam,
        )?;
        info!(
            width = image.width(),
            height = image.height(),
            "imagen decodificada"
        );
        Ok(image)
    }

    /// One RGBA frame of `side`x`side` (the video covering the square, centre cropped) taken at
    /// `at_s` seconds. It is raw pixels so no encoder of the ffmpeg build is needed.
    pub fn frame_at(
        &self,
        at_s: f64,
        side: u32,
        cancellation: &AtomicBool,
    ) -> Result<Vec<u8>, DecodingError> {
        let path = self.validate_file()?;
        let bytes_frame = bytes_by_frame(side).ok_or(ClipError::SideNotValid { side })?;
        let filters = format!(
            "scale=iw*sar:ih,setsar=1,\
             scale={side}:{side}:force_original_aspect_ratio=increase:flags=bilinear,\
             crop={side}:{side},format=rgba"
        );
        let mut command = Command::new(&self.ffmpeg_path);
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(["-ss", &ffmpeg_seconds(at_s)])
            .args(video_input(&path))
            .args(["-an", "-sn", "-dn", "-vf", &filters])
            .args(["-frames:v", "1"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
        let mut frames = run(
            command,
            Stdio::null(),
            &self.ffmpeg_path,
            self.time_limit,
            Some(cancellation),
            move |stdout| read_frames(stdout, bytes_frame, 1),
        )?;
        frames
            .pop()
            .map(|frame| frame.to_vec())
            .ok_or(DecodingError::VideoWithoutFrames)
    }

    /// The frame at `at_s` seconds with the video's own aspect ratio (no crop), scaled down so its
    /// longer side is at most `max_side`. For stills where the framing square must match the video.
    pub fn whole_frame_at(
        &self,
        at_s: f64,
        max_side: u32,
        cancellation: &AtomicBool,
    ) -> Result<RgbaImage, DecodingError> {
        let path = self.validate_file()?;
        let scale = format!("min(1\\,{max_side}/max(iw*sar\\,ih))");
        let filters = format!(
            "scale=w=max(1\\,round(iw*sar*{scale})):h=max(1\\,round(ih*{scale})):flags=bilinear,\
             setsar=1,format=rgba"
        );
        let mut command = Command::new(&self.ffmpeg_path);
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(["-ss", &ffmpeg_seconds(at_s)])
            .args(video_input(&path))
            .args(["-an", "-sn", "-dn", "-vf", &filters])
            .args(["-frames:v", "1"])
            .args(["-f", "image2pipe", "-c:v", "pam", "pipe:1"]);
        run(
            command,
            Stdio::null(),
            &self.ffmpeg_path,
            self.time_limit,
            Some(cancellation),
            read_pam,
        )
    }

    fn probe_video(&self, cancellation: Option<&AtomicBool>) -> Result<VideoInfo, DecodingError> {
        let path = self.validate_file()?;
        let mut command = Command::new(&self.ffprobe_path);
        command
            .args(["-v", "error", "-select_streams", "v:0"])
            .args(["-show_entries", "stream=width,height:format=duration"])
            .args(["-of", "default=noprint_wrappers=1"])
            .args(video_input(&path));
        let output = run(
            command,
            Stdio::null(),
            &self.ffprobe_path,
            self.time_limit,
            cancellation,
            read_text,
        )?;
        let mut info = interpret_ffprobe(&output)?;
        info.has_audio = self.audio_codec(cancellation)?.is_some();
        info!(
            file = %self.video_name(),
            duration_s = info.duration_s,
            "video inspeccionado"
        );
        Ok(info)
    }

    fn probe_image(&self, cancellation: Option<&AtomicBool>) -> Result<ImageInfo, DecodingError> {
        let input = self.open_image()?;
        let mut command = Command::new(&self.ffprobe_path);
        command
            .args(["-v", "error", "-select_streams", "v:0"])
            .args(["-show_entries", "stream=width,height"])
            .args(["-of", "default=noprint_wrappers=1"]);
        image_input(&mut command);
        let output = run(
            command,
            input.into(),
            &self.ffprobe_path,
            self.time_limit,
            cancellation,
            read_text,
        )?;
        let (width, height) = interpret_sides(&output)?;
        Ok(ImageInfo { width, height })
    }

    fn open_image(&self) -> Result<File, DecodingError> {
        let max_bytes = self.max_size_bytes.min(MAX_IMAGE_SIZE_BYTES);
        let canonical = self.validate_file_of_until(max_bytes)?;
        File::open(&canonical).map_err(|source| DecodingError::UnreadableFile {
            path: self.video_path.clone(),
            source,
        })
    }

    fn video_name(&self) -> String {
        self.video_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

impl Decoder for FfmpegDecoder {
    fn inspect(&self) -> Result<VideoInfo, DecodingError> {
        self.probe_video(None)
    }

    fn inspect_cancelable(&self, cancellation: &AtomicBool) -> Result<VideoInfo, DecodingError> {
        self.probe_video(Some(cancellation))
    }

    fn decode(
        &self,
        plan: &TimePlan,
        fps: Fps,
        side: u32,
        cancellation: &AtomicBool,
    ) -> Result<Clip, DecodingError> {
        let path = self.validate_file()?;
        let bytes_frame = bytes_by_frame(side).ok_or(ClipError::SideNotValid { side })?;
        let frames_max = plan.frame_count(fps);
        if frames_max == 0 {
            return Err(DecodingError::VideoWithoutFrames);
        }

        let mut command = Command::new(&self.ffmpeg_path);
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(["-ss", &ffmpeg_seconds(plan.start_s)])
            .args(["-t", &ffmpeg_seconds(plan.input_duration_s)])
            .args(video_input(&path))
            .args(["-an", "-sn", "-dn"])
            .args([
                "-vf",
                &filters_string(plan.factor_speed, fps, side, &self.framing),
            ])
            .args(["-frames:v", &frames_max.to_string()])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
        let rgba_frames = run(
            command,
            Stdio::null(),
            &self.ffmpeg_path,
            self.time_limit,
            Some(cancellation),
            move |stdout| read_frames(stdout, bytes_frame, frames_max),
        )?;
        if rgba_frames.is_empty() {
            return Err(DecodingError::VideoWithoutFrames);
        }

        let frames: Vec<Frame> = rgba_frames
            .into_iter()
            .zip(0u64..)
            .map(|(rgba, index)| Frame {
                rgba,
                duration_ms: frame_duration_ms(index, fps),
            })
            .collect();
        info!(
            file = %self.video_name(),
            frames = frames.len(),
            side,
            fps = fps.value(),
            "video decodificado"
        );
        Ok(Clip::new(side, frames)?)
    }
}

fn ffmpeg_input(absolute_path: &Path) -> OsString {
    let mut input = OsString::from("file:");
    input.push(absolute_path);
    input
}

fn video_input(absolute_path: &Path) -> [OsString; 6] {
    [
        "-format_whitelist".into(),
        VIDEO_FORMATS.into(),
        "-protocol_whitelist".into(),
        "file".into(),
        "-i".into(),
        ffmpeg_input(absolute_path),
    ]
}

fn image_input(command: &mut Command) {
    command
        .args(["-format_whitelist", IMAGE_FORMATS])
        .args(["-protocol_whitelist", "pipe"])
        .args(["-max_pixels", &IMAGE_MAX_PIXELS.to_string()])
        .args(["-i", "pipe:0"]);
}

fn image_filters() -> String {
    let scale = format!(
        "min(1\\,min({MAX_DECODED_SHORT_SIDE}/min(iw*sar\\,ih)\\,\
         {MAX_VIDEO_SIDE_PX}/max(iw*sar\\,ih)))"
    );
    format!(
        "scale=w=max(1\\,round(iw*sar*{scale})):h=max(1\\,round(ih*{scale})):flags=lanczos,\
         setsar=1,format=rgba"
    )
}

fn filters_string(factor_speed: f64, fps: Fps, side: u32, framing: &FramingSpec) -> String {
    format!(
        "scale=iw*sar:ih,setsar=1,\
         setpts=(PTS-STARTPTS)/{factor_speed:.6},\
         fps={},\
         {}",
        fps.value(),
        framing.canvas_filter(side)
    )
}

fn ffmpeg_seconds(seconds: f64) -> String {
    format!("{seconds:.6}")
}

/// Reads through one reused buffer and keeps each frame in its final `Arc`, so the full clip is
/// never held twice and the allocator does not churn with frame-sized blocks.
fn read_frames(
    mut stdout: impl Read,
    bytes_frame: usize,
    max: usize,
) -> io::Result<Vec<Arc<[u8]>>> {
    let mut frames = Vec::new();
    let mut buffer = vec![0; bytes_frame];
    while frames.len() < max {
        let read = read_block(&mut stdout, &mut buffer)?;
        if read == 0 {
            break;
        }
        if read < bytes_frame {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                format!("frame incompleto: {read} de {bytes_frame} bytes"),
            ));
        }
        frames.push(Arc::from(buffer.as_slice()));
    }
    Ok(frames)
}

fn read_text(mut stdout: impl Read) -> io::Result<String> {
    let mut text = String::new();
    stdout.read_to_string(&mut text)?;
    Ok(text)
}

fn read_pam(mut stdout: impl Read) -> io::Result<RgbaImage> {
    let not_valid = |reason| io::Error::new(ErrorKind::InvalidData, reason);
    let mut header = Vec::new();
    while !header.ends_with(b"ENDHDR\n") {
        if header.len() == MAX_PAM_HEADER_BYTES {
            return Err(not_valid("la cabecera de la imagen no termina"));
        }
        let mut byte = [0];
        stdout.read_exact(&mut byte)?;
        header.extend(byte);
    }
    let header = String::from_utf8_lossy(&header);
    let field = |key: &str| {
        header
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|value| value.trim().parse::<u32>().ok())
    };
    let (Some(width), Some(height), Some(4), Some(255)) = (
        field("WIDTH "),
        field("HEIGHT "),
        field("DEPTH "),
        field("MAXVAL "),
    ) else {
        return Err(not_valid("la cabecera de la imagen no es RGBA"));
    };
    if width.min(height) > MAX_DECODED_SHORT_SIDE {
        return Err(not_valid("la imagen decodificada no se redujo"));
    }
    let bytes = image_bytes(width, height)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    let mut rgba = vec![0; bytes];
    stdout.read_exact(&mut rgba)?;
    RgbaImage::new(width, height, rgba)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn read_block(reader: &mut impl Read, destination: &mut [u8]) -> io::Result<usize> {
    let mut read = 0;
    while read < destination.len() {
        match reader.read(&mut destination[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(read)
}

fn interpret_ffprobe(output: &str) -> Result<VideoInfo, DecodingError> {
    let (width, height) = interpret_sides(output)?;

    let duration_text = ffprobe_value(output, "duration").unwrap_or_default();
    let duration_s: f64 = duration_text
        .parse()
        .map_err(|_| DecodingError::UnreadableDuration {
            text: duration_text.to_string(),
        })?;
    if !(duration_s.is_finite() && duration_s > 0.0) {
        return Err(DecodingError::DurationNotValid { duration_s });
    }

    Ok(VideoInfo {
        duration_s,
        width,
        height,
        has_audio: false,
    })
}

fn ffprobe_value<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
    })
}

fn interpret_sides(output: &str) -> Result<(u32, u32), DecodingError> {
    let value = |key| ffprobe_value(output, key);
    let (width, height) = match (value("width"), value("height")) {
        (None, None) => return Err(DecodingError::WithoutVideoTrack),
        (width, height) => (
            width.and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
            height.and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
        ),
    };
    let valid_side = 1..=MAX_VIDEO_SIDE_PX;
    if !valid_side.contains(&width) || !valid_side.contains(&height) {
        return Err(DecodingError::DimensionsNotValid {
            width,
            height,
            max: MAX_VIDEO_SIDE_PX,
        });
    }
    Ok((width, height))
}

fn run<T: Send + 'static>(
    mut command: Command,
    input: Stdio,
    executable: &Path,
    time_limit: Duration,
    cancellation: Option<&AtomicBool>,
    read_output: impl FnOnce(ChildStdout) -> io::Result<T> + Send + 'static,
) -> Result<T, DecodingError> {
    let program = executable.display().to_string();
    let start = Instant::now();
    let child = command
        .stdin(input)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| {
            if source.kind() == ErrorKind::NotFound {
                DecodingError::FfmpegNotFound {
                    program: program.clone(),
                }
            } else {
                DecodingError::ProcessNotLaunched {
                    program: program.clone(),
                    source,
                }
            }
        })?;
    let mut process = WatchedProcess::new(child, program);
    let without_pipe = |program: &str| DecodingError::ProcessNotLaunched {
        program: program.to_string(),
        source: io::Error::other("no se pudo abrir el pipe"),
    };
    let stdout = process
        .child
        .stdout
        .take()
        .ok_or_else(|| without_pipe(&process.program))?;
    let mut stderr = process
        .child
        .stderr
        .take()
        .ok_or_else(|| without_pipe(&process.program))?;

    let thread_not_launched = |source| DecodingError::ThreadNotLaunched { source };
    let stderr_reader = launch_thread("ffmpeg-stderr".to_string(), move || {
        read_final_bounded(&mut stderr, STDERR_MAX_BYTES)
    })
    .map_err(thread_not_launched)?;
    let (sending, reception) = mpsc::channel();
    launch_thread("ffmpeg-stdout".to_string(), move || {
        let _ = sending.send(read_output(stdout));
    })
    .map_err(thread_not_launched)?;

    let cancelled = || cancellation.is_some_and(|flag| flag.load(Ordering::Relaxed));
    let reading = loop {
        if cancelled() {
            return Err(DecodingError::Cancelled);
        }
        let remaining = time_limit.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err(DecodingError::TimedOut {
                program: process.program.clone(),
                limit: time_limit,
            });
        }
        match reception.recv_timeout(remaining.min(CANCELLATION_INTERVAL)) {
            Ok(reading) => break reading,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                break Err(io::Error::other(
                    "el hilo que leía la salida terminó sin resultado",
                ));
            }
        }
    };
    let state = process
        .wait_until(start, time_limit)
        .map_err(|source| DecodingError::ProcessNotLaunched {
            program: process.program.clone(),
            source,
        })?
        .ok_or_else(|| DecodingError::TimedOut {
            program: process.program.clone(),
            limit: time_limit,
        })?;
    // On Ctrl-C in a terminal ffmpeg gets the signal too and exits with an error:
    // that is a cancellation, not an ffmpeg failure.
    if cancelled() {
        return Err(DecodingError::Cancelled);
    }
    let stderr = stderr_reader
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("el hilo que leía stderr falló")))
        .map_err(|source| DecodingError::UnreadableOutput {
            program: process.program.clone(),
            source,
        })?;

    if !state.success() {
        return Err(DecodingError::FfmpegFailed {
            program: process.program.clone(),
            code: state.code(),
            summarized_stderr: summarize_stderr(&String::from_utf8_lossy(&stderr)),
        });
    }
    reading.map_err(|source| DecodingError::UnreadableOutput {
        program: process.program.clone(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interprets_ffprobe_output() {
        let info = interpret_ffprobe("width=640\nheight=360\nduration=3.000000\n").unwrap();
        assert_eq!(
            info,
            VideoInfo {
                duration_s: 3.0,
                width: 640,
                height: 360,
                has_audio: false
            }
        );
    }

    #[test]
    fn without_width_nor_height_has_no_track_of_video() {
        assert!(matches!(
            interpret_ffprobe("duration=1.000000\n"),
            Err(DecodingError::WithoutVideoTrack)
        ));
    }

    #[test]
    fn rejects_huge_or_unreadable_null_dimensions() {
        for output in [
            "width=0\nheight=360\nduration=1\n",
            "width=N/A\nheight=360\nduration=1\n",
            "width=640\nduration=1\n",
            "width=20000\nheight=360\nduration=1\n",
        ] {
            assert!(
                matches!(
                    interpret_ffprobe(output),
                    Err(DecodingError::DimensionsNotValid { .. })
                ),
                "{output:?}"
            );
        }
    }

    #[test]
    fn rejects_durations_unreadable_or_not_positive() {
        assert!(matches!(
            interpret_ffprobe("width=640\nheight=360\nduration=N/A\n"),
            Err(DecodingError::UnreadableDuration { .. })
        ));
        assert!(matches!(
            interpret_ffprobe("width=640\nheight=360\n"),
            Err(DecodingError::UnreadableDuration { .. })
        ));
        for duration in ["0", "-1", "inf", "NaN"] {
            assert!(
                matches!(
                    interpret_ffprobe(&format!("width=640\nheight=360\nduration={duration}\n")),
                    Err(DecodingError::DurationNotValid { .. })
                ),
                "{duration}"
            );
        }
    }

    #[test]
    fn string_of_filters_follows_filter_order() {
        assert_eq!(
            filters_string(2.0, Fps::new(15).unwrap(), 512, &FramingSpec::default()),
            "scale=iw*sar:ih,setsar=1,setpts=(PTS-STARTPTS)/2.000000,fps=15,\
             crop=w=max(1\\,floor(min(iw\\,ih)/1.000000)):h=max(1\\,floor(min(iw\\,ih)/1.000000)):\
             x=max(0\\,min(iw-ow\\,0.500000*iw-ow/2)):y=max(0\\,min(ih-oh\\,0.500000*ih-oh/2)),\
             scale=512:512:flags=lanczos,format=rgba"
        );
    }

    #[test]
    fn reads_frames_complete_until_eof_or_max() {
        let data: Vec<u8> = (0..12).collect();
        let frames = read_frames(data.as_slice(), 4, 10).unwrap();
        assert_eq!(
            frames,
            vec![
                Arc::from([0, 1, 2, 3]),
                Arc::from([4, 5, 6, 7]),
                Arc::from([8, 9, 10, 11])
            ]
        );

        let frames = read_frames(data.as_slice(), 4, 2).unwrap();
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn frame_incomplete_is_error() {
        let data = [0u8; 6];
        let error = read_frames(data.as_slice(), 4, 10).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn reads_pam_image() {
        let mut pam = b"P7\nWIDTH 2\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\n\
TUPLTYPE RGB_ALPHA\nENDHDR\n"
            .to_vec();
        pam.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        let image = read_pam(pam.as_slice()).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(image.rgba(), [1, 2, 3, 4, 5, 6, 7, 8]);
        pam.pop();
        assert_eq!(
            read_pam(pam.as_slice()).unwrap_err().kind(),
            ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn header_pam_oversized_rejects_without_reserve() {
        for header in [
            "P7\nWIDTH 4294967295\nHEIGHT 4294967295\nDEPTH 4\nMAXVAL 255\nENDHDR\n",
            "P7\nWIDTH 2000\nHEIGHT 2000\nDEPTH 4\nMAXVAL 255\nENDHDR\n",
            "P7\nWIDTH 16385\nHEIGHT 8\nDEPTH 4\nMAXVAL 255\nENDHDR\n",
            "P7\nWIDTH 2\nHEIGHT 1\nDEPTH 3\nMAXVAL 255\nENDHDR\n",
            "P7\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nENDHDR\n",
        ] {
            let error = read_pam(header.as_bytes()).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidData, "{header:?}");
        }
        let without_end = [b'#'; 4096];
        assert_eq!(
            read_pam(without_end.as_slice()).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }

    #[test]
    fn input_of_ffmpeg_is_path_absolute_with_protocol_file() {
        assert_eq!(
            ffmpeg_input(Path::new("/videos/concat:a.ts|-b.ts")),
            "file:/videos/concat:a.ts|-b.ts"
        );
    }

    #[test]
    fn whole_frame_keeps_the_aspect_ratio_and_caps_the_longer_side() {
        let directory = tempfile::tempdir().unwrap();
        let video = directory.path().join("video.mp4");
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=10"])
            .args(["-t", "1", "-c:v", "mpeg4", "-pix_fmt", "yuv420p"])
            .arg(&video)
            .output()
            .expect("hace falta ffmpeg para estas pruebas");
        assert!(output.status.success());
        let cancellation = AtomicBool::new(false);

        let big = FfmpegDecoder::new(&video)
            .whole_frame_at(0.5, 640, &cancellation)
            .unwrap();
        assert_eq!((big.width(), big.height()), (640, 360));

        let tiny = FfmpegDecoder::new(&video)
            .whole_frame_at(0.5, 2000, &cancellation)
            .unwrap();
        assert_eq!((tiny.width(), tiny.height()), (1280, 720));
    }
}
