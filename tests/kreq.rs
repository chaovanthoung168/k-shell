use kshell::runtime::ShellState;
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
mod common;
use common::TempDir;

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}
impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}
struct Server {
    url: String,
    requests: mpsc::Receiver<Request>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (send, requests) = mpsc::channel();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let Some(request) = read_request(&mut stream) else {
                            continue;
                        };
                        let method = request.method.clone();
                        let path = request.path.clone();
                        send.send(request).unwrap();
                        if path == "/slow" {
                            thread::sleep(Duration::from_millis(250));
                        }
                        let (status, extra, body): (&str, &str, &[u8]) = match path.as_str() {
                            "/json" => (
                                "200 OK",
                                "Content-Type: application/json\r\n",
                                br#"{"id":1,"name":"John","active":true}"#,
                            ),
                            "/array" => ("200 OK", "", b"[1,true,null]"),
                            "/string" => ("200 OK", "", br#""hello""#),
                            "/number" => ("200 OK", "", b"123456789012345678901234567890"),
                            "/bool" => ("200 OK", "", b"false"),
                            "/null" => ("200 OK", "", b"null"),
                            "/invalid" => ("200 OK", "", b"secret-invalid-json"),
                            "/empty" => ("204 No Content", "", b""),
                            "/binary" => (
                                "200 OK",
                                "Content-Type: application/octet-stream\r\n",
                                b"\x00\xff\xfe\r\n\x80binary\x00",
                            ),
                            "/notfound" => ("404 Not Found", "", b"missing"),
                            "/error" => ("500 Internal Server Error", "", b"failed"),
                            "/redirect" => ("302 Found", "Location: /json\r\n", b"redirecting"),
                            "/loop" => ("302 Found", "Location: /loop\r\n", b""),
                            "/credentials" => (
                                "200 OK",
                                "Set-Cookie: private-cookie\r\nX-Reflection: private-token\r\nX-API-Key: private-api-key\r\n",
                                b"ok",
                            ),
                            _ => ("200 OK", "Content-Type: text/plain\r\n", b"hello\r\nworld"),
                        };
                        let mut reply = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
                            body.len()
                        ).into_bytes();
                        if method != "HEAD" {
                            reply.extend_from_slice(body);
                        }
                        let sent = stream.write_all(&reply);
                        if path != "/slow" {
                            assert!(sent.is_ok(), "fixture response {path}: {sent:?}");
                        }
                        // Finish with a FIN and let the client close its side.
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        let mut remaining = Vec::new();
                        let _ = stream.read_to_end(&mut remaining);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("mock server: {e}"),
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    fn request(&self) -> Request {
        self.requests.recv_timeout(Duration::from_secs(5)).unwrap()
    }
    fn run(&self, arguments: &str) -> Output {
        shell(&arguments.replace("URL", &self.url))
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut byte = [0];
        if stream.read_exact(&mut byte).is_err() {
            return None;
        }
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break bytes.len();
        }
        if bytes.len() > 65536 {
            return None;
        }
    };
    let text = String::from_utf8(bytes).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_owned();
    let path = first.next()?.to_owned();
    let headers: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.to_owned(), v.trim().to_owned()))
        .collect();
    let length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(header_end + length < 1024 * 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}
fn shell(text: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", text])
        .output()
        .unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn all_methods_are_case_insensitive_and_raw_output_is_exact() {
    let server = Server::new();
    for method in ["get", "PoSt", "PUT", "patch", "delete", "HEAD", "options"] {
        let result = server.run(&format!("kreq {method} URL/json"));
        assert!(
            result.status.success(),
            "method {method}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        success(&result);
        assert!(result.stderr.is_empty());
        assert_eq!(server.request().method, method.to_ascii_uppercase());
        assert_eq!(
            result.stdout,
            if method == "HEAD" {
                b"".as_slice()
            } else {
                br#"{"id":1,"name":"John","active":true}"#
            }
        );
    }
}

#[test]
fn interactive_output_formats_content_and_raw_and_file_modes_omit_summary() {
    use kshell::commands::OutputFormat;
    let server = Server::new();
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_owned());
    let format = OutputFormat {
        terminal_width: Some(120),
        ..Default::default()
    };
    for (flags, pretty) in [("", false), ("--json -i", true)] {
        let mut output = Vec::new();
        state
            .execute_with_format(
                &format!("kreq get {}/json {flags}", server.url),
                &mut output,
                format,
            )
            .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with("StatusCode        : 200 OK\n"));
        assert!(!text.contains("StatusDescription"));
        assert!(text.contains("RawContent        : HTTP/1.1 200 OK"));
        assert!(text.contains("Headers           : {"));
        if pretty {
            assert!(
                text.contains("Content           : {\n                      \"active\": true,")
            );
        } else {
            assert!(
                text.contains("Content           : {\"id\":1,\"name\":\"John\",\"active\":true}")
            );
        }
    }
    let mut raw = Vec::new();
    state
        .execute_with_format(
            &format!("kreq get {}/binary --raw", server.url),
            &mut raw,
            format,
        )
        .unwrap();
    assert_eq!(raw, b"\x00\xff\xfe\r\n\x80binary\x00");
    let mut output = Vec::new();
    state
        .execute_with_format(
            &format!("kreq get {}/binary -o saved.bin", server.url),
            &mut output,
            format,
        )
        .unwrap();
    assert!(output.is_empty());
    assert_eq!(fs::read(temp.path().join("saved.bin")).unwrap(), raw);
}

#[test]
fn response_selectors_keep_combined_status_and_headers_does_not_parse_the_body() {
    use kshell::commands::OutputFormat;
    let server = Server::new();
    let temp = TempDir::new();
    let mut state = ShellState::new(temp.path().to_owned());
    let format = OutputFormat {
        terminal_width: Some(120),
        ..Default::default()
    };
    let mut content = Vec::new();
    state
        .execute_with_format(
            &format!("kreq get {}/json --json content", server.url),
            &mut content,
            format,
        )
        .unwrap();
    let content = String::from_utf8(content).unwrap();
    assert!(content.starts_with("StatusCode        : 200 OK\nContent           : {\n"));
    assert!(
        !content.contains("Headers")
            && !content.contains("RawContent")
            && !content.contains("StatusDescription")
    );
    let mut headers = Vec::new();
    state
        .execute_with_format(
            &format!("kreq get {}/invalid --json header", server.url),
            &mut headers,
            format,
        )
        .unwrap();
    let headers = String::from_utf8(headers).unwrap();
    assert!(headers.starts_with("StatusCode        : 200 OK\nHeaders           : {"));
    assert!(!headers.contains("Content") && !headers.contains("secret-invalid-json"));
    let mut raw = Vec::new();
    state
        .execute(
            &format!("kreq get {}/notfound headers --fail", server.url),
            &mut raw,
        )
        .unwrap_err();
    assert_eq!(state.last_status(), 22);
    assert!(raw.starts_with(b"HTTP/1.1 404 Not Found\r\n") && raw.ends_with(b"\r\n\r\n"));
    assert!(!raw.ends_with(b"missing"));
    let mut output = Vec::new();
    state
        .execute(
            &format!("kreq get {}/json headers -o headers.txt", server.url),
            &mut output,
        )
        .unwrap();
    assert!(output.is_empty());
    assert!(
        fs::read(temp.path().join("headers.txt"))
            .unwrap()
            .starts_with(b"HTTP/1.1 200 OK\r\n")
    );
    let mut body = Vec::new();
    state
        .execute(
            &format!("kreq get {}/json content -i", server.url),
            &mut body,
        )
        .unwrap();
    assert_eq!(body, br#"{"id":1,"name":"John","active":true}"#);
}

#[test]
fn bodies_authentication_repeat_headers_and_windows_quoting() {
    let server = Server::new();
    for (data, content_type) in [
        (
            r#"{"name":"John","path":"C:\Users\A B"}"#,
            "application/json",
        ),
        ("Hello World", "text/plain"),
    ] {
        let result = server.run(&format!("kreq -H 'Content-Type: {content_type}' post -H 'Authorization: Bearer private-token' URL/json -d '{data}' --header 'X-API-Key: private-api-key' -H 'X-Test: first' -H 'X-Test: second' --json"));
        success(&result);
        let request = server.request();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/json");
        assert_eq!(request.body, data.as_bytes());
        assert_eq!(request.header("Content-Type"), Some(content_type));
        assert_eq!(
            request.header("authorization"),
            Some("Bearer private-token")
        );
        assert_eq!(request.header("x-api-key"), Some("private-api-key"));
        assert_eq!(
            request
                .headers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("x-test"))
                .count(),
            2
        );
    }
    let result = server.run("kreq -X PATCH URL/json -d 'unchanged' -j");
    success(&result);
    let request = server.request();
    assert_eq!(request.method, "PATCH");
    assert!(request.header("content-type").is_none());
}

#[test]
fn explicit_json_formats_all_values_and_rejects_original_invalid_bytes() {
    let server = Server::new();
    for flag in ["--json", "-j"] {
        let result = server.run(&format!("kreq get URL/json {flag}"));
        success(&result);
        assert_eq!(
            result.stdout,
            b"{\n  \"active\": true,\n  \"id\": 1,\n  \"name\": \"John\"\n}\n"
        );
    }
    for path in ["array", "string", "number", "bool", "null"] {
        let result = server.run(&format!("kreq get URL/{path} -j"));
        success(&result);
        let _: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert!(!result.stdout.contains(&27));
        if path == "number" {
            assert_eq!(result.stdout, b"123456789012345678901234567890\n");
        }
    }
    for path in ["invalid", "empty", "text", "binary"] {
        let result = server.run(&format!("kreq get URL/{path} -j"));
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let message = String::from_utf8(result.stderr).unwrap();
        assert!(message.contains("not valid JSON"), "{path}: {message}");
        assert!(!message.contains("secret-invalid-json"));
    }
    let text = server.run("kreq get URL/text");
    success(&text);
    assert_eq!(text.stdout, b"hello\r\nworld");
    let empty = server.run("kreq get URL/empty");
    success(&empty);
    assert!(empty.stdout.is_empty());
}

#[test]
fn raw_files_streams_and_os_pipes_preserve_binary_bytes() {
    let server = Server::new();
    let temp = TempDir::new();
    let binary = b"\x00\xff\xfe\r\n\x80binary\x00";
    let mut state = ShellState::new(temp.path().to_owned());
    let mut out = Vec::new();
    state
        .execute(&format!("kreq get {}/binary", server.url), &mut out)
        .unwrap();
    assert_eq!(out, binary);
    let result = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .current_dir(temp.path())
        .args([
            "-c",
            &format!("kreq get {}/binary -o 'download bytes.bin'", server.url),
        ])
        .output()
        .unwrap();
    success(&result);
    assert!(result.stdout.is_empty());
    assert_eq!(
        fs::read(temp.path().join("download bytes.bin")).unwrap(),
        binary
    );
    let mut producer = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", &format!("kreq get {}/binary", server.url)])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let consumer = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", "cat"])
        .stdin(Stdio::from(producer.stdout.take().unwrap()))
        .output()
        .unwrap();
    success(&consumer);
    assert_eq!(consumer.stdout, binary);
    assert!(producer.wait().unwrap().success());
    let file = fs::File::create(temp.path().join("redirect.bin")).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .args(["-c", &format!("kreq get {}/binary", server.url)])
        .stdout(file)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read(temp.path().join("redirect.bin")).unwrap(), binary);
    let result = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .current_dir(temp.path())
        .args([
            "-c",
            &format!("kreq get {}/json -j -o raw.json", server.url),
        ])
        .output()
        .unwrap();
    success(&result);
    assert!(result.stdout.is_empty());
    assert_eq!(
        fs::read(temp.path().join("raw.json")).unwrap(),
        br#"{"id":1,"name":"John","active":true}"#
    );
    let result = server.run("kreq get URL/text -o 'missing-parent-kreq/file'");
    assert!(!result.status.success());
    let failed = state
        .execute(
            &format!("kreq get {}/notfound --fail", server.url),
            &mut Vec::new(),
        )
        .unwrap_err();
    assert_eq!(failed.status, 22);
    assert_eq!(state.last_status(), 22);
}

#[test]
fn http_errors_redirects_headers_and_sensitive_verbose_diagnostics() {
    let server = Server::new();
    for path in ["notfound", "error"] {
        let normal = server.run(&format!("kreq get URL/{path}"));
        success(&normal);
        let fail = server.run(&format!("kreq get URL/{path} --fail"));
        assert_eq!(fail.status.code(), Some(22));
        assert_eq!(fail.stdout, normal.stdout);
    }
    let redirect = server.run("kreq get URL/redirect");
    success(&redirect);
    assert_eq!(redirect.stdout, b"redirecting");
    let followed = server.run("kreq get -L URL/redirect");
    success(&followed);
    assert!(followed.stdout.starts_with(b"{\"id\""));
    let looped = server.run("kreq get URL/loop -L");
    assert!(!looped.status.success());
    assert!(looped.stdout.is_empty());
    let included = server.run("kreq get URL/text -i");
    success(&included);
    assert!(included.stdout.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(included.stdout.ends_with(b"\r\n\r\nhello\r\nworld"));
    let verbose = server.run("kreq get URL/credentials -v -H 'Authorization: Bearer private-token' -H 'X-API-Key: private-api-key'");
    success(&verbose);
    assert_eq!(verbose.stdout, b"ok");
    let log = String::from_utf8(verbose.stderr).unwrap();
    for secret in ["private-token", "private-api-key", "private-cookie"] {
        assert!(!log.contains(secret), "{log}");
    }
    for field in ["> GET", "< HTTP/1.1 200", "[REDACTED]", "Elapsed:"] {
        assert!(log.contains(field), "{log}");
    }
}

#[test]
fn timeout_connection_errors_invalid_options_and_help_have_correct_status() {
    let server = Server::new();
    let timeout = server.run("kreq get URL/slow --timeout=0.05");
    assert!(!timeout.status.success());
    assert!(
        String::from_utf8(timeout.stderr)
            .unwrap()
            .contains("timed out")
    );
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    let result = shell(&format!(
        "kreq get http://{address}/ --timeout 1 -H 'Authorization: Bearer private-token'"
    ));
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(
        !String::from_utf8(result.stderr)
            .unwrap()
            .contains("private-token")
    );
    for suffix in [
        "",
        "get",
        "get file:///test",
        "TRACE http://localhost",
        "get http://localhost --unknown",
        "get http://localhost -H 'Authorization: private-token\nBAD'",
        "get http://localhost --timeout NaN",
        "get http://localhost --timeout 0",
        "get http://localhost --timeout -1",
        "get http://localhost --timeout 999999999999",
        "get http://localhost -o",
        "get http://localhost --json=true",
        "get http://user:private-token@localhost",
    ] {
        let result = shell(&format!("kreq {suffix}"));
        assert_eq!(result.status.code(), Some(2), "{suffix}");
        assert!(result.stdout.is_empty());
        assert!(
            !String::from_utf8(result.stderr)
                .unwrap()
                .contains("private-token")
        );
    }
    for command in ["kreq -h", "kreq --help", "help kreq"] {
        let result = shell(command);
        success(&result);
        assert!(String::from_utf8(result.stdout).unwrap().contains("kreq"));
    }
    // These shell operators remain explicitly unsupported; do not bypass the parser.
    for command in ["kreq get URL/text | cat", "kreq get URL/text > file"] {
        let result = server.run(command);
        assert_eq!(result.status.code(), Some(2));
    }
}
