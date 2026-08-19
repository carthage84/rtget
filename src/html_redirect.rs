use url::Url;

/// Best-effort extraction of a download URL from an HTML interstitial page.
///
/// Handles meta-refresh and the common `location.href.replace('a','b')` /
/// `location.replace('url')` / `location.href = 'url'` patterns used by
/// hotlink-protection pages. This is not a JavaScript engine.
pub(crate) fn extract_html_redirect(html: &str, base: &Url) -> Option<Url> {
    href_replace(html, base)
        .or_else(|| location_literal(html, base))
        .or_else(|| meta_refresh(html, base))
}

pub(crate) fn is_html_content_type(content_type: &str) -> bool {
    let ct = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase();
    ct == "text/html" || ct == "application/xhtml+xml"
}

pub(crate) fn url_looks_like_html(url: &Url) -> bool {
    let name = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.is_empty()
        || name.ends_with(".html")
        || name.ends_with(".htm")
        || name.ends_with(".php")
        || name.ends_with(".asp")
        || name.ends_with(".aspx")
}

pub(crate) fn looks_like_html_bytes(bytes: &[u8]) -> bool {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map(|i| &bytes[i..])
        .unwrap_or(bytes);
    let prefix = start.get(..64).unwrap_or(start);
    let s = String::from_utf8_lossy(prefix).to_ascii_lowercase();
    s.starts_with("<!doctype html") || s.starts_with("<html")
}

/// `location.href.replace('from','to')` applied to the current URL.
fn href_replace(html: &str, base: &Url) -> Option<Url> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("href.replace(") {
        let abs = from + rel + "href.replace(".len();
        if let Some((old, new)) = parse_two_string_args(&html[abs..]) {
            let next = base.as_str().replace(&old, &new);
            if next != base.as_str()
                && let Ok(url) = Url::parse(&next)
            {
                return Some(url);
            }
        }
        from += rel + 1;
    }
    None
}

fn location_literal(html: &str, base: &Url) -> Option<Url> {
    let lower = html.to_ascii_lowercase();
    for needle in [
        "location.replace(",
        "location.assign(",
        "location.href=",
        "location.href =",
    ] {
        let mut from = 0;
        while let Some(rel) = lower[from..].find(needle) {
            let abs = from + rel + needle.len();
            let rest = html[abs..].trim_start();
            if let Some((value, _)) = parse_one_string_arg(rest)
                && let Ok(url) = base.join(&value).or_else(|_| Url::parse(&value))
                && url != *base
            {
                return Some(url);
            }
            from += rel + 1;
        }
    }
    None
}

fn meta_refresh(html: &str, base: &Url) -> Option<Url> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("http-equiv") {
        let window = &html[from + rel..].get(..400).unwrap_or(&html[from + rel..]);
        let wlower = window.to_ascii_lowercase();
        if wlower.contains("refresh")
            && let Some(url) = refresh_url_from_tag(window, base)
        {
            return Some(url);
        }
        from += rel + 1;
    }
    None
}

fn refresh_url_from_tag(tag: &str, base: &Url) -> Option<Url> {
    let lower = tag.to_ascii_lowercase();
    let content_at = lower.find("content=")?;
    let rest = tag[content_at + "content=".len()..].trim_start();
    let (value, _) = if rest.starts_with('"') || rest.starts_with('\'') {
        parse_one_string_arg(rest)?
    } else {
        let end = rest.find([' ', '>', '/']).unwrap_or(rest.len());
        (rest[..end].to_string(), "")
    };
    let value = value.trim();
    let url_part = value
        .split_once(';')
        .map(|(_, r)| r)
        .unwrap_or(value)
        .trim();
    let url_part = url_part
        .split_once('=')
        .filter(|(k, _)| k.trim().eq_ignore_ascii_case("url"))
        .map(|(_, v)| v.trim())
        .unwrap_or(url_part)
        .trim_matches(['\'', '"']);
    if url_part.is_empty() {
        return None;
    }
    base.join(url_part).or_else(|_| Url::parse(url_part)).ok()
}

fn parse_two_string_args(s: &str) -> Option<(String, String)> {
    let (first, rest) = parse_one_string_arg(s.trim_start())?;
    let rest = rest.trim_start().strip_prefix(',')?.trim_start();
    let (second, _) = parse_one_string_arg(rest)?;
    Some((first, second))
}

fn parse_one_string_arg(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let quote = s.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                out.push(bytes[i + 1] as char);
                i += 2;
            }
            c if c == quote as u8 => {
                return Some((out, &s[i + 1..]));
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_examples_js_replace() {
        let html = r#"<!DOCTYPE html><html><body>
<script>url=window.location.href.replace('file-examples.com/wp-content/storage/','file-examples.com/storage/fe9ba84efb6a85f53a0f00c/'); window.location.replace(url);</script>
</body></html>"#;
        let base = Url::parse(
            "https://file-examples.com/wp-content/storage/2017/04/file_example_MP4_1920_18MG.mp4",
        )
        .unwrap();
        let next = extract_html_redirect(html, &base).unwrap();
        assert_eq!(
            next.as_str(),
            "https://file-examples.com/storage/fe9ba84efb6a85f53a0f00c/2017/04/file_example_MP4_1920_18MG.mp4"
        );
    }

    #[test]
    fn location_replace_literal() {
        let html = r#"<script>window.location.replace("https://cdn.example.com/a.bin");</script>"#;
        let base = Url::parse("https://example.com/download").unwrap();
        assert_eq!(
            extract_html_redirect(html, &base).unwrap().as_str(),
            "https://cdn.example.com/a.bin"
        );
    }

    #[test]
    fn meta_refresh_relative() {
        let html = r#"<meta http-equiv="refresh" content="3;url=/files/a.bin">"#;
        let base = Url::parse("https://example.com/get").unwrap();
        assert_eq!(
            extract_html_redirect(html, &base).unwrap().as_str(),
            "https://example.com/files/a.bin"
        );
    }

    #[test]
    fn mp4_url_is_not_html() {
        let url = Url::parse("https://example.com/video.mp4").unwrap();
        assert!(!url_looks_like_html(&url));
        assert!(is_html_content_type("text/html; charset=UTF-8"));
        assert!(looks_like_html_bytes(b"\n<!DOCTYPE html><html>"));
    }
}
