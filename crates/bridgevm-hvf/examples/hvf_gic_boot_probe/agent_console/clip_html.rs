//! Rich text between the Mac's `public.html` and Windows' "HTML Format"
//! (CF_HTML), with an agent that answered CLIPCAPS with CF_HTML.
//!
//! CF_HTML is UTF-8 behind a header of byte offsets:
//! `Version:0.9\r\nStartHTML:…\r\nEndHTML:…\r\nStartFragment:…\r\nEndFragment:…\r\n`,
//! where the fragment is what a paste inserts and the HTML around it carries
//! the context (Word's styles live in its `<head>`). The Mac side is a plain
//! HTML document. The agent moves the CF_HTML bytes unchanged both ways
//! (CLIPSETH, CLIPXH), so the digest of what the host sent is what comes back.

#[cfg(test)]
#[path = "clip_html_tests.rs"]
mod tests;

/// Largest HTML Format either way, in bytes (the agent's HtmlMaxBytes).
pub(crate) const CLIP_HTML_MAX_BYTES: usize = 4 << 20;

/// The HTML part of a CLIPXH reply: `<html-bytes> <b64(html)|->`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GuestHtml<'a> {
    None,
    /// Named by its size only: over the agent's limit.
    TooLarge(usize),
    /// The CF_HTML's base64, as on the wire.
    Present(&'a str),
}

impl<'a> GuestHtml<'a> {
    pub(super) fn b64(&self) -> Option<&'a str> {
        match self {
            Self::Present(b64) => Some(b64),
            _ => None,
        }
    }
}

/// `<html-bytes> <b64(html)|-> <b64(text)>`: the HTML part and the text's
/// base64, or None when malformed.
pub(super) fn parse_html_part(rest: &str) -> Option<(GuestHtml<'_>, &str)> {
    let mut fields = rest.splitn(3, ' ');
    let size = fields.next()?.parse::<usize>().ok()?;
    let body = fields.next()?;
    let text = fields.next()?;
    let html = match (size, body) {
        (0, _) => GuestHtml::None,
        (bytes, "-") => GuestHtml::TooLarge(bytes),
        (bytes, _) if bytes > CLIP_HTML_MAX_BYTES => GuestHtml::TooLarge(bytes),
        (_, b64) => GuestHtml::Present(b64),
    };
    Some((html, text))
}

const START_FRAGMENT: &str = "<!--StartFragment-->";
const END_FRAGMENT: &str = "<!--EndFragment-->";
/// The header below with 10-digit offsets: 13 + 22 + 20 + 26 + 24 bytes.
const HEADER_LEN: usize = 105;

/// The CF_HTML for a Mac `public.html` document. Its own fragment markers are
/// kept; else the body's content is the fragment; else the whole document,
/// wrapped in a body.
pub(crate) fn cf_html_from_mac(html: &str) -> Vec<u8> {
    let html = html.trim_start_matches('\u{feff}').replace('\0', "");
    // ASCII lowercase keeps every byte offset.
    let lower = html.to_ascii_lowercase();
    let marked = lower
        .find(&START_FRAGMENT.to_ascii_lowercase())
        .map(|start| start + START_FRAGMENT.len())
        .and_then(|start| Some((start, start + lower[start..].find(&END_FRAGMENT.to_ascii_lowercase())?)));
    let (doc, start, end) = if let Some((start, end)) = marked {
        (html, start, end)
    } else if let Some((open, close)) = body_span(&lower) {
        let doc = format!("{}{START_FRAGMENT}{}{END_FRAGMENT}{}", &html[..open], &html[open..close], &html[close..]);
        (doc, open + START_FRAGMENT.len(), close + START_FRAGMENT.len())
    } else {
        let head = format!("<html><body>\r\n{START_FRAGMENT}");
        let doc = format!("{head}{html}{END_FRAGMENT}\r\n</body>\r\n</html>");
        (doc, head.len(), head.len() + html.len())
    };
    let header = format!(
        "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\n",
        HEADER_LEN,
        HEADER_LEN + doc.len(),
        HEADER_LEN + start,
        HEADER_LEN + end
    );
    debug_assert_eq!(header.len(), HEADER_LEN);
    [header.into_bytes(), doc.into_bytes()].concat()
}

/// The content between `<body …>` and `</body`, as byte offsets.
fn body_span(lower: &str) -> Option<(usize, usize)> {
    let tag = lower.find("<body")?;
    let open = tag + lower[tag..].find('>')? + 1;
    let close = open + lower[open..].rfind("</body")?;
    Some((open, close))
}

/// The Mac `public.html` for a guest CF_HTML: the whole HTML part when its
/// offsets are usable, else the fragment. None when it is not UTF-8 or names
/// no usable range; the copy then goes as text alone.
pub(crate) fn mac_html_from_cf(cf: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(cf).ok()?;
    let header = &text[..text.find('<').unwrap_or(text.len())];
    let offset = |name: &str| -> Option<usize> {
        let value = header
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))?;
        // Version 1.0 allows -1 for "absent"; that, like garbage, is None.
        usize::try_from(value.trim().parse::<i64>().ok()?).ok()
    };
    let range = |start: Option<usize>, end: Option<usize>| {
        // Some writers count a trailing NUL the agent already cut.
        let (start, end) = (start?, end?.min(text.len()));
        text.get(start..end).filter(|doc| !doc.trim().is_empty())
    };
    let doc = range(offset("StartHTML"), offset("EndHTML"))
        .or_else(|| range(offset("StartFragment"), offset("EndFragment")))?;
    // Without it Cocoa decodes the (UTF-8) pasteboard HTML in a legacy encoding:
    // pasted as rich text, Korean came out as mojibake.
    Some(format!("<meta charset=\"utf-8\">{doc}"))
}
