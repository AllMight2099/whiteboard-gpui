use gpui::*;

pub type TileId = u64;

#[derive(Clone, Debug)]
pub enum TileContent {
    Empty,
    Loading,
    Loaded(Vec<String>),
    Error(String),
}

pub const TILE_HEADER_HEIGHT: f32 = 36.0;
pub const TILE_URL_BAR_HEIGHT: f32 = 30.0;
pub const TILE_CHROME_HEIGHT: f32 = TILE_HEADER_HEIGHT + TILE_URL_BAR_HEIGHT;

pub struct BrowserTile {
    pub id: TileId,
    pub canvas_pos: Point<f32>,
    pub canvas_size: Size<f32>,
    pub url: String,
    pub url_input: String,
    pub title: String,
    pub content: TileContent,
    pub scroll_y: f32,
    pub url_focus: FocusHandle,
}

impl BrowserTile {
    pub fn new(id: TileId, pos: Point<f32>, url_focus: FocusHandle) -> Self {
        Self {
            id,
            canvas_pos: pos,
            canvas_size: size(700.0, 500.0),
            url: String::new(),
            url_input: String::new(),
            title: "New Tab".to_string(),
            content: TileContent::Empty,
            scroll_y: 0.0,
            url_focus,
        }
    }

    pub fn screen_bounds(&self, offset: Point<f32>, zoom: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(
                px(self.canvas_pos.x * zoom + offset.x),
                px(self.canvas_pos.y * zoom + offset.y),
            ),
            size: size(
                px(self.canvas_size.width * zoom),
                px(self.canvas_size.height * zoom),
            ),
        }
    }

    /// Title-bar strip (top TILE_HEADER_HEIGHT px of tile)
    pub fn header_bounds(&self, offset: Point<f32>, zoom: f32) -> Bounds<Pixels> {
        let b = self.screen_bounds(offset, zoom);
        Bounds {
            origin: b.origin,
            size: size(b.size.width, px(TILE_HEADER_HEIGHT * zoom)),
        }
    }

    /// URL-bar strip below the title bar
    pub fn url_bar_bounds(&self, offset: Point<f32>, zoom: f32) -> Bounds<Pixels> {
        let b = self.screen_bounds(offset, zoom);
        Bounds {
            origin: point(b.origin.x, b.origin.y + px(TILE_HEADER_HEIGHT * zoom)),
            size: size(b.size.width, px(TILE_URL_BAR_HEIGHT * zoom)),
        }
    }

    /// Close button (right part of header)
    pub fn close_btn_bounds(&self, offset: Point<f32>, zoom: f32) -> Bounds<Pixels> {
        let hdr = self.header_bounds(offset, zoom);
        let btn = TILE_HEADER_HEIGHT * zoom;
        Bounds {
            origin: point(hdr.origin.x + hdr.size.width - px(btn), hdr.origin.y),
            size: size(px(btn), px(btn)),
        }
    }
}

/// Returns true if `pt` is inside `bounds`.
pub fn hit(pt: Point<Pixels>, bounds: &Bounds<Pixels>) -> bool {
    pt.x >= bounds.origin.x
        && pt.x <= bounds.origin.x + bounds.size.width
        && pt.y >= bounds.origin.y
        && pt.y <= bounds.origin.y + bounds.size.height
}
