mod app;
mod errors;
mod loaded;
mod transactions_table;

use app::{FocusSearch, OpenStatement, ShowSummary, ShowTransactions, StatementApp};
use gpui_kit::{
    AppContext as _, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);

            cx.bind_keys([
                KeyBinding::new("ctrl-o", OpenStatement, Some("StatementApp")),
                KeyBinding::new("ctrl-f", FocusSearch, Some("StatementApp")),
                KeyBinding::new("ctrl-1", ShowTransactions, Some("StatementApp")),
                KeyBinding::new("ctrl-2", ShowSummary, Some("StatementApp")),
            ]);

            let titlebar = TitlebarOptions {
                title: Some("M-Pesa Statements".into()),
                ..TitlebarOptions::default()
            };

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1120.), px(780.)),
                    cx,
                ))),
                window_min_size: Some(size(px(880.), px(560.))),
                titlebar: Some(titlebar),
                app_id: Some("m-pesa-statements".to_owned()),
                ..WindowOptions::default()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                gpui_kit::component::Theme::sync_system_appearance(Some(window), cx);
                cx.new(|cx| StatementApp::new(window, cx))
            })
            .expect("failed to open the statement window");
        });
}
