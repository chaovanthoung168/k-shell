use std::time::Duration;

use reqwest::{
    Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};

use crate::error::ShellError;

pub(super) const HELP: &str = "K Shell HTTP Client

USAGE:
    kreq <METHOD> <URL> [OPTIONS] [content|headers]

RESPONSE VIEWS:
    content           Show StatusCode and Content only
    header, headers   Show StatusCode and response Headers only
                      StatusCode includes the description, e.g. 200 OK

METHODS:
    get, post, put, patch, delete, head, options (case-insensitive)

OPTIONS:
    -j, --json         Pretty-print JSON in Content (body only when redirected)
        --raw          Show only the response body, without a terminal summary
    -H, --header       Add request header: 'Name: value' (repeatable)
    -d, --data         Send request body exactly as supplied
    -o, --output       Save raw response body to file (overwrites)
    -i, --include      Include response headers on stdout
    -v, --verbose      Show HTTP details on stderr (credentials redacted)
    -L, --location     Follow redirects (up to 10)
    -X, --request      Override HTTP method
        --timeout      Total request timeout in seconds (default 30, max 86400)
        --fail         Fail on HTTP 4xx/5xx (body is still available)
    -h, --help         Show help

Options may appear before or after the URL. Quote JSON and URLs containing &.
Terminal output shows status, Content, RawContent preview and Headers.
--raw, file output and redirected output omit the summary.
Raw output has no added newline. --json never sets a request Content-Type.
With --json -o, JSON is validated but the file contains the original bytes.
Shell pipes/redirection/variable expansion are not implemented yet; use -o.
--json applies to content; headers skips body parsing and downloading.
Redirected content is body-only; redirected headers includes the HTTP status.
With headers -o, save the HTTP status and headers instead of the body.
";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum View {
    #[default]
    All,
    Content,
    Headers,
}

impl View {
    fn parse(value: &str) -> Result<Self, ShellError> {
        match value.to_ascii_lowercase().as_str() {
            "content" => Ok(Self::Content),
            "header" | "headers" => Ok(Self::Headers),
            _ => Err(syntax("response view must be content or headers")),
        }
    }
}

pub(super) struct Args {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub data: Option<String>,
    pub output: Option<String>,
    pub json: bool,
    pub raw: bool,
    pub include: bool,
    pub verbose: bool,
    pub location: bool,
    pub fail: bool,
    pub timeout: Duration,
    pub view: View,
}

fn syntax(message: &str) -> ShellError {
    ShellError::new(format!("kreq: {message}; see kreq --help"), 2)
}

fn method(value: &str) -> Result<Method, ShellError> {
    match value.to_ascii_uppercase().as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "PATCH" => Ok(Method::PATCH),
        "DELETE" => Ok(Method::DELETE),
        "HEAD" => Ok(Method::HEAD),
        "OPTIONS" => Ok(Method::OPTIONS),
        _ => Err(syntax(
            "expected GET, POST, PUT, PATCH, DELETE, HEAD or OPTIONS",
        )),
    }
}

pub(super) fn parse(args: &[String]) -> Result<Option<Args>, ShellError> {
    let mut headers = HeaderMap::new();
    let (mut data, mut output, mut override_method) = (None, None, None);
    let (mut json, mut include, mut verbose, mut location, mut fail) =
        (false, false, false, false, false);
    let mut timeout = Duration::from_secs(30);
    let mut raw = false;
    let mut positional = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            positional.extend(iter.map(String::as_str));
            break;
        }
        // Only long options accept =value; body/header contents remain untouched.
        let (flag, inline) = if arg.starts_with("--") {
            arg.split_once('=')
                .map_or((arg.as_str(), None), |(k, v)| (k, Some(v)))
        } else {
            (arg.as_str(), None)
        };
        let takes_value = matches!(
            flag,
            "-H" | "--header"
                | "-d"
                | "--data"
                | "-o"
                | "--output"
                | "-X"
                | "--request"
                | "--timeout"
        );
        if inline.is_some() && !takes_value {
            return Err(syntax("option does not accept a value"));
        }
        let value = if takes_value {
            Some(
                inline
                    .or_else(|| iter.next().map(String::as_str))
                    .ok_or_else(|| syntax("missing option value"))?,
            )
        } else {
            None
        };
        match flag {
            "-h" | "--help" => return Ok(None),
            "-j" | "--json" => json = true,
            "--raw" => raw = true,
            "-i" | "--include" => include = true,
            "-v" | "--verbose" => verbose = true,
            "-L" | "--location" => location = true,
            "--fail" => fail = true,
            "-H" | "--header" => {
                let (name, value) = value
                    .and_then(|v| v.split_once(':'))
                    .ok_or_else(|| syntax("header must have the form 'Name: value'"))?;
                let name = HeaderName::from_bytes(name.trim().as_bytes())
                    .map_err(|_| syntax("invalid header name"))?;
                let mut value = HeaderValue::from_str(value.trim())
                    .map_err(|_| syntax("invalid header value"))?;
                value.set_sensitive(super::sensitive(name.as_str()));
                headers.append(name, value);
            }
            "-d" | "--data" => data = value.map(str::to_owned),
            "-o" | "--output" => {
                let path = value
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| syntax("output path must not be empty"))?;
                output = Some(path.to_owned());
            }
            "-X" | "--request" => override_method = Some(method(value.unwrap_or_default())?),
            "--timeout" => {
                let seconds = value
                    .unwrap_or_default()
                    .parse::<f64>()
                    .map_err(|_| syntax("timeout must be positive seconds, at most 86400"))?;
                if !seconds.is_finite() || seconds <= 0.0 || seconds > 86400.0 {
                    return Err(syntax("timeout must be positive seconds, at most 86400"));
                }
                timeout =
                    Duration::try_from_secs_f64(seconds).map_err(|_| syntax("invalid timeout"))?;
                if timeout.is_zero() {
                    return Err(syntax("timeout must be at least one nanosecond"));
                }
            }
            _ if flag.starts_with('-') => return Err(syntax("unknown option")),
            _ => positional.push(arg.as_str()),
        }
    }
    let (base_method, url, view) = match positional.as_slice() {
        [verb, url, view] => (method(verb)?, *url, View::parse(view)?),
        [url, view] if override_method.is_some() && url.contains("://") => {
            (Method::GET, *url, View::parse(view)?)
        }
        [verb, url] => (method(verb)?, *url, View::All),
        [url] if override_method.is_some() => (Method::GET, *url, View::All),
        _ => {
            return Err(syntax(
                "usage: kreq <METHOD> <URL> [OPTIONS] [content|headers]",
            ));
        }
    };
    let url = Url::parse(url).map_err(|_| syntax("invalid HTTP URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(syntax("URL must use http:// or https://"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(syntax(
            "URL credentials are not supported; use an Authorization header",
        ));
    }
    Ok(Some(Args {
        method: override_method.unwrap_or(base_method),
        url,
        headers,
        data,
        output,
        json,
        raw,
        include,
        verbose,
        location,
        fail,
        timeout,
        view,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arguments(text: &[&str]) -> Result<Option<Args>, ShellError> {
        parse(&text.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }
    #[test]
    fn response_selectors_work_with_options_on_either_side_and_method_override() {
        for input in [
            vec!["get", "http://localhost", "--json", "content"],
            vec!["--json", "GET", "http://localhost", "content"],
            vec!["get", "http://localhost", "CONTENT", "-j"],
            vec!["-X", "GET", "HTTP://localhost", "content", "-j"],
        ] {
            let args = arguments(&input).unwrap().unwrap();
            assert_eq!(args.view, View::Content);
            assert!(args.json);
        }
        for view in ["header", "headers", "HEADERS"] {
            assert_eq!(
                arguments(&["get", "http://localhost", view])
                    .unwrap()
                    .unwrap()
                    .view,
                View::Headers
            );
        }
        assert!(arguments(&["get", "http://localhost", "unknown"]).is_err());
        assert!(arguments(&["get", "http://localhost", "content", "headers"]).is_err());
        assert_eq!(
            arguments(&["get", "http://localhost"])
                .unwrap()
                .unwrap()
                .view,
            View::All
        );
    }
    #[test]
    fn options_preserve_data_and_never_infer_content_type() {
        let args = arguments(&[
            "--json",
            "-H",
            "Authorization: Bearer SECRET",
            "pOsT",
            "https://example.invalid/?a=1&b=2",
            "--data={\"name\":\"A B\"}",
            "-H",
            "X-Test: 1",
            "--header=X-Test: 2",
            "-X",
            "PATCH",
            "--timeout=0.25",
            "-o",
            "C:\\My Files\\out.bin",
            "-i",
            "-v",
            "-L",
            "--fail",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(args.method, Method::PATCH);
        assert_eq!(args.data.as_deref(), Some("{\"name\":\"A B\"}"));
        assert!(!args.headers.contains_key("content-type"));
        assert!(args.headers["authorization"].is_sensitive());
        assert_eq!(args.headers.get_all("x-test").iter().count(), 2);
        assert_eq!(args.timeout, Duration::from_millis(250));
        assert_eq!(args.output.as_deref(), Some("C:\\My Files\\out.bin"));
        assert!(args.json && args.include && args.verbose && args.location && args.fail);
    }
    #[test]
    fn methods_defaults_help_and_invalid_input_are_pure_and_safe() {
        for verb in ["get", "POST", "put", "PaTcH", "delete", "head", "OPTIONS"] {
            let args = arguments(&[verb, "http://localhost"]).unwrap().unwrap();
            assert_eq!(args.method.as_str(), verb.to_ascii_uppercase());
            assert_eq!(args.timeout, Duration::from_secs(30));
            assert!(!(args.json || args.include || args.verbose || args.location || args.fail));
        }
        assert!(arguments(&["--help"]).unwrap().is_none());
        assert_eq!(
            arguments(&["-X", "DELETE", "http://localhost"])
                .unwrap()
                .unwrap()
                .method,
            Method::DELETE
        );
        for input in [
            vec![],
            vec!["get"],
            vec!["get", "file:///SECRET"],
            vec!["get", "http://localhost", "--timeout", "NaN"],
            vec!["get", "http://localhost", "--timeout", "0"],
            vec!["get", "http://localhost", "--timeout", "999999"],
            vec![
                "get",
                "http://localhost",
                "-H",
                "Authorization: SECRET\r\nx: y",
            ],
            vec!["get", "http://SECRET@localhost"],
            vec!["get", "http://localhost", "--SECRET"],
            vec!["get", "http://localhost", "-d"],
        ] {
            let error = arguments(&input).err().expect("invalid arguments");
            assert_eq!(error.status, 2);
            assert!(!error.message.contains("SECRET"));
        }
    }
}
