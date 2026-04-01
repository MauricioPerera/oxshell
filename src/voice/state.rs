/// Voice mode states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceMode {
    /// Voice is disabled
    Disabled,
    /// Idle — waiting for user to press record key
    Idle,
    /// Recording audio from microphone
    Recording,
    /// Processing audio (sending to STT)
    Processing,
    /// Error state (mic not available, etc.)
    Error,
}

/// Voice state manager
pub struct VoiceState {
    pub mode: VoiceMode,
    /// Whether voice feature is available on this platform
    pub available: bool,
    /// Current interim transcript (updates live during recording)
    pub interim_text: String,
    /// Final transcript from last recording
    pub final_text: Option<String>,
    /// Error message if any
    pub error: Option<String>,
    /// Recording duration in milliseconds
    pub recording_ms: u64,
}

impl VoiceState {
    pub fn new() -> Self {
        let available = check_audio_available();
        Self {
            mode: if available { VoiceMode::Idle } else { VoiceMode::Disabled },
            available,
            interim_text: String::new(),
            final_text: None,
            error: None,
            recording_ms: 0,
        }
    }

    /// Start recording
    pub fn start_recording(&mut self) -> Result<(), String> {
        if !self.available {
            return Err("Audio capture not available on this platform".into());
        }
        if self.mode != VoiceMode::Idle {
            return Err(format!("Cannot start recording in {:?} state", self.mode));
        }
        self.mode = VoiceMode::Recording;
        self.interim_text.clear();
        self.final_text = None;
        self.recording_ms = 0;
        Ok(())
    }

    /// Stop recording and start processing
    pub fn stop_recording(&mut self) {
        if self.mode == VoiceMode::Recording {
            self.mode = VoiceMode::Processing;
        }
    }

    /// Set the final transcript and return to idle
    pub fn set_transcript(&mut self, text: String) {
        self.final_text = Some(text);
        self.mode = VoiceMode::Idle;
    }

    /// Set error state
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.mode = VoiceMode::Error;
    }

    /// Reset from error to idle
    pub fn reset(&mut self) {
        self.error = None;
        self.mode = if self.available {
            VoiceMode::Idle
        } else {
            VoiceMode::Disabled
        };
    }

    /// Get status text for display
    pub fn status_text(&self) -> &str {
        match self.mode {
            VoiceMode::Disabled => "Voice: unavailable",
            VoiceMode::Idle => "Voice: ready (Ctrl+R to record)",
            VoiceMode::Recording => "Voice: recording...",
            VoiceMode::Processing => "Voice: processing...",
            VoiceMode::Error => "Voice: error",
        }
    }
}

/// Check if audio capture tools are available
fn check_audio_available() -> bool {
    if cfg!(target_os = "macos") {
        // Check for sox (rec command)
        std::process::Command::new("which")
            .arg("rec")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else if cfg!(target_os = "linux") {
        // Check for arecord (ALSA)
        std::process::Command::new("which")
            .arg("arecord")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else if cfg!(target_os = "windows") {
        // Windows has built-in audio APIs but we need ffmpeg or sox
        std::process::Command::new("where")
            .arg("ffmpeg")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        false
    }
}
