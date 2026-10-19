//! Reusable native desktop application entry point.
//!
//! The executable target is intentionally a thin launcher. Keeping startup and
//! the GPUI root in the library makes the native application testable and gives
//! future tools (for example a review or diagnostics launcher) one shared path.

mod assets;
mod bootstrap;
mod heroicons;
mod theme;
mod ui;

use gpui_kit::*;
use skillissue_domain::identity::APPLICATION_ID;
use std::sync::Arc;

/// Run the native GPUI application with the normal profile and command-line
/// review/smoke switches.
pub fn run() {
    if skillissue_core::documents::run_worker_if_requested() {
        return;
    }
    if let Err(error) = skillissue_platform::initialize_app_identity() {
        eprintln!("{error}");
    }
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("create skillissue workers"),
    );
    let data_dir = bootstrap::data_directory().expect("resolve skillissue data directory");
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let reviewing = std::env::args().any(|arg| arg.starts_with("--review-output="));
    let show_window = smoke || reviewing || std::env::args().any(|arg| arg == "--smoke-gallery");
    let app_runtime = runtime.clone();
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            assets::load_fonts(cx);
            component::Theme::change(component::ThemeMode::from(cx.window_appearance()), None, cx);
            theme::apply(cx);
            let result = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(780.), px(760.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(680.), px(600.))),
                    titlebar: None,
                    window_background: WindowBackgroundAppearance::Blurred,
                    show: show_window,
                    focus: show_window && !reviewing,
                    app_id: Some(APPLICATION_ID.into()),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        ui::NativeView::new(app_runtime.clone(), data_dir, smoke, window, cx)
                    })
                },
            );
            if let Err(error) = result {
                eprintln!("skillissue window could not open: {error:#}");
                cx.quit();
            }
            if show_window && !reviewing {
                cx.activate(true);
            }
        });
}
