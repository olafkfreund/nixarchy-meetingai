//! System tray icon.
//!
//! The tray gives quick access without focusing the window: open the overlay,
//! open the dashboard, start listening, quit.
//!
//! On Linux this uses the KSNI backend, which runs its D-Bus service on a worker
//! thread and needs no GTK main loop — GPUI owns the main loop here. Event polling
//! is explicit so the UI can process clicks on its own frame.

/// What a tray interaction asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ShowOverlay,
    OpenDashboard,
    ToggleListening,
    Quit,
}

/// A live tray icon, or a recorded reason it could not be created.
pub enum Tray {
    /// Created successfully.
    Live(Box<dyn TrayBackend>),
    /// Unavailable; the reason is shown in logs, never claimed as working.
    Unavailable(String),
}

/// The parts of a tray a caller needs.
pub trait TrayBackend: Send {
    /// Drain pending actions without blocking.
    fn poll(&self) -> Vec<TrayAction>;
    /// Update the tooltip, e.g. to show listening state.
    fn set_tooltip(&self, text: &str);
}

impl Tray {
    /// Create the tray, reporting why it is unavailable rather than failing.
    pub fn start() -> Self {
        #[cfg(target_os = "linux")]
        {
            match ksni_tray::start() {
                Ok(backend) => Tray::Live(Box::new(backend)),
                Err(error) => Tray::Unavailable(error),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Tray::Unavailable("the tray is not implemented on this platform yet".to_owned())
        }
    }

    /// Drain pending actions.
    pub fn poll(&self) -> Vec<TrayAction> {
        match self {
            Tray::Live(backend) => backend.poll(),
            Tray::Unavailable(_) => Vec::new(),
        }
    }

    pub fn set_tooltip(&self, text: &str) {
        if let Tray::Live(backend) = self {
            backend.set_tooltip(text);
        }
    }

    /// Why the tray is unavailable, if it is.
    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Tray::Live(_) => None,
            Tray::Unavailable(reason) => Some(reason),
        }
    }
}

/// Render the mark at one size, in RGBA.
///
/// A rounded ring with a bright core: legible on both light and dark panels, and
/// drawn in code so no external asset can go missing.
fn icon_at(size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    let centre = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 * 0.42;
    let core = size as f32 * 0.20;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - centre;
            let dy = y as f32 - centre;
            let distance = (dx * dx + dy * dy).sqrt();
            // Anti-alias the outer edge over one pixel.
            let alpha = ((radius - distance) * 255.0).clamp(0.0, 255.0);
            let (r, g, b) = if distance < core {
                (255, 255, 255)
            } else if distance < radius {
                (150, 152, 158)
            } else {
                (0, 0, 0)
            };
            pixels.extend_from_slice(&[r, g, b, alpha as u8]);
        }
    }
    pixels
}

/// The default 16×16 icon, used by tests and as a fallback.
#[cfg(test)]
fn icon_rgba() -> (Vec<u8>, u32, u32) {
    const SIZE: u32 = 16;
    (icon_at(SIZE), SIZE, SIZE)
}

/// Sizes a panel may ask for, largest first.
const ICON_SIZES: [u32; 4] = [64, 48, 32, 22];

#[cfg(target_os = "linux")]
mod ksni_tray {
    use super::{ICON_SIZES, TrayAction, TrayBackend, icon_at};
    use ksni::blocking::TrayMethods;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Shared queue between the D-Bus thread and the UI.
    type Queue = Arc<Mutex<Vec<TrayAction>>>;

    /// The SNI item served over D-Bus.
    struct Item {
        queue: Queue,
        tooltip: Arc<Mutex<String>>,
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "dev.lennette.overlay".into()
        }

        fn title(&self) -> String {
            "lennette".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            // Serve every common size: panels pick the closest and downscale,
            // and a lone 16px pixmap renders blurry or is dropped entirely.
            ICON_SIZES
                .iter()
                .map(|&size| ksni::Icon {
                    width: size as i32,
                    height: size as i32,
                    data: icon_at(size),
                })
                .collect()
        }

        fn icon_name(&self) -> String {
            // A themed name gives the panel a fallback if pixmaps are ignored.
            "lennette".into()
        }

        fn status(&self) -> ksni::Status {
            ksni::Status::Active
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            let text = self
                .tooltip
                .lock()
                .map(|value| value.clone())
                .unwrap_or_else(|_| "lennette".to_owned());
            ksni::ToolTip {
                title: text,
                description: String::new(),
                ..Default::default()
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            if let Ok(mut queue) = self.queue.lock() {
                queue.push(TrayAction::ShowOverlay);
            }
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;

            let push = |action: TrayAction| {
                let queue = self.queue.clone();
                move |_item: &mut Item| {
                    if let Ok(mut queue) = queue.lock() {
                        queue.push(action);
                    }
                }
            };

            vec![
                StandardItem {
                    label: "Show overlay".into(),
                    activate: Box::new(push(TrayAction::ShowOverlay)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Open dashboard".into(),
                    activate: Box::new(push(TrayAction::OpenDashboard)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Start or stop listening".into(),
                    activate: Box::new(push(TrayAction::ToggleListening)),
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "Quit lennette".into(),
                    activate: Box::new(push(TrayAction::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    pub struct Backend {
        queue: Queue,
        handle: ksni::blocking::Handle<Item>,
    }

    impl TrayBackend for Backend {
        fn poll(&self) -> Vec<TrayAction> {
            self.queue
                .lock()
                .map(|mut queue| std::mem::take(&mut *queue))
                .unwrap_or_default()
        }

        fn set_tooltip(&self, text: &str) {
            // The blocking handle marshals this onto the service thread itself.
            self.handle.update(move |item| {
                if let Ok(mut tooltip) = item.tooltip.lock() {
                    *tooltip = text.to_owned();
                }
            });
        }
    }

    pub fn start() -> Result<Backend, String> {
        let queue: Queue = Arc::new(Mutex::new(Vec::new()));
        let tooltip = Arc::new(Mutex::new("lennette".to_owned()));
        let item = Item {
            queue: queue.clone(),
            tooltip,
        };

        // The blocking trait spawns the D-Bus service on its own thread, which is
        // what lets GPUI keep the main loop.
        let handle = item
            .spawn()
            .map_err(|error| format!("the tray service could not start: {error}"))?;
        let _ = Duration::from_millis(0);
        Ok(Backend { queue, handle })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_the_expected_shape() {
        let (rgba, width, height) = icon_rgba();
        assert_eq!(width, 16);
        assert_eq!(height, 16);
        assert_eq!(rgba.len(), 16 * 16 * 4);
        // The centre is opaque, the corners are transparent.
        let centre = (8 * 16 + 8) * 4;
        assert_eq!(rgba[centre + 3], 255);
        assert_eq!(rgba[3], 0);
    }

    #[test]
    fn starting_never_panics_and_reports_availability() {
        let tray = Tray::start();
        match &tray {
            Tray::Live(_) => assert!(tray.unavailable_reason().is_none()),
            Tray::Unavailable(reason) => {
                assert!(tray.unavailable_reason().is_some());
                assert!(!reason.is_empty());
            }
        }
        // Polling is always safe.
        let _ = tray.poll();
    }
}
