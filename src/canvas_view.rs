use crate::html::html_to_text;
use crate::tile::*;
use crate::tile_window::TileWindowView;
use futures::channel::oneshot;
use gpui::prelude::FluentBuilder as _;
use gpui::*;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ViewMode {
    Canvas,
    Windows,
}

#[derive(Copy, Clone)]
pub enum DragState {
    None,
    PanningCanvas {
        start_mouse: Point<f32>,
        start_offset: Point<f32>,
    },
    MovingTile {
        id: TileId,
        start_mouse: Point<f32>,
        start_pos: Point<f32>,
    },
}

pub struct CanvasView {
    pub offset: Point<f32>,
    pub zoom: f32,
    pub tiles: Vec<BrowserTile>,
    pub next_tile_id: TileId,
    pub drag_state: DragState,
    pub _tasks: Vec<Task<()>>,
    pub view_mode: ViewMode,
    pub tile_windows: Vec<WindowHandle<TileWindowView>>,
}

impl CanvasView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            offset: point(100.0_f32, 80.0_f32),
            zoom: 1.0,
            tiles: Vec::new(),
            next_tile_id: 0,
            drag_state: DragState::None,
            _tasks: Vec::new(),
            view_mode: ViewMode::Canvas,
            tile_windows: Vec::new(),
        };
        view.add_tile(point(50.0_f32, 50.0_f32), cx);
        view.add_tile(point(800.0_f32, 50.0_f32), cx);
        view
    }

    fn add_tile(&mut self, pos: Point<f32>, cx: &mut Context<Self>) {
        let id = self.next_tile_id;
        self.next_tile_id += 1;
        let focus = cx.focus_handle();
        self.tiles.push(BrowserTile::new(id, pos, focus));
    }

    // ── Mode switching ───────────────────────────────────────────────────────

    /// Called by TileWindowView to sync its updated state back into the canvas
    /// tile before switching back to canvas mode.
    pub fn sync_tile_from_window(
        &mut self,
        tile_id: TileId,
        url: String,
        url_input: String,
        title: String,
        content: TileContent,
        cx: &mut Context<Self>,
    ) {
        if let Some(tile) = self.tiles.iter_mut().find(|t| t.id == tile_id) {
            tile.url = url;
            tile.url_input = url_input;
            tile.title = title;
            tile.content = content;
        }
        cx.notify();
    }

    /// Open a native OS window for every tile and switch to Windows mode.
    pub fn switch_to_windows_mode(&mut self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();

        // Snapshot tile data — each window gets an independent copy.
        let snapshots: Vec<(TileId, String, String, String, TileContent)> = self
            .tiles
            .iter()
            .map(|t| {
                (
                    t.id,
                    t.url.clone(),
                    t.url_input.clone(),
                    t.title.clone(),
                    t.content.clone(),
                )
            })
            .collect();

        let mut handles = Vec::new();
        for (i, (tile_id, url, url_input, title, content)) in snapshots.into_iter().enumerate() {
            let canvas_weak = weak.clone();
            let title_ss = SharedString::from(title.clone());
            // Cascade windows so they don't stack perfectly on top of each other.
            let origin = point(
                px(80.0 + i as f32 * 40.0),
                px(80.0 + i as f32 * 40.0),
            );
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin,
                            size: size(px(900.0), px(700.0)),
                        })),
                        titlebar: Some(TitlebarOptions {
                            title: Some(title_ss),
                            appears_transparent: false,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    move |_window, cx| {
                        cx.new(|cx| {
                            TileWindowView::new(
                                tile_id,
                                url,
                                url_input,
                                title,
                                content,
                                canvas_weak,
                                cx,
                            )
                        })
                    },
                )
                .unwrap();
            handles.push(handle);
        }

        self.tile_windows = handles;
        self.view_mode = ViewMode::Windows;
        cx.notify();
    }

    /// Close all tile windows and return to canvas mode.
    pub fn switch_to_canvas_mode(&mut self, cx: &mut Context<Self>) {
        for handle in self.tile_windows.drain(..) {
            let _ = handle.update(cx, |_, window, _cx| window.remove_window());
        }
        self.view_mode = ViewMode::Canvas;
        cx.notify();
    }

    // ── Navigation ───────────────────────────────────────────────────────────

    fn navigate_tile(&mut self, tile_id: TileId, url: String, cx: &mut Context<Self>) {
        if let Some(tile) = self.tiles.iter_mut().find(|t| t.id == tile_id) {
            tile.content = TileContent::Loading;
            tile.url = url.clone();
            tile.title = "Loading\u{2026}".to_string();
        }
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

        let task = cx.spawn(async move |weak: WeakEntity<CanvasView>, cx| {
            if let Ok(result) = rx.await {
                let content = match result {
                    Ok(html) => TileContent::Loaded(html_to_text(&html)),
                    Err(e) => TileContent::Error(e),
                };
                let _ = weak.update(cx, |view, cx| {
                    if let Some(tile) = view.tiles.iter_mut().find(|t| t.id == tile_id) {
                        tile.content = content.clone();
                        tile.title = match &content {
                            TileContent::Loaded(lines) => {
                                lines.first().cloned().unwrap_or_else(|| tile.url.clone())
                            }
                            _ => tile.url.clone(),
                        };
                    }
                    cx.notify();
                });
            }
        });
        self._tasks.push(task);
    }

    // ── Mouse handlers ────────────────────────────────────────────────────────

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pos = event.position;
        let pt = point(f32::from(pos.x), f32::from(pos.y));
        let offset = self.offset;
        let zoom = self.zoom;

        let tile_ids: Vec<TileId> = self.tiles.iter().map(|t| t.id).collect();
        for &id in tile_ids.iter().rev() {
            // Collect all hit-test bounds and data in one borrow — the borrow
            // ends when `hit_info` is assigned so we can mutate freely below.
            let hit_info = self.tiles.iter().find(|t| t.id == id).map(|tile| {
                (
                    tile.close_btn_bounds(offset, zoom),
                    tile.url_bar_bounds(offset, zoom),
                    tile.header_bounds(offset, zoom),
                    tile.canvas_pos,
                    tile.url_focus.clone(),
                )
            });

            let Some((close_b, url_b, hdr_b, canvas_pos, url_focus)) = hit_info else {
                continue;
            };

            if hit(pos, &close_b) {
                self.tiles.retain(|t| t.id != id);
                cx.notify();
                return;
            }

            if hit(pos, &url_b) {
                window.focus(&url_focus);
                cx.notify();
                return;
            }

            if hit(pos, &hdr_b) {
                self.drag_state = DragState::MovingTile {
                    id,
                    start_mouse: pt,
                    start_pos: canvas_pos,
                };
                cx.notify();
                return;
            }
        }

        // No tile hit → pan canvas
        self.drag_state = DragState::PanningCanvas {
            start_mouse: pt,
            start_offset: offset,
        };
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pos = event.position;
        let pt = point(f32::from(pos.x), f32::from(pos.y));

        // DragState is Copy, so match by value to avoid any borrow conflicts
        // when we subsequently mutate self.offset / self.tiles.
        match self.drag_state {
            DragState::PanningCanvas {
                start_mouse,
                start_offset,
            } => {
                let dx = pt.x - start_mouse.x;
                let dy = pt.y - start_mouse.y;
                self.offset = point(start_offset.x + dx, start_offset.y + dy);
                cx.notify();
            }
            DragState::MovingTile {
                id,
                start_mouse,
                start_pos,
            } => {
                let dx = (pt.x - start_mouse.x) / self.zoom;
                let dy = (pt.y - start_mouse.y) / self.zoom;
                let new_x = start_pos.x + dx;
                let new_y = start_pos.y + dy;
                if let Some(tile) = self.tiles.iter_mut().find(|t| t.id == id) {
                    tile.canvas_pos = point(new_x, new_y);
                }
                cx.notify();
            }
            DragState::None => {}
        }
    }

    fn on_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.drag_state = DragState::None;
        cx.notify();
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta_y = match event.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(p) => p.y * 20.0,
        };

        let zoom_factor: f32 = if delta_y < 0.0 { 1.1 } else { 0.9 };
        let mouse_x = f32::from(event.position.x);
        let mouse_y = f32::from(event.position.y);

        // Canvas point under mouse (before zoom change)
        let canvas_x = (mouse_x - self.offset.x) / self.zoom;
        let canvas_y = (mouse_y - self.offset.y) / self.zoom;

        self.zoom = (self.zoom * zoom_factor).clamp(0.1, 5.0);

        // Keep the canvas point under mouse fixed
        self.offset.x = mouse_x - canvas_x * self.zoom;
        self.offset.y = mouse_y - canvas_y * self.zoom;

        cx.notify();
    }

    // ── Rendering helpers ────────────────────────────────────────────────────

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom_pct = (self.zoom * 100.0).round() as i32;
        let mode = self.view_mode;

        div()
            .absolute()
            .left_0()
            .top_0()
            .right_0()
            .h(px(40.0))
            .bg(hsla(0.65, 0.3, 0.12, 0.95))
            .border_b_1()
            .border_color(hsla(0.65, 0.3, 0.3, 0.5))
            .flex()
            .items_center()
            .px(px(12.0))
            .gap(px(8.0))
            .child(
                div()
                    .text_sm()
                    .text_color(hsla(0.0, 0.0, 0.8, 1.0))
                    .child("\u{1F310} Infinite Canvas Browser"),
            )
            // "+ New Tab" — only useful in canvas mode
            .when(mode == ViewMode::Canvas, |el| {
                el.child(
                    div()
                        .id("btn-new-tile")
                        .text_sm()
                        .px(px(10.0))
                        .py(px(4.0))
                        .bg(hsla(0.65, 0.5, 0.3, 1.0))
                        .rounded_md()
                        .text_color(white())
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _evt, _window, cx| {
                                let x = (400.0 - this.offset.x) / this.zoom;
                                let y = (300.0 - this.offset.y) / this.zoom;
                                this.add_tile(point(x, y), cx);
                                cx.notify();
                            }),
                        )
                        .child("+ New Tab"),
                )
            })
            // Mode-toggle button
            .child(if mode == ViewMode::Canvas {
                div()
                    .id("btn-mode-toggle")
                    .text_sm()
                    .px(px(10.0))
                    .py(px(4.0))
                    .bg(hsla(0.12, 0.6, 0.35, 1.0))
                    .rounded_md()
                    .text_color(white())
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _evt, _window, cx| {
                            this.switch_to_windows_mode(cx);
                        }),
                    )
                    .child("\u{29C9} Expand to Windows")
                    .into_any_element()
            } else {
                div()
                    .id("btn-mode-toggle")
                    .text_sm()
                    .px(px(10.0))
                    .py(px(4.0))
                    .bg(hsla(0.55, 0.6, 0.32, 1.0))
                    .rounded_md()
                    .text_color(white())
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _evt, _window, cx| {
                            this.switch_to_canvas_mode(cx);
                        }),
                    )
                    .child("\u{25C4} Back to Canvas")
                    .into_any_element()
            })
            .child(
                div()
                    .text_sm()
                    .text_color(hsla(0.0, 0.0, 0.5, 1.0))
                    .child(if mode == ViewMode::Canvas {
                        format!(
                            "Zoom: {}%  |  drag to pan  |  scroll to zoom  |  drag title to move",
                            zoom_pct
                        )
                    } else {
                        "Windows mode — tiles are in individual windows".to_string()
                    }),
            )
    }
}

// ── Dot-grid background ─────────────────────────────────────────────────────

fn draw_dot_grid(bounds: Bounds<Pixels>, offset: Point<f32>, zoom: f32, window: &mut Window) {
    let spacing = (30.0 * zoom).max(8.0);
    let dot = 1.5_f32 * zoom.min(2.0);
    let color = hsla(0.65, 0.3, 0.5, 0.25);

    let start_x = offset.x.rem_euclid(spacing);
    let start_y = offset.y.rem_euclid(spacing);

    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);

    let mut x = start_x;
    while x < width {
        let mut y = start_y;
        while y < height {
            window.paint_quad(fill(
                Bounds {
                    origin: point(
                        bounds.origin.x + px(x - dot / 2.0),
                        bounds.origin.y + px(y - dot / 2.0),
                    ),
                    size: size(px(dot), px(dot)),
                },
                color,
            ));
            y += spacing;
        }
        x += spacing;
    }
}

// ── Render ──────────────────────────────────────────────────────────────────

impl Render for CanvasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let offset = self.offset;
        let zoom = self.zoom;
        let mode = self.view_mode;

        // Snapshot tile data so we can pass it into element closures without
        // holding a borrow on `self.tiles`.
        struct TileSnapshot {
            id: TileId,
            origin: Point<Pixels>,
            full_w: Pixels,
            full_h: Pixels,
            header_h: Pixels,
            url_bar_h: Pixels,
            title: SharedString,
            url_input: SharedString,
            content: TileContent,
            url_focus: FocusHandle,
            is_url_focused: bool,
        }

        let snapshots: Vec<TileSnapshot> = self
            .tiles
            .iter()
            .map(|tile| {
                let b = tile.screen_bounds(offset, zoom);
                TileSnapshot {
                    id: tile.id,
                    origin: b.origin,
                    full_w: b.size.width,
                    full_h: b.size.height,
                    header_h: px(TILE_HEADER_HEIGHT * zoom),
                    url_bar_h: px(TILE_URL_BAR_HEIGHT * zoom),
                    title: SharedString::from(tile.title.clone()),
                    url_input: SharedString::from(tile.url_input.clone()),
                    content: tile.content.clone(),
                    url_focus: tile.url_focus.clone(),
                    is_url_focused: tile.url_focus.is_focused(window),
                }
            })
            .collect();

        // In Windows mode, show a placeholder instead of the full canvas.
        if mode == ViewMode::Windows {
            return div()
                .id("canvas-root")
                .relative()
                .size_full()
                .bg(hsla(0.65, 0.2, 0.08, 1.0))
                .overflow_hidden()
                .child(self.render_toolbar(cx))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_color(hsla(0.65, 0.4, 0.7, 1.0))
                                .child("\u{29C9}  Windows Mode Active"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(hsla(0.0, 0.0, 0.5, 1.0))
                                .child("Your tiles are open in individual windows."),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(hsla(0.0, 0.0, 0.4, 1.0))
                                .child(
                                    "Click \u{201C}\u{25C4} Back to Canvas\u{201D} in any tile window or in the toolbar above.",
                                ),
                        ),
                )
        }

        let mut root = div()
            .id("canvas-root")
            .relative()
            .size_full()
            .bg(hsla(0.65, 0.2, 0.08, 1.0))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            // Dot-grid background canvas
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _state, window, _cx| {
                        draw_dot_grid(bounds, offset, zoom, window);
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
            // Toolbar — rendered above the canvas so it intercepts its own clicks
            .child(self.render_toolbar(cx));

        // Tiles
        for snap in snapshots {
            let id = snap.id;

            // Compute content area height as f32 first to avoid Pixels::0 access
            let raw_content_h =
                (f32::from(snap.full_h) - f32::from(snap.header_h) - f32::from(snap.url_bar_h))
                    .max(0.0);
            let content_h = px(raw_content_h);

            let is_focused = snap.is_url_focused;
            let url_input_display = if is_focused {
                format!("{}|", snap.url_input)
            } else if snap.url_input.is_empty() {
                "Enter URL\u{2026}".to_string()
            } else {
                snap.url_input.to_string()
            };

            let tile_el = div()
                // Use the tile id directly — ElementId::Integer expects u64
                .id(ElementId::Integer(snap.id))
                .absolute()
                .left(snap.origin.x)
                .top(snap.origin.y)
                .w(snap.full_w)
                .h(snap.full_h)
                .rounded(px(8.0))
                .border_1()
                .border_color(hsla(0.65, 0.3, 0.3, 0.8))
                .bg(hsla(0.65, 0.2, 0.12, 1.0))
                .overflow_hidden()
                // ── Title bar ──────────────────────────────────────────────
                .child(
                    div()
                        .id(ElementId::Name(format!("hdr-{}", id).into()))
                        .h(snap.header_h)
                        .w_full()
                        .bg(hsla(0.65, 0.25, 0.16, 1.0))
                        .flex()
                        .items_center()
                        .px(px(8.0))
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(hsla(0.65, 0.3, 0.75, 1.0))
                                .truncate()
                                .child(snap.title.clone()),
                        )
                        // Close button — red circle
                        .child(
                            div()
                                .id(ElementId::Name(format!("close-{}", id).into()))
                                .w(px(14.0))
                                .h(px(14.0))
                                .rounded_full()
                                .bg(hsla(0.0, 0.9, 0.6, 1.0))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _evt, _window, cx| {
                                        this.tiles.retain(|t| t.id != id);
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
                // ── URL bar ────────────────────────────────────────────────
                .child(
                    div()
                        .id(ElementId::Name(format!("url-bar-{}", id).into()))
                        .track_focus(&snap.url_focus)
                        .h(snap.url_bar_h)
                        .w_full()
                        .bg(hsla(0.0, 0.0, 0.95, 1.0))
                        .border_b_1()
                        .border_color(hsla(0.65, 0.2, 0.7, 0.5))
                        .flex()
                        .items_center()
                        .px(px(8.0))
                        .cursor_text()
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _window, cx| {
                            let char_to_insert: Option<String> = event
                                .keystroke
                                .key_char
                                .as_deref()
                                .filter(|s| s.chars().all(|c| c.is_ascii() && !c.is_control()))
                                .map(|s| s.to_string());

                            if let Some(tile) = this.tiles.iter_mut().find(|t| t.id == id) {
                                match event.keystroke.key.as_str() {
                                    "backspace" => {
                                        tile.url_input.pop();
                                        cx.notify();
                                    }
                                    "return" => {
                                        let raw = tile.url_input.trim().to_string();
                                        if !raw.is_empty() {
                                            let url = if raw.starts_with("http://")
                                                || raw.starts_with("https://")
                                            {
                                                raw
                                            } else {
                                                format!("https://{}", raw)
                                            };
                                            this.navigate_tile(id, url, cx);
                                        }
                                    }
                                    "escape" => {
                                        cx.notify();
                                    }
                                    _ => {
                                        if let Some(ch) = char_to_insert {
                                            tile.url_input.push_str(&ch);
                                            cx.notify();
                                        }
                                    }
                                }
                            }
                        }))
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(if is_focused {
                                    hsla(0.0, 0.0, 0.1, 1.0)
                                } else {
                                    hsla(0.0, 0.0, 0.4, 1.0)
                                })
                                .truncate()
                                .child(SharedString::from(url_input_display)),
                        ),
                )
                // ── Content area ───────────────────────────────────────────
                .child(
                    div()
                        .w_full()
                        .h(content_h)
                        .bg(white())
                        .overflow_hidden()
                        .p(px(10.0))
                        .children(render_content(&snap.content)),
                );

            root = root.child(tile_el);
        }

        root
    }
}

fn render_content(content: &TileContent) -> Vec<AnyElement> {
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
