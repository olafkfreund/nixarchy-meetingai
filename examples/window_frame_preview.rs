//! Capture-safe visual fixture: real views, isolated data, no keys or capture exclusion.
//! cargo run --example window_frame_preview -- <scratch-directory> dark 50 [dashboard]
use std::{path::PathBuf, sync::Arc};

use gpui::{
    AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use lennette::{
    Services,
    app::EmbeddedAssets,
    core::models::settings::{Settings, ThemeChoice},
    db::Database,
    ui::views::{dashboard::view::DashboardView, overlay::Overlay},
};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let directory = PathBuf::from(
        args.first()
            .expect("an isolated scratch directory is required"),
    );
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("preview.db");
    let database = Database::open(&path)?;
    let settings = Settings {
        theme: match args.get(1).map(String::as_str) {
            Some("light") => ThemeChoice::Light,
            Some("glass") => ThemeChoice::Glass,
            _ => ThemeChoice::Dark,
        },
        transparency: args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50),
        ..Default::default()
    };
    settings.save(&database)?;
    let dashboard = args.get(3).is_some_and(|s| s == "dashboard");
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    Application::new()
        .with_assets(EmbeddedAssets)
        .run(move |cx| {
            cx.text_system()
                .add_fonts(vec![
                    lennette::ui::theme::icons::FONT_BYTES.into(),
                    include_bytes!("../src/assets/fonts/OTF/Satoshi-Regular.otf")
                        .as_slice()
                        .into(),
                    include_bytes!("../src/assets/fonts/OTF/Satoshi-Medium.otf")
                        .as_slice()
                        .into(),
                    include_bytes!("../src/assets/fonts/OTF/Satoshi-Bold.otf")
                        .as_slice()
                        .into(),
                ])
                .expect("preview fonts");
            let dimensions = if dashboard {
                size(px(1000.0), px(760.0))
            } else {
                size(px(720.0), px(620.0))
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None, dimensions, cx,
                ))),
                window_background: gpui::WindowBackgroundAppearance::Transparent,
                titlebar: Some(TitlebarOptions {
                    title: Some("Lennette visual preview".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            };
            if dashboard {
                cx.open_window(options, |window, cx| {
                    lennette::platform::window::configure_frame(window);
                    cx.new(|_| {
                        let mut view = DashboardView::new(Some(database));
                        view.navigate_to(lennette::ui::views::dashboard::nav::Route::Settings);
                        view
                    })
                })
                .expect("dashboard preview");
            } else {
                cx.open_window(options, |window, cx| {
                    lennette::platform::window::configure_frame(window);
                    cx.new(|_| {
                        Overlay::new(
                            Services {
                                vault: Arc::new(None),
                                database_path: Some(path),
                                database_ready: true,
                            },
                            None,
                            None,
                            None,
                            None,
                        )
                    })
                })
                .expect("overlay preview");
            }
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
    Ok(())
}
