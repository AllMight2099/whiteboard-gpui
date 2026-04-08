use crate::canvas_view::CanvasView;
use crate::html::html_to_text;
use crate::tile::{TileContent, TileId};
use futures::channel::oneshot;
use gpui::*;

// ── State ────────────────────────────────────────────────────────────────────

pub struct TileWindowView {
    pub tile_id: TileId,
    pub url: String,
    pub url_input: String,
    pub title: String,
    pub content: TileContent,
    pub url_focus: FocusHandle,
    /// Weak back-reference so we can sync state and switch mode on "Back to Canvas".
    pub canvas_view: WeakEntity<CanvasView>,
    pub _tasks: Vec<Task<()>>,
}

impl TileWindowView {
    pub fn new(
        tile_id: TileId,
        url: String,
        url_input: String,
        title: String,
        content: TileContent,
        canvas_view: WeakEntity<CanvasView>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            tile_id,
            url,
            url_input,
            title,
            content,
            url_focus: cx.focus_handle(),
            canvas_view,
            _tasks: Vec::new(),
        }
    }

    // ── Navigation ───────────────────────────────────────────────────────────

    fn navigate(&mut self, url: String, cx: &mut Context<Self>) {
        self.content = TileContent::Loading;
        self.url = url.clone();
        self.title = "Loading\u{2026}".to_string();
        cx.notify();

        let (tx, rx) = oneshot::channel::<Result<String, String>>();

        std::thread::spawn(move || {
            let result = reqwest::blocking::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; whiteboard-browser/0.1)")
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .map_err(|e| e.to_string())
                .and_then(|client| client.get(&url).send().map_err(|e| e.to_string()))
                .and_then(|resp| resp.text().map_err(|e| e.to_string()));
            let _ = tx.send(result);
        });

        let task = cx.spawn(async move |weak: WeakEntity<TileWindowView>, cx| {
            if let Ok(result) = rx.await {
                let content = match result {
                    Ok(html) => TileContent::Loaded(html_to_text(&html)),
                    Err(e) => TileContent::Error(e),
                };
                let _ = weak.update(cx, |view, cx| {
                    view.title = match &content {
                        TileContent::Loaded(lines) => {
                            lines.first().cloned().unwrap_or_else(|| view.url.clone())
                        }
                        _ => view.url.clone(),
                    };
                    view.content = content;
                    cx.notify();
                });
            }
        });
        self._tasks.push(task);
    }

    // ── Mode switching ───────────────────────────────────────────────────────

    /// Sync this window's state back into the canvas tile, then ask the canvas
    /// to switch back to Canvas mode (which will close all tile windows).
    fn back_to_canvas(&mut self, cx: &mut Context<Self>) {
        let tile_id = self.tile_id;
        let url = self.url.clone();
        let url_input = self.url_input.clone();
        let title = self.title.clone();
        let content = self.content.clone();

        self.canvas_view
            .update(cx, |canvas, cx| {
                canvas.sync_tile_from_window(tile_id, url, url_input, title, content, cx);
                canvas.switch_to_canvas_mode(cx);
            })
            .ok();
        // switch_to_canvas_mode closes all tile windows (including this one)
        // via WindowHandle::update, so we don't need to close ourselves here.
    }
}

// ── Render ───────────────────────────────────────────────────────────────────

impl Render for TileWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_url_focused = self.url_focus.is_focused(window);

        let url_input_display = if is_url_focused {
            format!("{}|", self.url_input)
        } else if self.url_input.is_empty() {
            "Enter URL\u{2026}".to_string()
        } else {
            self.url_input.clone()
        };

        let title = self.title.clone();
        let content = self.content.clone();

        div()
            .id("tile-window-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(white())
            // ── Top toolbar ───────────────────────────────────────────────
            .child(
                div()
                    .id("tile-window-toolbar")
                    .h(px(40.0))
                    .w_full()
                    .flex_shrink_0()
                    .bg(hsla(0.65, 0.25, 0.14, 1.0))
                    .border_b_1()
                    .border_color(hsla(0.65, 0.3, 0.25, 0.8))
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .gap(px(8.0))
                    // Title text
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(hsla(0.65, 0.3, 0.8, 1.0))
                            .truncate()
                            .child(SharedString::from(title)),
                    )
                    // "Back to Canvas" button
                    .child(
                        div()
                            .id("back-to-canvas-btn")
                            .px(px(10.0))
                            .py(px(4.0))
                            .bg(hsla(0.65, 0.5, 0.28, 1.0))
                            .rounded_md()
                            .text_sm()
                            .text_color(white())
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _evt, _window, cx| {
                                    this.back_to_canvas(cx);
                                }),
                            )
                            .child("\u{25C4} Back to Canvas"),
                    ),
            )
            // ── URL bar ───────────────────────────────────────────────────
            .child(
                div()
                    .id("tile-window-url-bar")
                    .track_focus(&self.url_focus)
                    .h(px(30.0))
                    .w_full()
                    .flex_shrink_0()
                    .bg(hsla(0.0, 0.0, 0.97, 1.0))
                    .border_b_1()
                    .border_color(hsla(0.65, 0.1, 0.8, 0.6))
                    .flex()
                    .items_center()
                    .px(px(10.0))
                    .cursor_text()
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                        let char_to_insert: Option<String> = event
                            .keystroke
                            .key_char
                            .as_deref()
                            .filter(|s| s.chars().all(|c| c.is_ascii() && !c.is_control()))
                            .map(|s| s.to_string());

                        match event.keystroke.key.as_str() {
                            "backspace" => {
                                this.url_input.pop();
                                cx.notify();
                            }
                            "return" => {
                                let raw = this.url_input.trim().to_string();
                                if !raw.is_empty() {
                                    let url = if raw.starts_with("http://")
                                        || raw.starts_with("https://")
                                    {
                                        raw
                                    } else {
                                        format!("https://{}", raw)
                                    };
                                    this.navigate(url, cx);
                                }
                            }
                            "escape" => {
                                cx.notify();
                            }
                            _ => {
                                if let Some(ch) = char_to_insert {
                                    this.url_input.push_str(&ch);
                                    cx.notify();
                                }
                            }
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(if is_url_focused {
                                hsla(0.0, 0.0, 0.05, 1.0)
                            } else {
                                hsla(0.0, 0.0, 0.4, 1.0)
                            })
                            .truncate()
                            .child(SharedString::from(url_input_display)),
                    ),
            )
            // ── Content area ──────────────────────────────────────────────
            .child(
                div()
                    .id("tile-window-content")
                    .flex_1()
                    .w_full()
                    .bg(white())
                    .overflow_hidden()
                    .p(px(12.0))
                    .children(render_tile_content(&content)),
            )
    }
}

// ── Content renderer (mirrors canvas_view::render_content) ───────────────────

pub fn render_tile_content(content: &TileContent) -> Vec<AnyElement> {
    match content {
        TileContent::Empty => vec![
            div()
                .text_sm()
                .text_color(hsla(0.0, 0.0, 0.5, 1.0))
                .child("Click the URL bar above and type a URL, then press Enter")
                .into_any_element(),
        ],

        TileContent::Loading => vec![
            div()
                .text_sm()
                .text_color(hsla(0.6, 0.5, 0.4, 1.0))
                .child("\u{27F3} Loading\u{2026}")
                .into_any_element(),
        ],

        TileContent::Error(e) => vec![
            div()
                .text_sm()
                .text_color(hsla(0.0, 0.8, 0.4, 1.0))
                .child(format!("\u{26A0} Error: {}", e))
                .into_any_element(),
        ],

        TileContent::Loaded(lines) => lines
            .iter()
            .take(300)
            .map(|line| {
                if line.is_empty() {
                    div().h(px(8.0)).into_any_element()
                } else {
                    div()
                        .text_sm()
                        .text_color(hsla(0.0, 0.0, 0.15, 1.0))
                        .child(SharedString::from(line.clone()))
                        .into_any_element()
                }
            })
            .collect(),
    }
}
