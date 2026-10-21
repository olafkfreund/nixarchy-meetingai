//! Application bootstrap: shared services, the overlay window and the dashboard.

use std::sync::Arc;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use crate::features::dashboard::view::DashboardView;
use crate::features::db::Database;
use crate::features::overlay::Overlay;
use crate::features::vault::Vault;

const OVERLAY_WIDTH: f32 = 720.0;
const OVERLAY_HEIGHT: f32 = 620.0;
const DASHBOARD_WIDTH: f32 = 1100.0;
const DASHBOARD_HEIGHT: f32 = 760.0;

/// Services shared by every window.
///
/// SQLite connections are not `Sync`, and each window wants its own anyway, so
/// this carries the database *path* rather than a live handle.
#[derive(Clone)]
pub struct Services {
    pub vault: Arc<Option<Vault>>,
    pub database_path: Option<std::path::PathBuf>,
    pub database_ready: bool,
}

impl Services {
    /// Open the vault and database, recording problems instead of failing to start.
    pub fn open() -> Self {
        let vault = match Vault::open_default() {
            Ok(vault) => Some(vault),
            Err(error) => {
                tracing::error!(%error, "credential vault unavailable");
                None
            }
        };

        let path = default_database_path();
        let database_ready = match path.as_ref() {
            Some(path) => match Database::open(path) {
                Ok(_) => true,
                Err(error) => {
                    tracing::error!(%error, path = %path.display(), "database unavailable");
                    false
                }
            },
            None => {
                tracing::error!("no user data directory for the database");
                false
            }
        };

        Self {
            vault: Arc::new(vault),
            database_path: path,
            database_ready,
        }
    }

    /// Whether both services opened successfully.
    pub fn is_ready(&self) -> bool {
        self.vault.is_some() && self.database_ready
    }

    /// Open a fresh connection for one window.
    pub fn connect(&self) -> Option<Database> {
        let path = self.database_path.as_ref()?;
        match Database::open(path) {
            Ok(database) => Some(database),
            Err(error) => {
                tracing::error!(%error, "could not open a database connection");
                None
            }
        }
    }
}

fn default_database_path() -> Option<std::path::PathBuf> {
    let directory = directories::ProjectDirs::from("dev", "lennette", "lennette")?
        .data_dir()
        .to_path_buf();
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory.join("lennette.db"))
}

/// Start the application. Blocks until the last window closes.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("lennette=info")),
        )
        .init();

    let services = Services::open();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        database = services.database_ready,
        vault = services.vault.is_some(),
        "starting lennette"
    );

    Application::new().run(move |cx: &mut App| {
        // Register the bundled Lucide icon font once for the whole app.
        if let Err(error) = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(
            crate::features::icons::FONT_BYTES,
        )]) {
            tracing::error!(%error, "could not register the icon font");
        }

        // The overlay opens first and can open the dashboard.
        let services_for_overlay = services.clone();
        let bounds = Bounds::centered(None, size(px(OVERLAY_WIDTH), px(OVERLAY_HEIGHT)), cx);
        let overlay_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // A transparent window lets the opacity control show the desktop
            // through the overlay instead of only fading toward black.
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            titlebar: Some(TitlebarOptions {
                title: Some("lennette".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        // The tray appears without needing a window, so start it together with
        // the shortcuts.
        let tray_state = Some(crate::features::tray::Tray::start());
        match tray_state.as_ref() {
            Some(crate::features::tray::Tray::Live(_)) => {
                tracing::info!("tray icon available")
            }
            Some(crate::features::tray::Tray::Unavailable(reason)) => {
                tracing::warn!(%reason, "tray icon unavailable")
            }
            None => {}
        }

        // Global shortcuts are grabbed once for the whole process.
        let mut shortcut_listener = None;
        let mut portal_shortcuts: Option<
            std::sync::Arc<crate::features::wayland::PortalShortcuts>,
        > = None;
        let pure_wayland =
            std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none();
        if pure_wayland {
            // Wayland: bind through the desktop portal instead of grabbing keys.
            let runtime = tokio::runtime::Handle::try_current();
            let (tx, rx) = std::sync::mpsc::channel();
            let actions = crate::features::shortcuts::Action::ALL.to_vec();
            match runtime {
                Ok(handle) => {
                    handle.spawn(async move {
                        let result =
                            crate::features::wayland::PortalShortcuts::bind(&actions).await;
                        let _ = tx.send(result);
                    });
                }
                Err(_) => {
                    std::thread::spawn(move || {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build();
                        let result = match runtime {
                            Ok(runtime) => runtime.block_on(
                                crate::features::wayland::PortalShortcuts::bind(&actions),
                            ),
                            Err(error) => Err(crate::features::wayland::PortalError::Bus(
                                error.to_string(),
                            )),
                        };
                        let _ = tx.send(result);
                    });
                }
            }
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(Ok(portal)) => {
                    tracing::info!("global shortcuts bound through the desktop portal");
                    portal_shortcuts = Some(std::sync::Arc::new(portal));
                }
                Ok(Err(error)) => tracing::warn!(%error, "portal shortcuts unavailable"),
                Err(_) => tracing::warn!("the shortcut portal did not answer in time"),
            }
        }

        let (supported, message) = crate::features::shortcuts::support();
        if !pure_wayland && supported {
            match crate::features::shortcuts::ShortcutListener::start(
                &crate::features::shortcuts::Binding::defaults(),
            ) {
                Ok((listener, failed)) => {
                    if failed.is_empty() {
                        tracing::info!(count = 6, "global shortcuts registered");
                    } else {
                        tracing::warn!(?failed, "some shortcuts could not be registered");
                    }
                    // Handed to the overlay, which polls it on the UI thread.
                    shortcut_listener = Some(std::rc::Rc::new(listener));
                }
                Err(error) => tracing::error!(%error, "global shortcuts unavailable"),
            }
        } else {
            tracing::warn!(%message, "global shortcuts disabled");
        }

        let portal_clone = portal_shortcuts.clone();
        cx.open_window(overlay_options, move |_, cx| {
            cx.new(|_| {
                Overlay::new(
                    services_for_overlay,
                    shortcut_listener.clone(),
                    tray_state,
                    portal_clone,
                )
            })
        })
        .expect("failed to open the overlay window");

        cx.activate(true);
    });
}

/// Open (or focus) the dashboard window.
pub fn open_dashboard(cx: &mut App, services: Services) {
    open_dashboard_at(cx, services, crate::features::dashboard::nav::Route::Home);
}

/// Open the dashboard on a specific page.
pub fn open_dashboard_at(
    cx: &mut App,
    services: Services,
    route: crate::features::dashboard::nav::Route,
) {
    let bounds = Bounds::centered(None, size(px(DASHBOARD_WIDTH), px(DASHBOARD_HEIGHT)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("lennette".into()),
            appears_transparent: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    if let Err(error) = cx.open_window(options, move |_, cx| {
        // Each window gets its own connection so a slow page never blocks the
        // overlay's writes.
        cx.new(|_| {
            let mut view = DashboardView::new(services.connect());
            view.navigate_to(route);
            view
        })
    }) {
        tracing::error!(%error, "could not open the dashboard");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_path_is_under_the_data_dir() {
        let path = default_database_path().expect("path");
        assert!(path.ends_with("lennette.db"));
        assert!(
            path.parent().is_some_and(|parent| parent.exists()),
            "data directory must exist"
        );
    }
}
