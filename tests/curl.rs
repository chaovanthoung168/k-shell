#![cfg(windows)]
use std::{
    fs,
    io::{self, Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use kshell::{
    interactive::completion::{complete, native_commands},
    platform,
};
mod common;
use common::TempDir;

#[test]
fn native_path_curl_has_precedence_and_preserves_arguments_cwd_and_status() {
    let temp = TempDir::new();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_kshell"), bin.join("curl.exe")).unwrap();
    let run = |input: &str| {
        Command::new(env!("CARGO_BIN_EXE_kshell"))
            .current_dir(temp.path())
            .env("PATH", &bin)
            .env("PATHEXT", ".EXE;.COM")
            .args(["-c", input])
            .output()
            .unwrap()
    };
    let output = run("curl -c 'echo \"ឯក file.txt\" \"https://example.invalid/?a=1&b=2\" \"\"'");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        output.stdout,
        "ឯក file.txt https://example.invalid/?a=1&b=2 \n".as_bytes()
    );
    assert_eq!(run("CURL.EXE -c 'exit 37'").status.code(), Some(37));
    let cwd = run("curl -c pwd");
    assert!(cwd.status.success());
    assert_eq!(
        std::path::Path::new(String::from_utf8(cwd.stdout).unwrap().trim())
            .canonicalize()
            .unwrap(),
        temp.path().canonicalize().unwrap()
    );
}

#[test]
fn system_curl_fallback_and_completion_work_without_path_and_explicit_paths_stay_explicit() {
    let temp = TempDir::new();
    if platform::curl::default_client().is_none() {
        eprintln!("Windows curl is not installed; skipping system curl check");
        return;
    }
    let run = |input: &str| {
        Command::new(env!("CARGO_BIN_EXE_kshell"))
            .current_dir(temp.path())
            .env("PATH", "")
            .env("PATHEXT", ".EXE;.COM")
            .args(["-c", input])
            .output()
            .unwrap()
    };
    for input in ["curl -q --version", "CURL.EXE -q --version"] {
        let output = run(input);
        assert!(output.status.success(), "{:?}", output.stderr);
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("curl "));
    }
    assert_eq!(
        run("curl -q --kshell-invalid-option").status.code(),
        Some(2)
    );
    assert_eq!(run("./missing/curl.exe --version").status.code(), Some(127));
    assert_eq!(run("./curl.cmd --version").status.code(), Some(126));
    assert_eq!(run("./curl.bat --version").status.code(), Some(126));
    let commands = native_commands(temp.path());
    assert!(commands.iter().any(|s| s == "curl"));
    assert!(commands.iter().any(|s| s == "curl.exe"));
    assert!(
        complete("cur", 3, temp.path(), &commands)
            .1
            .iter()
            .any(|p| p.replacement == "curl ")
    );
}

#[test]
fn real_curl_posts_quoted_json_and_downloads_to_a_unicode_path_using_only_loopback() {
    if platform::curl::default_client().is_none() {
        return;
    }
    let temp = TempDir::new();
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut connection = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) => return Err(e),
            }
        };
        connection.set_read_timeout(Some(Duration::from_secs(5)))?;
        connection.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut request = Vec::new();
        let mut end = None;
        let mut content_length = 0;
        loop {
            let mut chunk = [0u8; 1024];
            let count = connection.read(&mut chunk)?;
            if count == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            request.extend_from_slice(&chunk[..count]);
            if request.len() > 65_536 {
                return Err(io::Error::other("oversized fixture request"));
            }
            if end.is_none()
                && let Some(index) = request.windows(4).position(|s| s == b"\r\n\r\n")
            {
                end = Some(index + 4);
                let headers = String::from_utf8_lossy(&request[..index]);
                content_length = headers
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
                    .and_then(|(_, length)| length.trim().parse::<usize>().ok())
                    .unwrap_or(0);
            }
            if end.is_some_and(|header_end| request.len() >= header_end + content_length) {
                break;
            }
        }
        let response = b"{\"ok\":true}\n";
        write!(
            connection,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        )?;
        connection.write_all(response)?;
        Ok(request)
    });
    let input = format!(
        "curl -q --silent --show-error --noproxy '*' --max-time 3 -H 'Content-Type: application/json' --data-raw '{{\"message\":\"hello world\"}}' -o 'ឯក reply.json' 'http://{address}/api?first=1&second=2'"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_kshell"))
        .current_dir(temp.path())
        .env("PATH", "")
        .args(["-c", &input])
        .output()
        .unwrap();
    // Always join the bounded server before asserting, even on child failure.
    let request = server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = String::from_utf8(request.unwrap()).unwrap();
    assert!(request.starts_with("POST /api?first=1&second=2 HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("content-type: application/json\r\n")
    );
    assert!(request.ends_with("{\"message\":\"hello world\"}"));
    assert_eq!(
        fs::read(temp.path().join("ឯក reply.json")).unwrap(),
        b"{\"ok\":true}\n"
    );
    assert!(output.stdout.is_empty());
}
