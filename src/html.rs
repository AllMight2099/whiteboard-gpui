/// Convert HTML to readable text lines
pub fn html_to_text(html: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_buf = String::new();

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if in_tag {
            if ch == '>' {
                let tag = tag_buf.trim().to_lowercase();
                let tag_name = tag
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches('/');

                match tag_name {
                    "script" => in_script = true,
                    "/script" => in_script = false,
                    "style" => in_style = true,
                    "/style" => in_style = false,
                    _ => {}
                }

                if matches!(
                    tag_name,
                    "br" | "/p"
                        | "/div"
                        | "/h1"
                        | "/h2"
                        | "/h3"
                        | "/h4"
                        | "/h5"
                        | "/h6"
                        | "/li"
                        | "/tr"
                        | "/td"
                        | "/th"
                ) {
                    let trimmed = current_line.trim().to_string();
                    if !trimmed.is_empty() {
                        result.push(trimmed);
                    } else {
                        result.push(String::new());
                    }
                    current_line.clear();
                }

                if matches!(tag_name, "/h1" | "/h2" | "/h3") {
                    result.push(String::new());
                }

                tag_buf.clear();
                in_tag = false;
            } else {
                tag_buf.push(ch);
            }
        } else if ch == '<' {
            in_tag = true;
            tag_buf.clear();
        } else if !in_script && !in_style {
            if ch == '&' {
                let mut entity = String::new();
                let start = i;
                i += 1;
                while i < len && chars[i] != ';' && chars[i] != ' ' && i - start < 12 {
                    entity.push(chars[i]);
                    i += 1;
                }
                if i < len && chars[i] == ';' {
                    let decoded = match entity.as_str() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "nbsp" => " ",
                        "quot" => "\"",
                        "apos" => "'",
                        "copy" => "©",
                        "reg" => "®",
                        "mdash" => "—",
                        "ndash" => "–",
                        "hellip" => "...",
                        "laquo" => "«",
                        "raquo" => "»",
                        _ => "",
                    };
                    current_line.push_str(decoded);
                } else {
                    current_line.push('&');
                    current_line.push_str(&entity);
                    i -= 1; // we'll re-process the non-semicolon char
                }
            } else if ch == '\n' || ch == '\r' {
                if !current_line.ends_with(' ') && !current_line.is_empty() {
                    current_line.push(' ');
                }
            } else {
                current_line.push(ch);
            }
        }

        i += 1;
    }

    if !current_line.trim().is_empty() {
        result.push(current_line.trim().to_string());
    }

    // Collapse consecutive blank lines to at most one
    let mut cleaned: Vec<String> = Vec::new();
    let mut prev_empty = false;
    for line in result {
        let is_empty = line.trim().is_empty();
        if is_empty && prev_empty {
            continue;
        }
        prev_empty = is_empty;
        cleaned.push(line);
    }

    cleaned
}
