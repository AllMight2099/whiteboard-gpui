mod canvas_view;
mod html;
mod tile;
mod tile_window;

use canvas_view::CanvasView;
use gpui::*;

fn main() {
    Application::new().run(|app| {
        let bounds = Bounds::centered(None, size(px(1400.0), px(900.0)), app);
        app.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Whiteboard Browser [gpui]")),
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| CanvasView::new(cx)),
        )
        .unwrap();
    });
}
