//! Usage: cargo run -p skillissue-core --example live_audio_benchmark -- <wav> [reference.txt] --saved-key
//! Uploads the first 15 seconds once using the default profile's saved Deepgram key.
use skillissue_core::{audio, config, models::Settings, provider};

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.last().map(String::as_str) != Some("--saved-key") || !(2..=3).contains(&args.len()) {
        return Err("Usage: live_audio_benchmark <wav> [reference.txt] --saved-key. This sends up to 15 seconds of fixture audio and may consume credits.".into());
    }
    let settings = Settings::new_profile();
    let key = config::load_saved_key(&config::transcription_credential_id(&settings))?
        .ok_or("Save a Deepgram Audio key in the default profile first.")?;
    let mut settings = settings;
    settings.api_base_url = settings.transcription_base_url.clone();
    let mut reader =
        hound::WavReader::open(&args[0]).map_err(|_| "Could not open the fixture WAV.")?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || !(8_000..=192_000).contains(&spec.sample_rate)
    {
        return Err("Fixture must be mono PCM16 WAV at 8–192 kHz.".into());
    }
    let samples = reader
        .samples::<i16>()
        .take(spec.sample_rate as usize * 15)
        .map(|sample| sample.map(|sample| f32::from(sample) / 32768.0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not decode fixture audio.")?;
    let wav = audio::encode_hosted_wav(&audio::AudioSegment {
        source: "fixture".into(),
        sample_rate: spec.sample_rate,
        samples,
    })?;
    let reference = if args.len() == 3 {
        use std::io::Read;
        let mut reference = String::new();
        std::fs::File::open(&args[1])
            .map_err(|_| "Could not open reference transcript.")?
            .take(64 * 1024 + 1)
            .read_to_string(&mut reference)
            .map_err(|_| "Could not read reference transcript.")?;
        if reference.len() > 64 * 1024 {
            return Err("Reference transcript exceeds 64 KiB.".into());
        }
        Some(reference)
    } else {
        None
    };
    let report = provider::benchmark_live_audio(&settings, &key, wav, reference.as_deref()).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|_| "Could not encode evaluation.")?
    );
    Ok(())
}
