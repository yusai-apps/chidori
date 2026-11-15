//! Screenshots (section 9): capture, full-size PNG, 320 px thumbnail.

use std::path::Path;

use base64::Engine as _;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use serde::{Deserialize, Serialize};

pub const THUMB_WIDTH: u32 = 320;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    CursorMonitor,
    AllMonitors,
    /// The largest visible window of this process tree.
    AppWindow {
        root_pid: u32,
    },
    /// One window the user picked from the camera's menu (FR-34).
    Window {
        id: u32,
    },
}

/// An open window, as the camera's menu lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    /// The program's name ("PowerPoint"), from its version info.
    pub app: String,
}

/// The menu lists at most this many windows, front first.
pub const MAX_WINDOWS: usize = 20;

pub struct Captured {
    pub image: RgbaImage,
    /// `monitor:1`, `window:1234`.
    pub label: String,
}

pub struct CaptureResult {
    pub images: Vec<Captured>,
    /// The app window was not found; the cursor's monitor was used instead.
    pub fell_back: bool,
}

/// OS-specific capture behind a trait (section 6).
pub trait ScreenCapturer: Send + Sync {
    fn capture(&self, target: CaptureTarget) -> anyhow::Result<CaptureResult>;
    /// The windows a screenshot could take, front first, Chidori's own left out.
    fn windows(&self) -> anyhow::Result<Vec<WindowInfo>>;
}

#[cfg(any(windows, target_os = "macos"))]
pub struct XcapCapturer;

/// Where there is no screen capture yet (Linux, which builds so the app can be tested on CI and
/// in the cloud): a screenshot fails with a message, and there are no windows to pick.
#[cfg(not(any(windows, target_os = "macos")))]
pub struct NoCapturer;

#[cfg(not(any(windows, target_os = "macos")))]
impl ScreenCapturer for NoCapturer {
    fn capture(&self, _target: CaptureTarget) -> anyhow::Result<CaptureResult> {
        anyhow::bail!("screenshots are not available on this OS yet")
    }

    fn windows(&self) -> anyhow::Result<Vec<WindowInfo>> {
        Ok(Vec::new())
    }
}

#[cfg(any(windows, target_os = "macos"))]
impl XcapCapturer {
    fn cursor_monitor() -> anyhow::Result<Captured> {
        let (x, y) = crate::platform::cursor_pos().unwrap_or((0, 0));
        let monitor = xcap::Monitor::from_point(x, y).or_else(|_| {
            xcap::Monitor::all()?
                .into_iter()
                .find(|m| m.is_primary().unwrap_or(false))
                .ok_or(xcap::XCapError::new("no monitor"))
        })?;
        let index = xcap::Monitor::all()
            .ok()
            .and_then(|all| all.iter().position(|m| m.id().ok() == monitor.id().ok()))
            .map_or(1, |i| i + 1);
        Ok(Captured { image: monitor.capture_image()?, label: format!("monitor:{index}") })
    }
}

#[cfg(any(windows, target_os = "macos"))]
impl ScreenCapturer for XcapCapturer {
    fn capture(&self, target: CaptureTarget) -> anyhow::Result<CaptureResult> {
        match target {
            CaptureTarget::CursorMonitor => {
                Ok(CaptureResult { images: vec![Self::cursor_monitor()?], fell_back: false })
            }
            CaptureTarget::AllMonitors => {
                let mut images = Vec::new();
                for (i, m) in xcap::Monitor::all()?.into_iter().enumerate() {
                    images.push(Captured { image: m.capture_image()?, label: format!("monitor:{}", i + 1) });
                }
                Ok(CaptureResult { images, fell_back: false })
            }
            CaptureTarget::AppWindow { root_pid } => {
                let tree: std::collections::HashSet<u32> = crate::audio::process_tree(root_pid).into_iter().collect();
                let best = xcap::Window::all()?
                    .into_iter()
                    .filter(|w| w.pid().is_ok_and(|p| tree.contains(&p)))
                    .filter(|w| !w.is_minimized().unwrap_or(true))
                    .filter(|w| w.width().unwrap_or(0) > 50 && w.height().unwrap_or(0) > 50)
                    .max_by_key(|w| w.width().unwrap_or(0) as u64 * w.height().unwrap_or(0) as u64);
                if let Some(w) = best
                    && let Ok(image) = w.capture_image()
                {
                    return Ok(CaptureResult {
                        images: vec![Captured { image, label: format!("window:{}", w.pid().unwrap_or(root_pid)) }],
                        fell_back: false,
                    });
                }
                Ok(CaptureResult { images: vec![Self::cursor_monitor()?], fell_back: true })
            }
            CaptureTarget::Window { id } => {
                let window = xcap::Window::all()?
                    .into_iter()
                    .find(|w| w.id().ok() == Some(id) && !w.is_minimized().unwrap_or(true));
                if let Some(w) = window
                    && let Ok(image) = w.capture_image()
                {
                    let label = format!("window:{}", w.pid().unwrap_or(0));
                    return Ok(CaptureResult { images: vec![Captured { image, label }], fell_back: false });
                }
                // Closed or minimized since it was picked.
                Ok(CaptureResult { images: vec![Self::cursor_monitor()?], fell_back: true })
            }
        }
    }

    fn windows(&self) -> anyhow::Result<Vec<WindowInfo>> {
        let own = std::process::id();
        Ok(xcap::Window::all()?
            .into_iter()
            .filter(|w| w.pid().is_ok_and(|p| p != own))
            .filter(|w| !w.is_minimized().unwrap_or(true))
            .filter(|w| w.width().unwrap_or(0) >= 100 && w.height().unwrap_or(0) >= 60)
            .filter_map(|w| {
                let title = w.title().ok()?.trim().to_string();
                if title.is_empty() {
                    return None;
                }
                Some(WindowInfo { id: w.id().ok()?, title, app: w.app_name().unwrap_or_default() })
            })
            .take(MAX_WINDOWS)
            .collect())
    }
}

/// Full resolution PNG with fast compression.
pub fn save_png(image: &RgbaImage, path: &Path) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("png.tmp");
    {
        let file = std::fs::File::create(&tmp)?;
        let writer = std::io::BufWriter::with_capacity(1 << 20, file);
        PngEncoder::new_with_quality(writer, CompressionType::Fast, FilterType::Adaptive).write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 320 px wide JPEG (quality 80) as a data URL.
pub fn thumbnail_data_url(image: &RgbaImage) -> anyhow::Result<String> {
    let (w, h) = image.dimensions();
    let tw = THUMB_WIDTH.min(w.max(1));
    let th = ((h as u64 * tw as u64) / w.max(1) as u64).max(1) as u32;
    let thumb = image::imageops::thumbnail(image, tw, th);
    let rgb = image::DynamicImage::ImageRgba8(thumb).to_rgb8();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 80).encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg)))
}

/// Downscales to at most `max_width` and encodes JPEG (for the PDF export).
pub fn jpeg_for_print(path: &Path, max_width: u32, quality: u8) -> anyhow::Result<Vec<u8>> {
    let img = image::open(path)?;
    let img = if img.width() > max_width {
        img.resize(max_width, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(out)
}

/// The shutter sound as a WAV file: two short bursts of decaying noise, a shutter opening and
/// closing. Made here rather than shipped, so the installer carries no audio asset.
pub fn shutter_wav() -> Vec<u8> {
    const RATE: u32 = 22_050;
    const LENGTH_MS: u32 = 110;
    // (start ms, decay ms, gain); the second, softer burst is the shutter closing.
    const BURSTS: [(f32, f32, f32); 2] = [(0.0, 6.0, 0.35), (55.0, 8.0, 0.22)];
    let spec =
        hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut out = std::io::Cursor::new(Vec::new());
    let mut wav = hound::WavWriter::new(&mut out, spec).expect("in-memory WAV");
    let mut seed = 0x2545_f491_u32;
    let mut low = 0.0f32;
    for i in 0..RATE * LENGTH_MS / 1000 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let noise = seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
        // A one-pole low-pass takes the hiss off, so it clicks rather than crackles.
        low += 0.45 * (noise - low);
        let ms = i as f32 * 1000.0 / RATE as f32;
        let envelope: f32 = BURSTS
            .iter()
            .filter(|(start, ..)| ms >= *start)
            .map(|(start, decay, gain)| gain * (-(ms - start) / decay).exp())
            .sum();
        wav.write_sample((low * envelope * i16::MAX as f32) as i16).expect("in-memory WAV");
    }
    wav.finalize().expect("in-memory WAV");
    out.into_inner()
}

/// `images/0001_151603.png`, with `-m1`, `-m2` when several monitors share one capture.
pub fn file_name(counter: u64, local_hms: &str, monitor_suffix: Option<usize>) -> String {
    match monitor_suffix {
        Some(m) => format!("images/{counter:04}_{local_hms}-m{m}.png"),
        None => format!("images/{counter:04}_{local_hms}.png"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(file_name(1, "151603", None), "images/0001_151603.png");
        assert_eq!(file_name(12, "090000", Some(2)), "images/0012_090000-m2.png");
    }

    #[test]
    fn shutter_is_a_short_quiet_wav() {
        let wav = shutter_wav();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 22_050, 16));
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 22_050 * 110 / 1000);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 1000 && peak < i16::MAX as u16 / 2, "peak {peak}");
        // It fades out instead of stopping on a click.
        assert!(samples[samples.len() - 50..].iter().all(|s| s.unsigned_abs() < 200));
    }

    #[test]
    fn png_and_thumbnail_round_trip() {
        let img = RgbaImage::from_fn(1280, 720, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("images").join("0001_000000.png");
        save_png(&img, &path).unwrap();
        let back = image::open(&path).unwrap();
        assert_eq!((back.width(), back.height()), (1280, 720));
        let url = thumbnail_data_url(&img).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        let bytes = base64::engine::general_purpose::STANDARD.decode(&url["data:image/jpeg;base64,".len()..]).unwrap();
        let thumb = image::load_from_memory(&bytes).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (320, 180));
        let print = jpeg_for_print(&path, 640, 85).unwrap();
        assert_eq!(image::load_from_memory(&print).unwrap().width(), 640);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "lists the real desktop's windows"]
    fn lists_open_windows_quickly() {
        let t = std::time::Instant::now();
        let windows = XcapCapturer.windows().unwrap();
        println!("{} windows in {:?}", windows.len(), t.elapsed());
        for w in &windows {
            println!("{:>10} {:<24} {}", w.id, w.app, w.title);
        }
        assert!(windows.len() <= MAX_WINDOWS);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "captures the real screen"]
    fn captures_cursor_monitor_quickly() {
        let t = std::time::Instant::now();
        let r = XcapCapturer.capture(CaptureTarget::CursorMonitor).unwrap();
        let capture = t.elapsed();
        let img = &r.images[0].image;
        let t = std::time::Instant::now();
        let dir = tempfile::tempdir().unwrap();
        save_png(img, &dir.path().join("x.png")).unwrap();
        let png = t.elapsed();
        let t = std::time::Instant::now();
        thumbnail_data_url(img).unwrap();
        println!("{}x{} capture {:?} png {:?} thumb {:?}", img.width(), img.height(), capture, png, t.elapsed());
    }
}
