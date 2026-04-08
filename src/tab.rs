use crate::url::URL;

pub struct Tab {
    pub display_list: Vec<String>,
    text: String,
    pub scroll: f32,
    pub url: URL,
}

impl Tab {
    pub fn new(url: URL) -> Self {
        Self {
            display_list: Vec::new(),
            text: String::new(),
            scroll: 0.0,
            url,
        }
    }
}
