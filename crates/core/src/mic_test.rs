//! The setup wizard's 10-second mic test: level meter plus the first transcribed words.

use std::time::{Duration, Instant};

use crate::host::AppHandle;
use serde::Serialize;

use crate::asr::oneshot;
use crate::asr::worker::AsrConfig;
use crate::audio::clock::downmix;
use crate::audio::level::{FLOOR_DBFS, to_dbfs};
use crate::audio::resample::MonoResampler;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::events;
use crate::session::model::SourceId;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicTestResult {
    pub peak_dbfs: f32,
    pub text: String,
    pub device: Option<String>,
}

pub fn run(app: &AppHandle, st: &AppState, seconds: u64) -> AppResult<MicTestResult> {
    use crate::audio::source::{CaptureError, CaptureTarget};
    if !matches!(&*st.recorder.lock(), crate::recorder::Phase::Idle) {
        return Err(AppError::internal("not while recording"));
    }
    let settings = st.settings.read().clone();
    let device = crate::audio::mic_device_name(settings.source.mic_device_id.clone());
    let (status_tx, _status_rx) = crossbeam_channel::unbounded();
    let (tx, rx) = crossbeam_channel::bounded(400);
    let mut source = crate::audio::open_source(
        SourceId::Mic,
        CaptureTarget::Mic { device_id: settings.source.mic_device_id.clone() },
        status_tx,
    );
    source.start(tx).map_err(|e| match e.downcast_ref::<CaptureError>() {
        Some(CaptureError::MicDenied) => AppError::new(ErrorCode::MicDenied, e.to_string()),
        _ => AppError::new(ErrorCode::SourceLost, format!("{e:#}")),
    })?;

    let started = Instant::now();
    let mut mono = Vec::new();
    let mut rate = 48_000;
    let mut window_peak = 0f32;
    let mut overall_peak = 0f32;
    let mut last_emit = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(50)) {
            rate = chunk.rate;
            let before = mono.len();
            downmix(&chunk.samples, chunk.channels, &mut mono);
            let peak = mono[before..].iter().fold(0f32, |p, s| p.max(s.abs()));
            window_peak = window_peak.max(peak);
            overall_peak = overall_peak.max(peak);
        }
        if last_emit.elapsed() >= Duration::from_millis(100) {
            last_emit = Instant::now();
            emit_level(app, to_dbfs(window_peak));
            window_peak = 0.0;
        }
    }
    source.stop();
    emit_level(app, FLOOR_DBFS);

    let mut resampler = MonoResampler::new(rate).map_err(AppError::from)?;
    let mut samples = Vec::new();
    resampler.process(&mono, &mut samples).map_err(AppError::from)?;
    resampler.flush(&mut samples).map_err(AppError::from)?;

    // Like the live pipeline, only speech the voice detector finds goes to Whisper; a quiet
    // room otherwise comes back as an invented 「ありがとうございました。」.
    let speech = speech_only(&samples);
    let mut text = String::new();
    if !speech.is_empty() && oneshot::wait_ready(&st.worker, Duration::from_secs(120)) {
        let config = AsrConfig {
            language: settings.language,
            vocabulary: settings.vocabulary.clone(),
            accuracy_first: false,
            hallucination_filter: true,
            audio_ctx_experimental: false,
        };
        if let Ok((result, _)) = oneshot::transcribe(&st.worker, speech, config, Duration::from_secs(120)) {
            text = result.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
        }
    }
    Ok(MicTestResult { peak_dbfs: to_dbfs(overall_peak), text, device })
}

/// The utterances the segmenter cuts from a 16 kHz clip, joined with 200 ms of silence.
pub fn speech_only(samples: &[f32]) -> Vec<f32> {
    use crate::segmenter::{SegEvent, Segmenter, SegmenterConfig};
    use crate::vad::{EarshotVad, Vad};
    let mut vad = EarshotVad::new();
    let mut seg = Segmenter::new(SegmenterConfig::default());
    let mut utterances = Vec::new();
    let fl = seg.frame_len();
    for frame in samples.chunks_exact(fl) {
        let score = vad.score(frame);
        for ev in seg.push_frame(frame, score) {
            if let SegEvent::Closed(u) = ev {
                utterances.push(u.samples);
            }
        }
    }
    if let Some(SegEvent::Closed(u)) = seg.flush() {
        utterances.push(u.samples);
    }
    let mut out = Vec::new();
    for (i, u) in utterances.into_iter().enumerate() {
        if i > 0 {
            out.extend(std::iter::repeat_n(0.0, 3_200));
        }
        out.extend(u);
    }
    out
}

fn emit_level(app: &AppHandle, dbfs: f32) {
    events::emit(app, events::Event::Levels(vec![(SourceId::Mic, (dbfs * 10.0).round() / 10.0)]));
}

#[cfg(test)]
mod tests {
    #[test]
    fn silence_has_no_speech() {
        assert!(super::speech_only(&vec![0.0; 16_000 * 10]).is_empty());
    }
}
