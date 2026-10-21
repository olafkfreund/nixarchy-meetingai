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
pub struct EmbeddedAssets;

impl gpui::AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match path {
            "lennette.png" => Ok(Some(std::borrow::Cow::Borrowed(include_bytes!(
                "assets/lennette.png"
            )))),
            "play-filled.svg" => Ok(Some(std::borrow::Cow::Borrowed(include_bytes!(
                "assets/play-filled.svg"
            )))),
            "lennette_app_icon.png" => Ok(Some(std::borrow::Cow::Borrowed(include_bytes!(
                "assets/lennette_app_icon.png"
            )))),
            _ => Ok(None),
        }
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        Ok(vec!["lennette.png".into(), "lennette_app_icon.png".into()])
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("lennette=info")),
        )
        .init();

    // GPUI runs its own executor; reqwest/WebSockets and tokio::spawn still need
    // a live Tokio reactor on the UI thread for all provider requests.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("could not start the provider runtime");
    let _runtime_guard = runtime.enter();
    let services = Services::open();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        database = services.database_ready,
        vault = services.vault.is_some(),
        "starting lennette"
    );

    Application::new()
        .with_assets(EmbeddedAssets)
        .run(move |cx: &mut App| {
            crate::ui::widgets::syntax::warm_up();
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
                app_id: Some("dev.lennette.overlay".into()),
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
            // A tray is drawn by the desktop shell, outside the protected window.
            let tray_state = if std::env::var("LENNETTE_PRIVATE_OVERLAY").as_deref() == Ok("1") {
                tracing::info!("tray disabled for private overlay");
                None
            } else {
                Some(crate::features::tray::Tray::start())
            };
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
            let pure_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
                && std::env::var_os("DISPLAY").is_none();
            // The control socket lets a shortcut flag reach this instance.
            let control = crate::features::control::ControlListener::start();
            match &control {
                Ok(_) => tracing::info!("shortcut control socket ready"),
                Err(error) => tracing::warn!(%error, "shortcut control socket unavailable"),
            }

            // Hyprland has its own config; use it instead of the portal, which is
            // unreliable on some systems.
            if crate::features::hyprland::is_hyprland() {
                let bindings = crate::features::shortcuts::Binding::defaults();
                match crate::features::hyprland::install(&bindings) {
                    Ok(path) => {
                        tracing::info!(path = %path.display(), "wrote Hyprland bindings");
                        if let Some(directory) = crate::features::hyprland::config_directory()
                            && !crate::features::hyprland::is_required(&directory)
                        {
                            tracing::warn!(
                                line = crate::features::hyprland::required_line(),
                                "add this line to hyprland.lua so the bindings survive a restart"
                            );
                        }
                        match crate::features::hyprland::apply_now(&bindings) {
                            Ok(count) => tracing::info!(count, "Hyprland bindings applied now"),
                            Err(error) => tracing::warn!(%error, "could not apply bindings now"),
                        }
                    }
                    Err(error) => {
                        tracing::debug!(%error, "Hyprland bindings are managed by the system");
                        if let Err(error) = crate::features::hyprland::repair_managed_toggle() {
                            tracing::warn!(%error, "could not repair managed toggle binding");
                        }
                    }
                }
            }

            if pure_wayland && !crate::features::hyprland::is_hyprland() {
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
            } else if !crate::features::hyprland::is_hyprland() {
                tracing::warn!(%message, "global shortcuts disabled");
            }

            let portal_clone = portal_shortcuts.clone();
            let control_handle = control.ok().map(std::sync::Arc::new);
            cx.open_window(overlay_options, move |window, cx| {
                window.set_window_title("lennette");
                window.set_app_id("dev.lennette.overlay");
                window.activate_window();
                cx.new(|_| {
                    Overlay::new(
                        services_for_overlay,
                        shortcut_listener.clone(),
                        tray_state,
                        portal_clone,
                        control_handle,
                    )
                })
            })
            .expect("failed to open the overlay window");

            cx.activate(true);
        });
}

/// Open (or focus) the dashboard window.
pub fn open_dashboard(cx: &mut App, services: Services) {
    open_dashboard_at(
        cx,
        services,
        crate::features::dashboard::nav::Route::Conversations,
    );
}

/// Open the dashboard on a specific page.
pub fn open_dashboard_at(
    cx: &mut App,
    services: Services,
    route: crate::features::dashboard::nav::Route,
) {
    for handle in cx.windows() {
        if let Some(dashboard) = handle.downcast::<DashboardView>()
            && dashboard
                .update(cx, |view, window, cx| {
                    view.navigate_to(route);
                    window.activate_window();
                    cx.notify();
                })
                .is_ok()
        {
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(DASHBOARD_WIDTH), px(DASHBOARD_HEIGHT)), cx);
    let options = WindowOptions {
        app_id: Some("dev.lennette.overlay".into()),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        titlebar: Some(TitlebarOptions {
            title: Some("lennette dashboard".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    if let Err(error) = cx.open_window(options, move |window, cx| {
        window.set_window_title("lennette dashboard");
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
