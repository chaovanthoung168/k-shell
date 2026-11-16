//! Native HTTP utility. Raw bodies always use the executor's supplied byte stream.
mod args;
mod response;

use crate::{error::ShellError, platform};
use args::{Args, View};
use reqwest::{header::HeaderMap, redirect::Policy};
use std::{
    fs::File,
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn valid_syntax(args: &[String]) -> bool {
    args::parse(args).is_ok()
}

pub(super) fn run(
    args: &[String],
    cwd: &Path,
    output: &mut dyn Write,
    format: super::OutputFormat,
) -> Result<(), ShellError> {
    let Some(args) = args::parse(args)? else {
        return output.write_all(args::HELP.as_bytes()).map_err(io_error);
    };
    let runtime = crate::runtime::network::runtime()?;
    let generation = platform::interrupt::generation();
    let mut stderr = io::stderr();
    runtime.block_on(interruptible(
        execute(&args, cwd, output, &mut stderr, format),
        || platform::interrupt::generation() != generation,
    ))
}

async fn interruptible(
    request: impl std::future::Future<Output = Result<(), ShellError>>,
    cancelled: impl Fn() -> bool,
) -> Result<(), ShellError> {
    let mut request = std::pin::pin!(request);
    loop {
        if cancelled() {
            return Err(ShellError::new("kreq: request cancelled", 130));
        }
        if let Ok(result) = tokio::time::timeout(Duration::from_millis(50), request.as_mut()).await
        {
            return result;
        }
    }
}

pub(super) fn sensitive(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
    ) || name.contains("api-key")
        || name.contains("apikey")
        || name.contains("token")
        || name.contains("secret")
}

/// Scrub credentials even if the server reflects them under another header name.
fn scrub(value: &str, headers: &HeaderMap) -> String {
    let mut text = value.to_owned();
    for (name, secret) in headers {
        if sensitive(name.as_str())
            && let Ok(secret) = secret.to_str()
        {
            if !secret.is_empty() {
                text = text.replace(secret, "[REDACTED]");
            }
            if let Some((_, token)) = secret.split_once(' ')
                && !token.is_empty()
            {
                text = text.replace(token, "[REDACTED]");
            }
        }
    }
    text.escape_debug().to_string()
}

fn log_headers(
    log: &mut dyn Write,
    prefix: &str,
    headers: &HeaderMap,
    secrets: &HeaderMap,
) -> Result<(), ShellError> {
    for (name, value) in headers {
        let value = if sensitive(name.as_str()) {
            "[REDACTED]".to_owned()
        } else {
            scrub(value.to_str().unwrap_or("[non-text value]"), secrets)
        };
        writeln!(log, "{prefix} {}: {value}", scrub(name.as_str(), secrets)).map_err(io_error)?;
    }
    Ok(())
}

fn io_error(error: io::Error) -> ShellError {
    ShellError::new(format!("kreq: I/O error: {error}"), 1)
}

fn http_error(error: reqwest::Error) -> ShellError {
    // reqwest errors may embed URLs or credentials. Report categories only.
    let description = if error.is_timeout() {
        "request timed out"
    } else if error.is_connect() {
        "connection failed (check DNS, server and TLS certificates)"
    } else if error.is_redirect() {
        "redirect refused (limit exceeded, invalid target, or credentials would cross origins)"
    } else if error.is_body() || error.is_decode() {
        "failed to read response body"
    } else {
        "HTTP request failed while sending or receiving headers"
    };
    ShellError::new(format!("kreq: {description}"), 1)
}

async fn execute(
    args: &Args,
    cwd: &Path,
    output: &mut dyn Write,
    log: &mut dyn Write,
    format: super::OutputFormat,
) -> Result<(), ShellError> {
    let start = Instant::now();
    let policy = if args.location {
        // reqwest strips standard credentials on cross-origin redirects; stop
        // custom API keys as well by rejecting cross-origin authenticated hops.
        let authenticated = args.headers.keys().any(|name| sensitive(name.as_str()));
        Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("redirect limit exceeded");
            }
            if authenticated
                && attempt
                    .previous()
                    .first()
                    .is_some_and(|first| first.origin() != attempt.url().origin())
            {
                return attempt.error("authenticated cross-origin redirect refused");
            }
            attempt.follow()
        })
    } else {
        Policy::none()
    };
    let client = reqwest::Client::builder()
        .redirect(policy)
        .timeout(args.timeout)
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .map_err(http_error)?;
    let mut builder = client
        .request(args.method.clone(), args.url.clone())
        .headers(args.headers.clone());
    if let Some(data) = &args.data {
        builder = builder.body(data.clone());
    }
    let request = builder.build().map_err(http_error)?;
    if args.verbose {
        let mut url = args.url.clone();
        url.set_query(None);
        url.set_fragment(None);
        writeln!(
            log,
            "> {} {}",
            args.method,
            scrub(url.as_str(), &args.headers)
        )
        .map_err(io_error)?;
        log_headers(log, ">", request.headers(), &args.headers)?;
    }
    let mut response = client.execute(request).await.map_err(http_error)?;
    let status = response.status();
    let summary = format.terminal_width.is_some() && !args.raw && args.output.is_none();
    if args.verbose {
        writeln!(log, "< {:?} {}", response.version(), status.as_u16()).map_err(io_error)?;
        log_headers(log, "<", response.headers(), &args.headers)?;
    }
    let mut included = Vec::new();
    if (args.include && !summary && args.view != View::Content) || args.view == View::Headers {
        write!(included, "{:?} {}\r\n", response.version(), status).map_err(io_error)?;
        for (name, value) in response.headers() {
            write!(included, "{name}: ").map_err(io_error)?;
            included.write_all(value.as_bytes()).map_err(io_error)?;
            included.write_all(b"\r\n").map_err(io_error)?;
        }
        included.write_all(b"\r\n").map_err(io_error)?;
    }
    if args.view == View::Headers {
        if let Some(mut file) = output_file(args, cwd)? {
            file.write_all(&included).map_err(io_error)?;
            file.flush().map_err(io_error)?;
        } else if summary {
            response::Summary {
                status,
                version: response.version(),
                headers: response.headers(),
                request_headers: &args.headers,
                body: &[],
                pretty: None,
                total_bytes: 0,
                view: View::Headers,
            }
            .write(output, format.terminal_width.unwrap_or(80))
            .map_err(io_error)?;
        } else {
            output.write_all(&included).map_err(io_error)?;
        }
        return finish(args, status, start, output, log);
    }
    // Terminal summaries retain a bounded preview. JSON parsing uses the whole
    // original body; raw transfers continue to stream without buffering.
    let formatted = if args.json || summary {
        let mut body = Vec::new();
        let mut total = 0;
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            total += chunk.len();
            let keep = if args.json {
                chunk.len()
            } else {
                chunk
                    .len()
                    .min(response::PREVIEW_LIMIT.saturating_sub(body.len()))
            };
            body.extend_from_slice(&chunk[..keep]);
        }
        let pretty = if args.json {
            let value: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
                ShellError::new(
                    format!(
                        "kreq: response is not valid JSON (line {}, column {})",
                        e.line(),
                        e.column()
                    ),
                    1,
                )
            })?;
            Some(
                serde_json::to_vec_pretty(&value)
                    .map_err(|_| ShellError::new("kreq: cannot format JSON", 1))?,
            )
        } else {
            None
        };
        Some((body, pretty, total))
    } else {
        None
    };
    let mut file = output_file(args, cwd)?;
    output.write_all(&included).map_err(io_error)?;
    if let Some((body, pretty, total)) = formatted {
        if let Some(file) = &mut file {
            file.write_all(&body).map_err(io_error)?;
        } else if summary {
            response::Summary {
                status,
                version: response.version(),
                headers: response.headers(),
                request_headers: &args.headers,
                body: &body,
                pretty: pretty.as_deref(),
                total_bytes: total,
                view: args.view,
            }
            .write(output, format.terminal_width.unwrap_or(80))
            .map_err(io_error)?;
        } else {
            output
                .write_all(pretty.as_deref().unwrap_or(&body))
                .map_err(io_error)?;
            output.write_all(b"\n").map_err(io_error)?;
        }
    } else {
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            if let Some(file) = &mut file {
                file.write_all(&chunk).map_err(io_error)?;
            } else {
                output.write_all(&chunk).map_err(io_error)?;
            }
        }
    }
    if let Some(file) = &mut file {
        file.flush().map_err(io_error)?;
    }
    finish(args, status, start, output, log)
}

fn output_file(args: &Args, cwd: &Path) -> Result<Option<File>, ShellError> {
    let Some(path) = &args.output else {
        return Ok(None);
    };
    let path = super::files::resolve("kreq", cwd, path)?;
    let file = platform::filesystem::open_text_target(&path, false).map_err(io_error)?;
    file.set_len(0).map_err(io_error)?;
    Ok(Some(file))
}

fn finish(
    args: &Args,
    status: reqwest::StatusCode,
    start: Instant,
    output: &mut dyn Write,
    log: &mut dyn Write,
) -> Result<(), ShellError> {
    output.flush().map_err(io_error)?;
    if args.verbose {
        writeln!(log, "* Elapsed: {:.3}s", start.elapsed().as_secs_f64()).map_err(io_error)?;
    }
    if args.fail && (status.is_client_error() || status.is_server_error()) {
        return Err(ShellError::new(
            format!("kreq: HTTP {} (--fail)", status.as_u16()),
            22,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn verbose_redacts_credentials_even_when_reflected_and_escapes_controls() {
        let args = args::parse(
            &[
                "get",
                "http://localhost",
                "-H",
                "Authorization: Bearer private-token",
                "-H",
                "X-API-Key: private-key",
            ]
            .map(str::to_owned),
        )
        .unwrap()
        .unwrap();
        let mut response = HeaderMap::new();
        response.insert(
            "x-reflection",
            "Bearer private-token / private-key".parse().unwrap(),
        );
        response.insert("set-cookie", "session=private-cookie".parse().unwrap());
        let mut log = Vec::new();
        log_headers(&mut log, "<", &response, &args.headers).unwrap();
        let log = String::from_utf8(log).unwrap();
        assert!(!log.contains("private-"));
        assert!(log.contains("[REDACTED]"));
        assert_eq!(scrub("\x1b[31m\r\n", &args.headers), "\\u{1b}[31m\\r\\n");
    }

    #[test]
    fn cancelling_pending_work_drops_the_future_and_returns_130() {
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            flag.store(true, Ordering::Relaxed);
        });
        let pending = async {
            let _guard = Guard(dropped.clone());
            std::future::pending::<Result<(), ShellError>>().await
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(interruptible(pending, || cancelled.load(Ordering::Relaxed)));
        worker.join().unwrap();
        assert_eq!(result.unwrap_err().status, 130);
        assert!(dropped.load(Ordering::Relaxed));
    }
}
