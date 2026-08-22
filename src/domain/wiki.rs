use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WikiPage {
    #[serde(rename = "repo")]
    pub repository: String,
    pub slug: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct WikiDetail {
    #[serde(flatten)]
    pub page: WikiPage,
    pub links_to: Vec<String>,
    pub backlinks: Vec<String>,
}

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Extract normalized `[[slug]]` targets, preserving first-seen order.
pub fn parse_links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("]]") {
            let target = slugify(&rest[..end]);
            if !target.is_empty() && !out.contains(&target) {
                out.push(target);
            }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    out
}
