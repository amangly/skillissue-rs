//! Packet-independent resampling for continuous 100 ms PCM16 frames.
use super::StreamEvent;

pub struct LiveFrame {
    pub source: String,
    pub pcm: Vec<u8>,
    pub created_at: u64,
}

pub struct LiveResampler {
    source: String,
    rate: u32,
    weight: u32,
    sum: f64,
    pcm: Vec<u8>,
}

impl LiveResampler {
    pub fn new(source: &str, rate: u32) -> Result<Self, String> {
        if !(8_000..=192_000).contains(&rate) {
            return Err("Streaming audio requires an 8–192 kHz capture device.".into());
        }
        Ok(Self {
            source: source.into(),
            rate,
            weight: 0,
            sum: 0.0,
            pcm: Vec::with_capacity(3200),
        })
    }

    pub fn push(
        &mut self,
        samples: &[f32],
        emit: &dyn Fn(StreamEvent) -> Result<(), String>,
    ) -> Result<(), String> {
        for &sample in samples {
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            let mut remaining = 16_000;
            while remaining > 0 {
                let weight = remaining.min(self.rate - self.weight);
                self.sum += f64::from(sample) * f64::from(weight);
                self.weight += weight;
                remaining -= weight;
                if self.weight == self.rate {
                    self.append((self.sum / f64::from(self.rate)) as f32, emit)?;
                    self.sum = 0.0;
                    self.weight = 0;
                }
            }
        }
        Ok(())
    }

    fn append(
        &mut self,
        sample: f32,
        emit: &dyn Fn(StreamEvent) -> Result<(), String>,
    ) -> Result<(), String> {
        self.pcm
            .extend_from_slice(&((sample * 32767.0).round() as i16).to_le_bytes());
        if self.pcm.len() == 3200 {
            self.emit(emit)?;
        }
        Ok(())
    }

    fn emit(&mut self, emit: &dyn Fn(StreamEvent) -> Result<(), String>) -> Result<(), String> {
        if !self.pcm.is_empty() {
            let pcm = std::mem::replace(&mut self.pcm, Vec::with_capacity(3200));
            emit(StreamEvent::Frame(LiveFrame {
                source: self.source.clone(),
                created_at: crate::models::now_ms().saturating_sub(pcm.len() as u64 / 32),
                pcm,
            }))?;
        }
        Ok(())
    }

    pub fn finish(
        &mut self,
        emit: &dyn Fn(StreamEvent) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.weight > 0 {
            self.append((self.sum / f64::from(self.weight)) as f32, emit)?;
            self.weight = 0;
            self.sum = 0.0;
        }
        self.emit(emit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn continuous_frames_ignore_input_packet_boundaries_and_keep_silence() {
        for rate in [8_000, 16_000, 44_100, 48_000, 192_000] {
            let samples = vec![0.0; rate as usize + 13];
            let run = |packet| {
                let bytes = RefCell::new(Vec::new());
                let emit = |event| {
                    if let StreamEvent::Frame(frame) = event {
                        assert!(frame.pcm.len() <= 3200);
                        bytes.borrow_mut().extend(frame.pcm);
                    }
                    Ok(())
                };
                let mut stream = LiveResampler::new("microphone", rate).unwrap();
                for part in samples.chunks(packet) {
                    stream.push(part, &emit).unwrap();
                }
                stream.finish(&emit).unwrap();
                bytes.into_inner()
            };
            assert_eq!(run(1), run(137));
            assert_eq!(
                run(samples.len()).len(),
                (samples.len() * 16000).div_ceil(rate as usize) * 2
            );
        }
    }
}
