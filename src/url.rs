use http::HeaderMap;
use reqwest::Url;

// TODO change this to &str later
#[derive(Debug, Clone)]
pub struct URL {
    // scheme: String,
    // host: String,
    // path: String,
    // uri: String,
    url: Url,
}

impl URL {
    pub fn url(url: String) -> Option<Self> {
        // let scheme: String;
        // let host: String;
        // let path: String;
        let uri: String;

        let parsed = match Url::parse(&url) {
            Ok(a) => return Some(Self { url: a }),
            Err(_) => return None,
        };
    }

    pub fn request(self) -> String {
        let request = reqwest::blocking::get(self.url.as_str());
        let mut headers: HeaderMap = HeaderMap::new();
        let mut body: String = String::new();
        match request {
            Ok(response) => {
                // println!("Response Status: {}", response.status());
                headers = response.headers().clone();

                // println!("Response Headers:\n{:#?}", response.headers());
                match response.text() {
                    Ok(text) => {
                        body = text;
                    }
                    Err(e) => {
                        println!("Failed to read response body: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Request failed: {}", e);
            }
        }

        for header in headers.iter() {
            assert!(header.0 != "transfer-encoding");
            assert!(header.0 != "content-encoding");
            // println!("{}: {:?}", header.0, header.1);
        }

        return body;
    }
}
