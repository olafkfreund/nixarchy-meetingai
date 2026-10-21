//! Application bootstrap: opens the overlay window and runs the event loop.

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use crate::features::overlay::Overlay;

const DEFAULT_WIDTH: f32 = 720.0;
const DEFAULT_HEIGHT: f32 = 620.0;

/// Start the application. Blocks until the last window closes.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("lennette=info")),
        )
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting lennette");

    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(DEFAULT_WIDTH), px(DEFAULT_HEIGHT)), cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("lennette".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.open_window(options, |_, cx| cx.new(|_| Overlay::new()))
            .expect("failed to open the overlay window");

        cx.activate(true);
    });
}
