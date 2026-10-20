//! Offline capture segmentation benchmark. No profile, devices, or network.
//! cargo run --release -p skillissue-core --example audio_benchmark -- [fixture.wav ...]
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use skillissue_core::audio::SpeechSegmenter;
    use std::{
        cell::{Cell, RefCell},
        time::Instant,
    };

    fn measure(name: &str, samples: &[f32], rate: u32) -> serde_json::Value {
        let mut detector = SpeechSegmenter::new("fixture", rate, 8);
        let consumed = Cell::new(0usize);
        let emissions = RefCell::new(Vec::new());
        let emit = |segment: skillissue_core::audio::AudioSegment| {
            emissions.borrow_mut().push(serde_json::json!({
                "availableAtAudioMs": consumed.get() as f64 * 1000.0 / f64::from(rate),
                "segmentMs": segment.samples.len() as f64 / 16.0,
            }));
        };
        let started = Instant::now();
        for packet in samples.chunks((rate / 100) as usize) {
            consumed.set(consumed.get() + packet.len());
            detector.push(packet, &emit);
        }
        detector.finish(&emit);
        let elapsed = started.elapsed();
        serde_json::json!({
            "fixture": name,
            "inputRate": rate,
            "audioMs": samples.len() as f64 * 1000.0 / f64::from(rate),
            "processingMs": elapsed.as_secs_f64() * 1000.0,
            "segments": emissions.into_inner(),
        })
    }

    let rate = 48_000;
    let length = rate as usize * 5;
    let silence = vec![0.0; length];
    let tone: Vec<_> = (0..length)
        .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.1)
        .collect();
    let mut seed = 7_u32;
    let noise: Vec<_> = (0..length)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed as f64 / f64::from(u32::MAX) * 0.1 - 0.05) as f32
        })
        .collect();
    let mut results = vec![
        measure("synthetic silence", &silence, rate),
        measure("synthetic 440 Hz tone", &tone, rate),
        measure("synthetic broadband noise", &noise, rate),
    ];
    for path in std::env::args_os().skip(1) {
        let path = std::path::Path::new(&path);
        if std::fs::metadata(path)?.len() > 64 * 1024 * 1024 {
            return Err("WAV fixtures must be at most 64 MiB".into());
        }
        let mut reader = hound::WavReader::open(path)?;
        let spec = reader.spec();
        if !(8_000..=192_000).contains(&spec.sample_rate) || !(1..=2).contains(&spec.channels) {
            return Err("Use mono/stereo WAV at 8–192 kHz".into());
        }
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
            hound::SampleFormat::Int => {
                let divisor = 2_f64.powi(i32::from(spec.bits_per_sample) - 1);
                reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|v| (f64::from(v) / divisor) as f32))
                    .collect::<Result<_, _>>()?
            }
        };
        let mono: Vec<_> = samples
            .chunks_exact(usize::from(spec.channels))
            .map(|frame| frame.iter().sum::<f32>() / f32::from(spec.channels))
            .collect();
        results.push(measure(&path.to_string_lossy(), &mono, spec.sample_rate));
        let quiet: Vec<_> = mono.iter().map(|sample| sample * 0.1).collect();
        results.push(measure(
            &format!("{} (gain -20 dB)", path.display()),
            &quiet,
            spec.sample_rate,
        ));
        let mut clicks = mono.clone();
        let click_period = spec.sample_rate as usize / 3;
        for (index, sample) in clicks.iter_mut().enumerate() {
            if index % click_period < spec.sample_rate as usize / 500 {
                *sample = (*sample + if index % 2 == 0 { 0.4 } else { -0.4 }).clamp(-1.0, 1.0);
            }
        }
        results.push(measure(
            &format!("{} (synthetic clicks)", path.display()),
            &clicks,
            spec.sample_rate,
        ));
        let offset = spec.sample_rate as usize * 3;
        let overlap: Vec<_> = mono
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                (sample * 0.5 + mono.get(index + offset).copied().unwrap_or(0.0) * 0.5)
                    .clamp(-1.0, 1.0)
            })
            .collect();
        results.push(measure(
            &format!("{} (shifted speech mix)", path.display()),
            &overlap,
            spec.sample_rate,
        ));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Run this capture benchmark on Windows.");
}
