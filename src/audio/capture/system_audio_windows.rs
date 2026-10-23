//! Native WASAPI output loopback. Reuses the bounded/resampled CPAL worker.
use super::microphone::Microphone;

#[derive(Debug, thiserror::Error)]
#[error("Windows system-audio capture failed: {0}")]
pub struct SystemAudioError(String);

pub struct SystemAudio {
    capture: Microphone,
}
impl SystemAudio {
    pub fn start(source: Option<&str>) -> Result<Self, SystemAudioError> {
        Microphone::start_loopback(source)
            .map(|capture| Self { capture })
            .map_err(|e| SystemAudioError(e.to_string()))
    }
    pub fn source(&self) -> &str {
        self.capture.device_name()
    }
    pub fn take(&mut self, limit: usize) -> Vec<f32> {
        self.capture.take(limit)
    }
    pub fn is_running(&self) -> bool {
        self.capture.is_running()
    }
    pub fn failure(&self) -> Option<String> {
        self.capture.failure()
    }
    pub fn dropped(&self) -> u64 {
        self.capture.dropped()
    }
    pub fn stop(self) {
        self.capture.stop();
    }
}
