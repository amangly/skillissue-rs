// Rust | audio.rs | shared native runtime | Windows 10/11 WASAPI.
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

#[cfg(any(windows, test))]
pub use processing::Segmenter as SpeechSegmenter;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOptions {
    pub microphone: bool,
    pub system_audio: bool,
    pub chunk_seconds: u32,
}

#[derive(Clone, Debug)]
pub struct AudioSegment {
    pub source: String,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioLevel {
    pub source: String,
    pub level: f32,
}

pub struct AudioCapture {
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
    flushers: Vec<std::sync::mpsc::SyncSender<std::sync::mpsc::SyncSender<()>>>,
}

impl AudioCapture {
    pub fn start(
        options: CaptureOptions,
        on_segment: Arc<dyn Fn(AudioSegment) + Send + Sync>,
        on_level: Arc<dyn Fn(AudioLevel) + Send + Sync>,
        on_error: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Self, String> {
        if !options.microphone && !options.system_audio {
            return Err("Select microphone or system audio before starting capture.".into());
        }
        #[cfg(windows)]
        {
            windows_capture::start(options, on_segment, on_level, on_error)
        }
        #[cfg(not(windows))]
        {
            let _ = (options, on_segment, on_level, on_error);
            Err("Native microphone and system audio capture currently require Windows.".into())
        }
    }

    pub fn stop(mut self) {
        self.stop_and_join();
    }

    pub fn flush(&self) -> Result<(), String> {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};
        if self.stop.load(Ordering::Acquire) {
            return Err("Audio capture has stopped.".into());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let (ack_tx, ack_rx) = mpsc::sync_channel(self.flushers.len().max(1));
        for flusher in &self.flushers {
            flusher
                .try_send(ack_tx.clone())
                .map_err(|error| format!("Request audio flush: {error}"))?;
        }
        drop(ack_tx);
        for _ in &self.flushers {
            ack_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|error| format!("Audio flush did not finish: {error}"))?;
        }
        Ok(())
    }

    fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::Release);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Hosted capture is downmixed by WASAPI; resample to the gateway's 16 kHz PCM16 format.
pub fn encode_hosted_wav(segment: &AudioSegment) -> Result<Vec<u8>, String> {
    const RATE: u32 = 16_000;
    if !(8_000..=192_000).contains(&segment.sample_rate)
        || segment.samples.is_empty()
        || segment.samples.len() > segment.sample_rate as usize * 30
    {
        return Err(
            "Hosted capture requires a nonempty mono segment of at most 30 seconds at 8–192 kHz."
                .into(),
        );
    }
    let input_rate = u64::from(segment.sample_rate);
    let end = segment.samples.len() as u64 * u64::from(RATE);
    let output_len = end.div_ceil(input_rate);
    let mut cursor = Cursor::new(Vec::with_capacity(output_len as usize * 2 + 44));
    {
        let mut writer = hound::WavWriter::new(
            &mut cursor,
            hound::WavSpec {
                channels: 1,
                sample_rate: RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|error| format!("Create hosted WAV: {error}"))?;
        for output in 0..output_len {
            let start = output * input_rate;
            let stop = ((output + 1) * input_rate).min(end);
            let first = start / u64::from(RATE);
            let last = stop.div_ceil(u64::from(RATE));
            let mut sum = 0.0_f64;
            for input in first..last {
                let left = start.max(input * u64::from(RATE));
                let right = stop.min((input + 1) * u64::from(RATE));
                sum += f64::from(sanitize_sample(segment.samples[input as usize]))
                    * (right - left) as f64;
            }
            let sample = (sum / (stop - start) as f64).clamp(-1.0, 1.0);
            let pcm = if sample <= -1.0 {
                i16::MIN
            } else {
                (sample * f64::from(i16::MAX)).round() as i16
            };
            writer
                .write_sample(pcm)
                .map_err(|error| format!("Encode hosted WAV: {error}"))?;
        }
        writer
            .finalize()
            .map_err(|error| format!("Finish hosted WAV: {error}"))?;
    }
    Ok(cursor.into_inner())
}

pub fn encode_wav(segment: &AudioSegment) -> Result<Vec<u8>, String> {
    if !(8_000..=192_000).contains(&segment.sample_rate) {
        return Err("Audio sample rate must be between 8 kHz and 192 kHz.".into());
    }
    if segment.samples.is_empty() {
        return Err("Cannot encode an empty audio segment.".into());
    }
    if segment.samples.len() > segment.sample_rate as usize * 30 {
        return Err("Audio segment exceeds the 30 second encoding limit.".into());
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: segment.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::with_capacity(segment.samples.len() * 2 + 44));
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|error| format!("Create WAV: {error}"))?;
        for &sample in &segment.samples {
            let sample = sanitize_sample(sample);
            let pcm = if sample <= -1.0 {
                i16::MIN
            } else {
                (sample * i16::MAX as f32).round() as i16
            };
            writer
                .write_sample(pcm)
                .map_err(|error| format!("Encode WAV: {error}"))?;
        }
        writer
            .finalize()
            .map_err(|error| format!("Finalize WAV: {error}"))?;
    }
    Ok(cursor.into_inner())
}

fn sanitize_sample(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(any(windows, test))]
mod processing {
    use super::{AudioSegment, sanitize_sample};
    use std::collections::VecDeque;

    #[derive(Clone, Copy, Debug)]
    pub enum SampleEncoding {
        Float32,
        Float64,
        Pcm8,
        Pcm16,
        Pcm24,
        Pcm32,
    }

    impl SampleEncoding {
        fn bytes(self) -> usize {
            match self {
                Self::Pcm8 => 1,
                Self::Pcm16 => 2,
                Self::Pcm24 => 3,
                Self::Pcm32 | Self::Float32 => 4,
                Self::Float64 => 8,
            }
        }

        fn decode(self, bytes: &[u8]) -> f32 {
            let sample = match self {
                Self::Float32 => f32::from_le_bytes(bytes.try_into().unwrap()),
                Self::Float64 => f64::from_le_bytes(bytes.try_into().unwrap()) as f32,
                Self::Pcm8 => (bytes[0] as f32 - 128.0) / 128.0,
                Self::Pcm16 => i16::from_le_bytes(bytes.try_into().unwrap()) as f32 / 32_768.0,
                Self::Pcm24 => {
                    let signed = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]);
                    signed as f32 / 2_147_483_648.0
                }
                // WAVEFORMATEXTENSIBLE places valid PCM bits at the most significant end.
                Self::Pcm32 => {
                    i32::from_le_bytes(bytes.try_into().unwrap()) as f32 / 2_147_483_648.0
                }
            };
            sanitize_sample(sample)
        }
    }

    pub fn decode_mono(
        bytes: &[u8],
        channels: usize,
        encoding: SampleEncoding,
        output: &mut Vec<f32>,
    ) -> Result<(), String> {
        if channels == 0 || channels > 32 {
            return Err("Audio format must have between 1 and 32 channels.".into());
        }
        let sample_bytes = encoding.bytes();
        let frame_bytes = sample_bytes * channels;
        if !bytes.len().is_multiple_of(frame_bytes) {
            return Err("Audio packet contains an incomplete frame.".into());
        }
        output.clear();
        output.reserve(bytes.len() / frame_bytes);
        for frame in bytes.chunks_exact(frame_bytes) {
            let sum: f32 = frame
                .chunks_exact(sample_bytes)
                .map(|s| encoding.decode(s))
                .sum();
            output.push(sanitize_sample(sum / channels as f32));
        }
        Ok(())
    }

    /// Streaming speech detector and bounded mono 16 kHz segment builder.
    /// One instance belongs to one device stream. `finish` flushes pending audio
    /// without resetting the detector, so asking a question can continue capture.
    pub struct Segmenter {
        source: String,
        sample_rate: u32,
        detector: earshot::Detector,
        resample_weight: u32,
        resample_sum: f64,
        frame: Vec<f32>,
        max_samples: usize,
        pre_roll: VecDeque<f32>,
        samples: Vec<f32>,
        silence_samples: usize,
        voiced_samples: usize,
        continuation: bool,
    }

    impl Segmenter {
        pub fn new(source: &str, sample_rate: u32, chunk_seconds: u32) -> Self {
            assert!(
                (8_000..=192_000).contains(&sample_rate),
                "unsupported capture sample rate"
            );
            let max_samples = 16_000 * chunk_seconds.clamp(3, 20) as usize;
            Self {
                source: source.to_owned(),
                sample_rate,
                detector: earshot::Detector::default(),
                resample_weight: 0,
                resample_sum: 0.0,
                frame: Vec::with_capacity(256),
                max_samples,
                pre_roll: VecDeque::with_capacity(4_800),
                samples: Vec::with_capacity(max_samples),
                silence_samples: 0,
                voiced_samples: 0,
                continuation: false,
            }
        }

        pub fn push(&mut self, input: &[f32], emit: &dyn Fn(AudioSegment)) {
            // Area resampling carries fractional samples across WASAPI packets.
            // Both inference and emitted audio use mono 16 kHz, independent of
            // packet sizes and the device's native rate (including 44.1 kHz).
            for &sample in input {
                let mut remaining = 16_000;
                while remaining > 0 {
                    let weight = remaining.min(self.sample_rate - self.resample_weight);
                    self.resample_sum += f64::from(sanitize_sample(sample)) * f64::from(weight);
                    self.resample_weight += weight;
                    remaining -= weight;
                    if self.resample_weight == self.sample_rate {
                        self.frame
                            .push((self.resample_sum / f64::from(self.sample_rate)) as f32);
                        self.resample_weight = 0;
                        self.resample_sum = 0.0;
                        if self.frame.len() == 256 {
                            self.classify_frame(emit);
                        }
                    }
                }
            }
        }

        fn classify_frame(&mut self, emit: &dyn Fn(AudioSegment)) {
            let mut frame = [0.0; 256];
            let count = self.frame.len();
            frame[..count].copy_from_slice(&self.frame);
            self.frame.clear();
            let voiced = self.detector.predict_f32(&frame) >= 0.5;
            self.accept_frame(&frame[..count], voiced, emit);
        }

        fn accept_frame(&mut self, block: &[f32], voiced: bool, emit: &dyn Fn(AudioSegment)) {
            if self.samples.is_empty() && !voiced && !self.continuation {
                for &sample in block {
                    self.pre_roll.push_back(sample);
                    if self.pre_roll.len() > 4_800 {
                        self.pre_roll.pop_front();
                    }
                }
                return;
            }
            if self.samples.is_empty() {
                self.samples.extend(self.pre_roll.drain(..));
            }
            for part in block.chunks(self.max_samples - self.samples.len()) {
                self.samples.extend_from_slice(part);
                if voiced {
                    self.voiced_samples += part.len();
                    self.silence_samples = 0;
                } else {
                    self.silence_samples += part.len();
                }
                if self.samples.len() >= self.max_samples {
                    let continuation = self.continuation || self.voiced_samples >= 1_280;
                    self.emit_segment(emit);
                    self.continuation = continuation;
                }
            }
            if self.silence_samples >= 5_120 {
                self.emit_segment(emit);
            }
        }

        pub fn finish(&mut self, emit: &dyn Fn(AudioSegment)) {
            // Flush a partial inference frame without adding padding to output.
            if self.resample_weight > 0 {
                self.frame
                    .push((self.resample_sum / f64::from(self.resample_weight)) as f32);
                self.resample_weight = 0;
                self.resample_sum = 0.0;
            }
            if !self.frame.is_empty() {
                self.classify_frame(emit);
            }
            self.emit_segment(emit);
        }

        fn emit_segment(&mut self, emit: &dyn Fn(AudioSegment)) {
            if self.samples.is_empty() {
                self.pre_roll.clear();
                self.continuation = false;
                return;
            }
            let samples =
                std::mem::replace(&mut self.samples, Vec::with_capacity(self.max_samples));
            if self.voiced_samples >= 1_280 || (self.continuation && self.voiced_samples > 0) {
                emit(AudioSegment {
                    source: self.source.clone(),
                    sample_rate: 16_000,
                    samples,
                });
            }
            self.silence_samples = 0;
            self.voiced_samples = 0;
            self.continuation = false;
            self.pre_roll.clear();
        }
    }

    #[cfg(test)]
    mod segmentation_tests {
        use super::*;
        use std::cell::RefCell;

        #[test]
        fn speech_boundaries_keep_pre_roll_and_bound_continuous_speech() {
            let output = RefCell::new(Vec::new());
            let emit = |segment| output.borrow_mut().push(segment);
            let mut segmenter = Segmenter::new("microphone", 48_000, 3);
            // Script classifier decisions to test boundaries independently of
            // model accuracy; constant amplitude is not a speech fixture.
            for _ in 0..100 {
                segmenter.accept_frame(&[0.0; 256], false, &emit);
            }
            for _ in 0..440 {
                segmenter.accept_frame(&[0.1; 256], true, &emit);
            }
            segmenter.finish(&emit);
            let output = output.borrow();
            assert_eq!(output.len(), 3);
            assert!(
                output
                    .iter()
                    .all(|s| s.sample_rate == 16_000 && s.samples.len() <= 48_000)
            );
            assert_eq!(
                output.iter().map(|s| s.samples.len()).sum::<usize>(),
                4_800 + 440 * 256
            );
        }

        #[test]
        fn silence_endpoint_and_short_noise_do_not_create_empty_jobs() {
            let output = RefCell::new(Vec::new());
            let emit = |segment| output.borrow_mut().push(segment);
            let mut segmenter = Segmenter::new("system", 16_000, 8);
            segmenter.accept_frame(&[0.1; 256], true, &emit);
            for _ in 0..20 {
                segmenter.accept_frame(&[0.0; 256], false, &emit);
            }
            assert!(output.borrow().is_empty());
            for _ in 0..10 {
                segmenter.accept_frame(&[0.1; 256], true, &emit);
            }
            for _ in 0..19 {
                segmenter.accept_frame(&[0.0; 256], false, &emit);
            }
            assert!(output.borrow().is_empty());
            segmenter.accept_frame(&[0.0; 256], false, &emit);
            assert_eq!(output.borrow().len(), 1);
            segmenter.finish(&emit);
            assert_eq!(output.borrow().len(), 1);
        }

        #[test]
        fn packet_boundaries_do_not_change_resampling_or_detection() {
            for rate in [8_000, 44_100, 48_000, 192_000] {
                let input: Vec<_> = (0..rate / 2 + 7)
                    .map(|i| ((i as f32 * 0.031).sin() * 0.2).clamp(-1.0, 1.0))
                    .collect();
                let run = |packet_size| {
                    let output = RefCell::new(Vec::new());
                    let mut segmenter = Segmenter::new("microphone", rate, 3);
                    for packet in input.chunks(packet_size) {
                        segmenter.push(packet, &|segment: AudioSegment| {
                            output.borrow_mut().push(segment.samples)
                        });
                    }
                    let frame = segmenter.frame.clone();
                    let pre_roll = segmenter.pre_roll.clone();
                    let partial = segmenter.samples.clone();
                    segmenter
                        .finish(&|segment: AudioSegment| output.borrow_mut().push(segment.samples));
                    (output.into_inner(), frame, pre_roll, partial)
                };
                assert_eq!(run(input.len()), run(137));
                assert_eq!(run(input.len()), run(1));
            }
        }

        #[test]
        fn hard_chunk_limit_keeps_the_short_tail_of_an_existing_utterance() {
            let output = RefCell::new(Vec::new());
            let emit = |segment| output.borrow_mut().push(segment);
            let mut segmenter = Segmenter::new("microphone", 16_000, 3);
            for _ in 0..188 {
                segmenter.accept_frame(&[0.1; 256], true, &emit);
            }
            segmenter.finish(&emit);
            assert_eq!(output.borrow().len(), 2);
            assert_eq!(output.borrow()[1].samples.len(), 128);
        }

        #[test]
        fn silence_after_a_hard_boundary_ends_continuation_without_an_audio_job() {
            let output = RefCell::new(Vec::new());
            let emit = |segment| output.borrow_mut().push(segment);
            let mut segmenter = Segmenter::new("microphone", 16_000, 3);
            for _ in 0..187 {
                segmenter.accept_frame(&[0.1; 256], true, &emit);
            }
            segmenter.accept_frame(&[0.1; 128], true, &emit);
            assert_eq!(output.borrow().len(), 1);
            for _ in 0..20 {
                segmenter.accept_frame(&[0.0; 256], false, &emit);
            }
            segmenter.accept_frame(&[0.1; 256], true, &emit);
            segmenter.finish(&emit);
            assert_eq!(output.borrow().len(), 1);
        }
    }
}

#[cfg(windows)]
mod windows_capture {
    use super::processing::{SampleEncoding, Segmenter, decode_mono};
    use super::*;
    use std::sync::mpsc::{self, Receiver, SyncSender};
    use std::thread;
    use std::time::{Duration, Instant};
    use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode};
    use windows::Win32::Media::Audio::{AUDCLNT_BUFFERFLAGS_SILENT, IAudioCaptureClient};

    struct Apartment;

    impl Drop for Apartment {
        fn drop(&mut self) {
            wasapi::deinitialize();
        }
    }

    pub fn start(
        options: CaptureOptions,
        on_segment: Arc<dyn Fn(AudioSegment) + Send + Sync>,
        on_level: Arc<dyn Fn(AudioLevel) + Send + Sync>,
        on_error: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<AudioCapture, String> {
        let mut capture = AudioCapture {
            stop: Arc::new(AtomicBool::new(false)),
            workers: Vec::new(),
            flushers: Vec::new(),
        };
        let (ready_tx, ready_rx) = mpsc::sync_channel(2);
        for (enabled, source, direction) in [
            (options.microphone, "microphone", Direction::Capture),
            (options.system_audio, "system", Direction::Render),
        ] {
            if !enabled {
                continue;
            }
            let stop = capture.stop.clone();
            let segment = on_segment.clone();
            let level = on_level.clone();
            let error = on_error.clone();
            let ready = ready_tx.clone();
            let (flush_tx, flush_rx) = mpsc::sync_channel(1);
            let chunk_seconds = options.chunk_seconds;
            let worker = thread::Builder::new()
                .name(format!("skillissue-{source}"))
                .spawn(move || {
                    let result = run(
                        source,
                        direction,
                        chunk_seconds,
                        stop,
                        segment,
                        level,
                        &ready,
                        flush_rx,
                    );
                    if let Err(message) = result {
                        let message = format!("{source} capture: {message}");
                        let _ = ready.try_send(Err(message.clone()));
                        error(message);
                    }
                })
                .map_err(|error| format!("Start {source} capture thread: {error}"))?;
            capture.workers.push(worker);
            capture.flushers.push(flush_tx);
        }
        drop(ready_tx);
        for _ in 0..capture.workers.len() {
            match ready_rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(())) => {}
                Ok(Err(error)) => return Err(error),
                Err(error) => return Err(format!("Audio capture did not become ready: {error}")),
            }
        }
        Ok(capture)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        source: &str,
        direction: Direction,
        chunk_seconds: u32,
        stop: Arc<AtomicBool>,
        on_segment: Arc<dyn Fn(AudioSegment) + Send + Sync>,
        on_level: Arc<dyn Fn(AudioLevel) + Send + Sync>,
        ready: &SyncSender<Result<(), String>>,
        flush_rx: Receiver<SyncSender<()>>,
    ) -> Result<(), String> {
        wasapi::initialize_mta()
            .ok()
            .map_err(|error| format!("Initialize COM MTA: {error}"))?;
        let _apartment = Apartment;
        let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
        let device = enumerator
            .get_default_device(&direction)
            .map_err(|e| e.to_string())?;
        let mut client = device.get_iaudioclient().map_err(|e| e.to_string())?;
        let format = client.get_mixformat().map_err(|e| e.to_string())?;
        let sample_rate = format.get_samplespersec();
        let channels = format.get_nchannels() as usize;
        let encoding = match (
            format.get_subformat().map_err(|e| e.to_string())?,
            format.get_bitspersample(),
        ) {
            (SampleType::Float, 32) => SampleEncoding::Float32,
            (SampleType::Float, 64) => SampleEncoding::Float64,
            (SampleType::Int, 8) => SampleEncoding::Pcm8,
            (SampleType::Int, 16) => SampleEncoding::Pcm16,
            (SampleType::Int, 24) => SampleEncoding::Pcm24,
            (SampleType::Int, 32) => SampleEncoding::Pcm32,
            (sample_type, bits) => {
                return Err(format!(
                    "Unsupported {sample_type} {bits}-bit device format."
                ));
            }
        };
        if !(8_000..=192_000).contains(&sample_rate) || !(1..=32).contains(&channels) {
            return Err(format!(
                "Unsupported device format: {sample_rate} Hz, {channels} channels."
            ));
        }
        let block_align = format.get_blockalign() as usize;
        if block_align != channels * format.get_bitspersample() as usize / 8 {
            return Err("Device reports an inconsistent audio frame size.".into());
        }
        let mode = StreamMode::PollingShared {
            autoconvert: false,
            buffer_duration_hns: 1_000_000,
        };
        // A render endpoint plus Capture requests WASAPI system loopback.
        client
            .initialize_client(&format, &Direction::Capture, &mode)
            .map_err(|e| e.to_string())?;
        let buffer_frames = client.get_buffer_size().map_err(|e| e.to_string())? as usize;
        if buffer_frames == 0 || buffer_frames > sample_rate as usize * 2 {
            return Err("Device returned an invalid or excessive audio buffer size.".into());
        }
        let reader: IAudioCaptureClient = unsafe { client.as_iaudioclient().GetService() }
            .map_err(|e| format!("Activate WASAPI capture service: {e}"))?;
        let mut mono = Vec::with_capacity(buffer_frames);
        let mut segmenter = Segmenter::new(source, sample_rate, chunk_seconds);
        client.start_stream().map_err(|e| e.to_string())?;
        let _ = ready.send(Ok(()));
        let mut last_packet = Instant::now();
        let mut last_meter = Instant::now();
        let result = (|| -> Result<(), String> {
            while !stop.load(Ordering::Acquire) {
                for _ in 0..256 {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let frames = unsafe { reader.GetNextPacketSize() }
                        .map_err(|e| format!("Read WASAPI packet size: {e}"))?
                        as usize;
                    if frames == 0 {
                        break;
                    }
                    if frames > buffer_frames {
                        return Err("WASAPI packet exceeds the negotiated device buffer.".into());
                    }
                    read_packet(
                        &reader,
                        block_align,
                        channels,
                        encoding,
                        buffer_frames,
                        &mut mono,
                    )?;
                    if mono.is_empty() {
                        break;
                    }
                    last_packet = Instant::now();
                    if last_meter.elapsed() >= Duration::from_millis(100) {
                        let energy = mono.iter().map(|sample| sample * sample).sum::<f32>()
                            / mono.len() as f32;
                        on_level(AudioLevel {
                            source: source.into(),
                            level: energy.sqrt().clamp(0.0, 1.0),
                        });
                        last_meter = Instant::now();
                    }
                    segmenter.push(&mono, on_segment.as_ref());
                }
                while let Ok(ack) = flush_rx.try_recv() {
                    segmenter.finish(on_segment.as_ref());
                    let _ = ack.send(());
                }
                // Loopback endpoints may deliver no packets at all while the output is silent.
                if last_packet.elapsed() >= Duration::from_millis(650) {
                    segmenter.finish(on_segment.as_ref());
                    if last_meter.elapsed() >= Duration::from_millis(100) {
                        on_level(AudioLevel {
                            source: source.into(),
                            level: 0.0,
                        });
                        last_meter = Instant::now();
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        })();
        let stop_result = client
            .stop_stream()
            .map_err(|e| format!("Stop WASAPI stream: {e}"));
        segmenter.finish(on_segment.as_ref());
        on_level(AudioLevel {
            source: source.into(),
            level: 0.0,
        });
        result.and(stop_result)
    }

    fn read_packet(
        reader: &IAudioCaptureClient,
        block_align: usize,
        channels: usize,
        encoding: SampleEncoding,
        max_frames: usize,
        mono: &mut Vec<f32>,
    ) -> Result<(), String> {
        let mut data = std::ptr::null_mut();
        let mut frames = 0;
        let mut flags = 0;
        unsafe { reader.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
            .map_err(|error| format!("Read WASAPI buffer: {error}"))?;
        if frames == 0 {
            // AUDCLNT_S_BUFFER_EMPTY does not acquire a packet to release.
            mono.clear();
            return Ok(());
        }
        let result = if frames as usize > max_frames {
            Err("WASAPI returned too many frames.".into())
        } else if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
            // Silent WASAPI packets are allowed to have a null data pointer.
            mono.clear();
            mono.resize(frames as usize, 0.0);
            Ok(())
        } else if data.is_null() {
            Err("WASAPI returned a null non-silent audio buffer.".into())
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(data, frames as usize * block_align) };
            decode_mono(bytes, channels, encoding, mono)
        };
        let released = unsafe { reader.ReleaseBuffer(frames) }
            .map_err(|error| format!("Release WASAPI buffer: {error}"));
        result.and(released)
    }
}

#[cfg(test)]
mod tests {
    use super::processing::{SampleEncoding, Segmenter, decode_mono};
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn pcm24_and_float_downmix_preserve_polarity() {
        let mut mono = Vec::new();
        decode_mono(
            &[0, 0, 128, 255, 255, 127],
            1,
            SampleEncoding::Pcm24,
            &mut mono,
        )
        .unwrap();
        assert_eq!(mono[0], -1.0);
        assert!(mono[1] > 0.9999);
        let bytes: Vec<_> = [0.8_f32, -0.2_f32, f32::NAN, 0.4_f32]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        decode_mono(&bytes, 2, SampleEncoding::Float32, &mut mono).unwrap();
        assert!((mono[0] - 0.3).abs() < 0.0001);
        assert!((mono[1] - 0.2).abs() < 0.0001);
    }

    #[test]
    fn neural_detection_discards_silence_at_supported_device_rates() {
        let emitted = Mutex::new(Vec::new());
        let emit = |segment| emitted.lock().unwrap().push(segment);
        for rate in [8_000, 16_000, 44_100, 48_000, 96_000, 192_000] {
            let mut segmenter = Segmenter::new("system", rate, 3);
            for packet in vec![0.0; rate as usize].chunks(137) {
                segmenter.push(packet, &emit);
            }
            segmenter.finish(&emit);
        }
        assert!(emitted.lock().unwrap().is_empty());
    }

    #[test]
    fn wav_roundtrip_contains_real_pcm_and_correct_sample_rate() {
        let segment = AudioSegment {
            source: "microphone".into(),
            sample_rate: 48_000,
            samples: vec![-1.0, -0.5, 0.0, 0.5, 1.0, f32::INFINITY],
        };
        let bytes = encode_wav(&segment).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 48_000);
        let samples: Vec<i16> = reader.samples().collect::<Result<_, _>>().unwrap();
        assert_eq!(samples, [i16::MIN, -16_384, 0, 16_384, i16::MAX, 0]);
    }
}

#[cfg(test)]
mod hosted_resample_tests {
    use super::*;
    #[test]
    fn high_rate_capture_becomes_mono_16k_pcm_with_the_same_duration_and_tone() {
        let rate = 96_000;
        let segment = AudioSegment {
            source: "microphone".into(),
            sample_rate: rate,
            samples: (0..rate)
                .map(|index| (std::f32::consts::TAU * 1000.0 * index as f32 / rate as f32).sin())
                .collect(),
        };
        let wav = encode_hosted_wav(&segment).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(&wav)).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.duration(), 16_000);
        assert_eq!(wav.len(), 32_044);
        let samples = reader
            .samples::<i16>()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] <= 0 && pair[1] > 0)
            .count();
        assert!((999..=1001).contains(&crossings));
        let peak = samples
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 30_000);
        assert!(
            encode_hosted_wav(&AudioSegment {
                source: "system".into(),
                sample_rate: 192_000,
                samples: vec![0.2; 192_000 * 5]
            })
            .unwrap()
            .len()
                <= 2 * 1024 * 1024
        );
    }
}
