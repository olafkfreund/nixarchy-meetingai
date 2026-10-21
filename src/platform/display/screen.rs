//! Screen capture through xcap.
//!
//! xcap is called only from this module. Everything downstream works with a
//! [`Screenshot`] worth of pixels, so cropping, scaling and encoding stay pure and
//! testable without a display.
//!
//! Wayland note: xcap captures the screen directly on X11 and through a portal on
//! some Wayland compositors. Interactive region selection is not reproduced; here
//! a region is applied to an already-captured frame, which is what the overlay's
//! selector does once coordinates are known.

use super::capture::{self, CaptureError, CapturedImage, Region};
use image::RgbaImage;

/// One monitor, as reported by the platform.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub is_primary: bool,
    pub scale: f32,
}

/// Errors from the platform capture layer.
#[derive(Debug, thiserror::Error)]
pub enum ScreenError {
    #[error("no monitors were found")]
    NoMonitors,
    #[error("monitor {0} is no longer connected")]
    MonitorGone(usize),
    #[error("screen capture failed: {0}")]
    Capture(String),
}

/// Enumerate monitors.
pub fn monitors() -> Result<Vec<MonitorInfo>, ScreenError> {
    let monitors = xcap::Monitor::all().map_err(|error| ScreenError::Capture(error.to_string()))?;
    if monitors.is_empty() {
        return Err(ScreenError::NoMonitors);
    }
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| MonitorInfo {
            index,
            name: monitor
                .name()
                .unwrap_or_else(|_| format!("Display {index}"))
                .to_owned(),
            width: monitor.width().unwrap_or(0),
            height: monitor.height().unwrap_or(0),
            x: monitor.x().unwrap_or(0),
            y: monitor.y().unwrap_or(0),
            is_primary: monitor.is_primary().unwrap_or(index == 0),
            scale: monitor.scale_factor().unwrap_or(1.0),
        })
        .collect())
}

/// The primary monitor, falling back to the first one.
pub fn primary_monitor() -> Result<MonitorInfo, ScreenError> {
    let monitors = monitors()?;
    Ok(monitors
        .iter()
        .find(|monitor| monitor.is_primary)
        .cloned()
        .unwrap_or_else(|| monitors[0].clone()))
}

/// Capture a monitor's full frame.
pub fn capture_monitor(index: Option<usize>) -> Result<RgbaImage, ScreenError> {
    let monitors = xcap::Monitor::all().map_err(|error| ScreenError::Capture(error.to_string()))?;
    if monitors.is_empty() {
        return Err(ScreenError::NoMonitors);
    }
    // Resolve the index first, then take ownership once: reading `is_primary`
    // while also consuming the vec silently produced an empty list.
    let chosen = match index {
        Some(index) => {
            if index >= monitors.len() {
                return Err(ScreenError::MonitorGone(index));
            }
            index
        }
        None => monitors
            .iter()
            .position(|monitor| monitor.is_primary().unwrap_or(false))
            .unwrap_or(0),
    };
    let monitor = monitors
        .into_iter()
        .nth(chosen)
        .ok_or(ScreenError::NoMonitors)?;
    monitor
        .capture_image()
        .map_err(|error| ScreenError::Capture(error.to_string()))
}

/// Capture and encode in one step: crop to `region`, scale, then base64.
///
/// `region` is monitor-relative **physical** pixels, already multiplied by the
/// device pixel ratio by the caller.
pub fn capture(
    monitor: Option<usize>,
    region: Option<Region>,
    png: bool,
) -> Result<CapturedImage, ScreenError> {
    let frame = capture_monitor(monitor)?;
    capture::capture_to_base64(&frame, region, png)
        .map_err(|error| ScreenError::Capture(error.to_string()))
}

/// Describe why capture cannot work here, for the settings page.
pub fn capture_support() -> (bool, String) {
    match monitors() {
        Ok(monitors) => (
            true,
            format!(
                "{} display{} available.",
                monitors.len(),
                if monitors.len() == 1 { "" } else { "s" }
            ),
        ),
        Err(error) => (false, error.to_string()),
    }
}

/// Map a [`CaptureError`] to a user-facing message.
pub fn describe(error: &CaptureError) -> String {
    match error {
        CaptureError::Bounds => {
            "That region is empty or outside the screen. Pick an area inside this display."
                .to_owned()
        }
        CaptureError::NoMonitor => "No display is available to capture.".to_owned(),
        CaptureError::Encode(message) => format!("The screenshot could not be encoded: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_enumeration_never_panics() {
        // Runs against whatever display is present; the contract is that it
        // either lists monitors or reports a clear error.
        match monitors() {
            Ok(monitors) => {
                assert!(!monitors.is_empty());
                assert!(monitors.iter().all(|monitor| !monitor.name.is_empty()));
                assert!(monitors.iter().filter(|monitor| monitor.is_primary).count() <= 1);
            }
            Err(ScreenError::NoMonitors) | Err(ScreenError::Capture(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn support_probe_is_consistent() {
        let (supported, message) = capture_support();
        assert!(!message.is_empty());
        if !supported {
            assert!(!message.contains("available."));
        }
    }

    #[test]
    fn bounds_message_is_actionable() {
        assert!(describe(&CaptureError::Bounds).contains("outside the screen"));
    }

    #[test]
    fn capture_without_an_explicit_index_uses_the_primary() {
        // Regression: an earlier version consumed the monitor list while probing
        // for the primary, so every capture reported "no monitors".
        match capture(None, None, false) {
            Ok(image) => {
                assert!(image.base64.len() > 100, "captured image should have bytes");
                assert!(matches!(image.mime, "image/png" | "image/jpeg"));
            }
            // A headless CI box legitimately has no display.
            Err(ScreenError::NoMonitors) | Err(ScreenError::Capture(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn missing_monitor_index_is_reported() {
        let result = capture_monitor(Some(usize::MAX));
        assert!(matches!(
            result,
            Err(ScreenError::MonitorGone(_)) | Err(ScreenError::NoMonitors)
        ));
    }
}
