//! Human-readable terminal summaries; raw/file output never passes through here.
use reqwest::{StatusCode, Version, header::HeaderMap};
use std::io::{self, Write};
use unicode_width::UnicodeWidthChar;

pub(super) const PREVIEW_LIMIT: usize = 64 * 1024;
const LABEL_WIDTH: usize = 18;

fn safe_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .flat_map(|c| {
            if (c.is_control() && c != '\n')
                || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            {
                c.escape_default().to_string().chars().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn field(output: &mut dyn Write, name: &str, text: &str, width: usize) -> io::Result<()> {
    let available = width.saturating_sub(LABEL_WIDTH + 2).max(10);
    let mut first = true;
    for line in text.split('\n') {
        let mut part = String::new();
        let mut cells = 0;
        for c in line.chars() {
            let size = c.width().unwrap_or(0);
            if cells + size > available && !part.is_empty() {
                field_line(output, name, &part, first)?;
                first = false;
                part.clear();
                cells = 0;
            }
            part.push(c);
            cells += size;
        }
        field_line(output, name, &part, first)?;
        first = false;
    }
    Ok(())
}

fn field_line(output: &mut dyn Write, name: &str, text: &str, first: bool) -> io::Result<()> {
    if first {
        writeln!(output, "{name:<LABEL_WIDTH$}: {text}")
    } else {
        writeln!(output, "{:width$}{text}", "", width = LABEL_WIDTH + 2)
    }
}

pub(super) struct Summary<'a> {
    pub status: StatusCode,
    pub version: Version,
    pub headers: &'a HeaderMap,
    pub request_headers: &'a HeaderMap,
    pub body: &'a [u8],
    pub pretty: Option<&'a [u8]>,
    pub total_bytes: usize,
    pub view: super::args::View,
}

impl Summary<'_> {
    pub fn write(&self, output: &mut dyn Write, width: usize) -> io::Result<()> {
        let body_text = match std::str::from_utf8(self.body) {
            Ok(text) => Some(text),
            Err(error) if self.total_bytes > self.body.len() && error.error_len().is_none() => {
                // A bounded preview can end in the middle of a UTF-8 character.
                std::str::from_utf8(&self.body[..error.valid_up_to()]).ok()
            }
            Err(_) => None,
        };
        let content = if let Some(pretty) = self.pretty {
            // serde_json emits UTF-8; avoid a lossy conversion of response bytes.
            safe_text(std::str::from_utf8(pretty).map_err(io::Error::other)?)
        } else if let Some(text) = body_text
            && !text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{1b}'))
        {
            let mut preview = safe_text(text);
            if self.total_bytes > self.body.len() {
                preview.push_str("\n[Preview truncated; use --raw or -o for the complete body]");
            }
            preview
        } else {
            format!(
                "<binary response: {} bytes; use --raw or -o to read/save>",
                self.total_bytes
            )
        };
        let mut raw = format!("{:?} {}", self.version, self.status);
        let mut headers = Vec::new();
        for (name, value) in self.headers {
            let value = if super::sensitive(name.as_str()) {
                "[REDACTED]".to_owned()
            } else {
                super::scrub(
                    value.to_str().unwrap_or("[non-text value]"),
                    self.request_headers,
                )
            };
            raw.push_str(&format!("\n{name}: {value}"));
            headers.push(format!("[{name}, {value}]"));
        }
        // A bounded raw-body excerpt is independent of --json's Content view.
        raw.push_str("\n\n");
        if let Some(text) = body_text {
            let excerpt: String = text.chars().take(240).collect();
            raw.push_str(&safe_text(&excerpt));
            if self.total_bytes > excerpt.len() {
                raw.push_str("...");
            }
        } else {
            raw.push_str("<binary body>");
        }
        field(output, "StatusCode", &self.status.to_string(), width)?;
        if self.view != super::args::View::Headers {
            field(output, "Content", &content, width)?;
        }
        if self.view == super::args::View::All {
            field(output, "RawContent", &raw, width)?;
        }
        if self.view != super::args::View::Content {
            field(
                output,
                "Headers",
                &format!("{{{}}}", headers.join(", ")),
                width,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_is_compact_by_default_and_json_is_indented_under_its_label() {
        let headers = HeaderMap::new();
        let body = br#"{"id":1,"active":true}"#;
        let pretty =
            serde_json::to_vec_pretty(&serde_json::from_slice::<serde_json::Value>(body).unwrap())
                .unwrap();
        let mut summary = Summary {
            status: StatusCode::OK,
            version: Version::HTTP_11,
            headers: &headers,
            request_headers: &headers,
            body,
            pretty: None,
            total_bytes: body.len(),
            view: super::super::args::View::All,
        };
        let mut output = Vec::new();
        summary.write(&mut output, 120).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with(
            "StatusCode        : 200 OK\nContent           : {\"id\":1,\"active\":true}\n"
        ));
        assert!(!text.contains("StatusDescription"));
        summary.pretty = Some(&pretty);
        let mut output = Vec::new();
        summary.write(&mut output, 120).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("Content           : {\n                      \"active\": true,\n                      \"id\": 1\n                    }\nRawContent"));
        assert!(text.contains("{\"id\":1,\"active\":true}"));
        assert!(!text.contains("Forms"));
        for view in [
            super::super::args::View::Content,
            super::super::args::View::Headers,
        ] {
            summary.view = view;
            let mut output = Vec::new();
            summary.write(&mut output, 120).unwrap();
            let text = String::from_utf8(output).unwrap();
            assert!(text.starts_with("StatusCode        : 200 OK\n"));
            assert!(!text.contains("StatusDescription") && !text.contains("RawContent"));
            assert_eq!(
                text.contains("Content           :"),
                view == super::super::args::View::Content
            );
            assert_eq!(
                text.contains("Headers           :"),
                view == super::super::args::View::Headers
            );
        }
    }
    #[test]
    fn summary_handles_statuses_binary_empty_controls_width_and_sensitive_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", "private-cookie".parse().unwrap());
        let mut summary = Summary {
            status: StatusCode::NOT_FOUND,
            version: Version::HTTP_11,
            headers: &headers,
            request_headers: &headers,
            body: b"\xff\x00",
            pretty: None,
            total_bytes: 2,
            view: super::super::args::View::All,
        };
        let mut output = Vec::new();
        summary.write(&mut output, 120).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("StatusCode        : 404 Not Found"));
        assert!(text.contains("binary response: 2 bytes"));
        assert!(!text.contains("private-cookie"));
        summary.body = b"hello\x1b[31m\r\nworld";
        summary.total_bytes = summary.body.len();
        let mut output = Vec::new();
        summary.write(&mut output, 40).unwrap();
        assert!(!output.contains(&27));
        assert!(
            String::from_utf8(output).unwrap().lines().all(|line| line
                .chars()
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>()
                <= 40)
        );
        summary.body = b"";
        summary.total_bytes = 0;
        summary.status = StatusCode::NO_CONTENT;
        let mut output = Vec::new();
        summary.write(&mut output, 100).unwrap();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("Content           : \n")
        );
    }
}
