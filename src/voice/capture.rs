use anyhow::{Result, bail};
use std::path::PathBuf;
use tokio::process::Command;

const MAX_RECORDING_SECS: u64 = 60;
const SAMPLE_RATE: u32 = 16000;

/// Record audio from microphone to a temporary WAV file.
/// Uses platform-specific tools (arecord on Linux, rec/sox on macOS, ffmpeg on Windows).
pub async fn record_audio(duration_secs: u64) -> Result<PathBuf> {
    let duration = duration_secs.min(MAX_RECORDING_SECS);
    let output = std::env::temp_dir().join(format!(
        "oxshell_voice_{}.wav",
        uuid::Uuid::new_v4()
    ));
    let output_str = output.to_string_lossy().to_string();

    let result = if cfg!(target_os = "linux") {
        Command::new("arecord")
            .args([
                "-f", "S16_LE",
                "-r", &SAMPLE_RATE.to_string(),
                "-c", "1",
                "-d", &duration.to_string(),
                &output_str,
            ])
            .output()
            .await
    } else if cfg!(target_os = "macos") {
        Command::new("rec")
            .args([
                "-r", &SAMPLE_RATE.to_string(),
                "-c", "1",
                "-b", "16",
                &output_str,
                "trim", "0", &duration.to_string(),
            ])
            .output()
            .await
    } else if cfg!(target_os = "windows") {
        Command::new("ffmpeg")
            .args([
                "-f", "dshow",
                "-i", "audio=default",
                "-t", &duration.to_string(),
                "-ar", &SAMPLE_RATE.to_string(),
                "-ac", "1",
                "-y",
                &output_str,
            ])
            .output()
            .await
    } else {
        bail!("Audio capture not supported on this platform");
    };

    match result {
        Ok(out) if out.status.success() => {
            if output.exists() {
                Ok(output)
            } else {
                bail!("Recording completed but output file not found");
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!("Recording failed: {stderr}");
        }
        Err(e) => bail!("Failed to start audio capture: {e}"),
    }
}

/// Transcribe audio using Cloudflare Workers AI (Whisper model)
pub async fn transcribe(
    audio_path: &std::path::Path,
    cf_token: &str,
    account_id: &str,
) -> Result<String> {
    let audio_data = std::fs::read(audio_path)?;
    let audio_base64 = base64_encode(&audio_data);

    let client = reqwest::Client::new();
    let url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/@cf/openai/whisper",
        account_id
    );

    let response = client
        .post(&url)
        .bearer_auth(cf_token)
        .json(&serde_json::json!({
            "audio": audio_base64,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("Whisper API error: {body}");
    }

    let body: serde_json::Value = response.json().await?;
    let text = body
        .get("result")
        .and_then(|r| r.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    // Clean up temp file
    let _ = std::fs::remove_file(audio_path);

    if text.is_empty() {
        bail!("No speech detected in audio");
    }

    Ok(text)
}

/// Correct base64 encoding — processes all data in one pass
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = Vec::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;

    while i + 2 < data.len() {
        let (b0, b1, b2) = (data[i] as usize, data[i + 1] as usize, data[i + 2] as usize);
        result.push(CHARS[b0 >> 2]);
        result.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)]);
        result.push(CHARS[((b1 & 0xf) << 2) | (b2 >> 6)]);
        result.push(CHARS[b2 & 0x3f]);
        i += 3;
    }

    match data.len() - i {
        1 => {
            let b0 = data[i] as usize;
            result.push(CHARS[b0 >> 2]);
            result.push(CHARS[(b0 & 3) << 4]);
            result.extend_from_slice(b"==");
        }
        2 => {
            let (b0, b1) = (data[i] as usize, data[i + 1] as usize);
            result.push(CHARS[b0 >> 2]);
            result.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)]);
            result.push(CHARS[(b1 & 0xf) << 2]);
            result.push(b'=');
        }
        _ => {}
    }

    String::from_utf8(result).unwrap_or_default()
}
