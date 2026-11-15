//! Audio capture and per-source DSP.

pub mod clock;
pub mod decode;
pub mod dsp;
pub mod file_source;
pub mod level;
#[cfg(target_os = "macos")]
pub mod mac;
pub mod resample;
pub mod source;
#[cfg(windows)]
pub mod win;

use std::path::PathBuf;

use crossbeam_channel::Sender;

use crate::session::model::SourceId;
use source::{AudioSource, CaptureTarget, Device, RawChunk, SourceStatus};

/// Sound files that play as a recording's sources in place of the devices, at the speed they
/// were recorded, so a test run of the app hears the same thing every time and never the room.
/// A side without a file is silent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StandIn {
    /// Heard as 相手 (the app or the system).
    pub others: Option<PathBuf>,
    /// Heard as 自分 (the mic).
    pub me: Option<PathBuf>,
}

impl StandIn {
    pub fn source(&self, id: SourceId) -> Box<dyn AudioSource> {
        let file = if id == SourceId::Mic { &self.me } else { &self.others };
        match file {
            Some(path) => {
                Box::new(file_source::FileSource::new(id, path.clone(), None, crate::platform::qpc_now_100ns(), 1.0))
            }
            None => Box::new(Silence { id, tx: None }),
        }
    }
}

/// A source that never sends: a side of a `StandIn` without a file.
struct Silence {
    id: SourceId,
    /// Held so the DSP thread waits as it does on a quiet device.
    tx: Option<Sender<RawChunk>>,
}

impl AudioSource for Silence {
    fn id(&self) -> SourceId {
        self.id
    }

    fn start(&mut self, tx: Sender<RawChunk>) -> anyhow::Result<()> {
        self.tx = Some(tx);
        Ok(())
    }

    fn stop(&mut self) {
        self.tx = None;
    }
}

/// This OS's capture stream for a target (section 6).
pub fn open_source(id: SourceId, target: CaptureTarget, status: Sender<SourceStatus>) -> Box<dyn AudioSource> {
    #[cfg(windows)]
    {
        Box::new(win::capture::WasapiSource::new(id, target, status))
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(mac::capture::CoreAudioSource::new(id, target, status))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (target, status);
        Box::new(NoDevice { id })
    }
}

/// Where there is no device capture yet (Linux): starting fails with a message. A recording
/// still runs from a `StandIn`'s sound files, which is how CI and cloud test runs record.
#[cfg(not(any(windows, target_os = "macos")))]
struct NoDevice {
    id: SourceId,
}

#[cfg(not(any(windows, target_os = "macos")))]
impl AudioSource for NoDevice {
    fn id(&self) -> SourceId {
        self.id
    }

    fn start(&mut self, _tx: Sender<RawChunk>) -> anyhow::Result<()> {
        Err(source::CaptureError::Other("recording from a device is not available on this OS yet".into()).into())
    }

    fn stop(&mut self) {}
}

/// The mics to choose from, the default first (FR-13).
pub fn list_mic_devices() -> anyhow::Result<Vec<Device>> {
    #[cfg(windows)]
    {
        win::devices::list_mic_devices()
    }
    #[cfg(target_os = "macos")]
    {
        mac::devices::list_mic_devices()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Ok(Vec::new())
    }
}

/// Name of a capture device, or of the default one.
pub fn mic_device_name(device_id: Option<String>) -> Option<String> {
    #[cfg(windows)]
    {
        win::devices::mic_device_name(device_id)
    }
    #[cfg(target_os = "macos")]
    {
        mac::devices::mic_device_name(device_id)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = device_id;
        None
    }
}

/// The running root process of this executable, or 0.
pub fn find_app_root(exe: &std::path::Path) -> u32 {
    #[cfg(windows)]
    {
        win::process::find_root_by_exe(&win::process::snapshot(), exe).unwrap_or(0)
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        0
    }
}

/// A process and its descendants (an app's helper processes).
pub fn process_tree(root_pid: u32) -> Vec<u32> {
    #[cfg(windows)]
    {
        win::process::tree_of(&win::process::snapshot(), root_pid)
    }
    #[cfg(not(windows))]
    {
        vec![root_pid]
    }
}
