# kreq: native HTTP client

[Back to the README](../README.md#http-client-kreq)


`kreq` is a built-in HTTP client in this source tree. Build with `cargo build`
and launch `target/debug/kshell.exe` to use it; the existing v0.1.6 download
does not include this new command yet. No separate curl installation is required.
It uses reqwest with Rustls and native certificate roots, a current-thread Tokio
runtime shared with KPM, and serde_json for explicit JSON formatting.

Run each example separately inside K Shell:

```text
kreq get 'https://api.example.com/users/1'
kreq get 'https://api.example.com/users/1' --json
kreq GET 'https://api.example.com/users/1' -j
kreq post 'https://api.example.com/users' -H 'Content-Type: application/json' -d '{"name":"John","age":22}'
kreq post 'https://api.example.com/messages' -H 'Content-Type: text/plain' -d 'Hello World'
kreq put 'https://api.example.com/users/1' -d 'replacement'
kreq patch 'https://api.example.com/users/1' -H 'Content-Type: application/json' -d '{"active":true}'
kreq delete 'https://api.example.com/users/1'
kreq head 'https://api.example.com/users/1' -i
kreq options 'https://api.example.com/users' -i
kreq get 'https://api.example.com/profile' -H 'Authorization: Bearer TOKEN'
kreq get 'https://api.example.com/data' -H 'X-API-Key: TOKEN'
kreq get 'https://example.com/download' -L -o 'download.bin'
kreq get 'https://api.example.com/users' --timeout 5 --fail -v
kreq -X PATCH 'https://api.example.com/users/1' -d 'updated'
kreq --help
```

The example API addresses are placeholders; replace them with your own service.
Methods are case-insensitive; options work before or after the URL. `-X` can
override the positional method or supply it when only a URL is given.

| Option | Behavior |
| --- | --- |
| `-j`, `--json` | Parse original response bytes and pretty-print JSON; invalid or empty JSON fails. Objects, arrays and scalar values are supported. |
| `--raw` | Omit the interactive response summary and show only the body. With `--json`, show just formatted JSON. |
| `-H`, `--header` | Add a `Name: value` header; repeat for multiple headers. |
| `-d`, `--data` | Send the supplied text exactly, without adding a newline. |
| `-o`, `--output` | Save original body bytes, overwriting a regular file relative to the shell's current folder. Parent folder must exist. |
| `-i`, `--include` | Prepend final response headers on stdout; with `-o`, only headers go to stdout and the file contains just the body. |
| `-v`, `--verbose` | Send method, URL (without query/fragment), request/final-response headers, status and elapsed time to stderr. Sensitive header values are redacted. |
| `-L`, `--location` | Follow up to 10 HTTP/HTTPS redirects. Off by default. Cross-origin redirects with sensitive request headers are refused to protect credentials. |
| `-X`, `--request` | Override the method using one of the seven supported methods. |
| `--timeout` | Total network timeout in seconds; positive fractions allowed, default 30, maximum 86400. |
| `--fail` | Return status 22 for HTTP 4xx/5xx, while still writing the response body. |
| `-h`, `--help` | Show the complete usage reference. |

Interactive requests show a response summary with `StatusCode`,
`Content`, `RawContent`, and `Headers`. StatusCode combines the numeric code
and description, such as `200 OK` or `404 Not Found`. By default Content
shows the compact response text; `--json` indents JSON inside that field:

```text
StatusCode        : 200 OK
Content           : {
                      "active": true,
                      "id": 1,
                      "name": "John"
                    }
RawContent        : HTTP/1.1 200 OK
                    content-type: application/json

                    {"id":1,"name":"John","active":true}
Headers           : {[content-type, application/json]}
```

Select just content or response headers by adding a final response view.
Options may appear before or after that view; `header` aliases `headers`.
Both terminal views retain the combined StatusCode line:

```text
kreq get 'https://api.example.com/users/1' --json content
kreq get 'https://api.example.com/users/1' content
kreq get 'https://api.example.com/users/1' headers
kreq get 'https://api.example.com/users/1' header
```

Content view omits RawContent and Headers, including with `-i`. Headers view
omits Content and RawContent, skips downloading the body, and ignores `--json`
because no response-body parsing is needed. In raw/batch/redirected output,
content writes only the body; headers writes an HTTP status line and response
headers. With `headers -o headers.txt`, save that header block instead of the
body. `--fail` retains its normal status behavior in either view.

Use `--raw` for body-only terminal output, or `--raw --json` for body-only
formatted JSON. File output, redirected stdout, and batch/`-c` execution omit
the summary. Raw output preserves the exact response bytes, including binary
bytes, without added newlines or colors. JSON output stays plain for reliable
copying and external piping. `--json` changes only response
formatting: set request Content-Type explicitly with `-H`. With `--json -o`,
the body is validated as JSON and the file retains the original bytes; invalid
JSON leaves the destination untouched. Raw file transfers stream without
buffering the full body; explicit JSON formatting buffers it in memory.
Terminal text previews retain at most 64 KiB unless `--json` is requested;
binary bodies show a byte count. RawContent displays the normalized status and
headers plus at most 240 characters of the original body. It is a display
preview, not a wire capture. Summary headers redact credentials and terminal
controls are escaped. `-i` is redundant in a summary; in raw/batch/file modes
it retains its existing header-output behavior. Forms are not parsed.
Failed/interrupted transfers may leave partial raw files or output.

Successful HTTP transactions return 0, including 4xx/5xx unless `--fail` is used.
Usage errors return 2; network, TLS, timeout, JSON and file errors return 1.
Interactive Windows Ctrl+C cancels the transaction and returns 130. These
statuses update shell state and the exit code of `kshell -c`. Authentication
headers are forwarded as supplied. URL user/password credentials are rejected.
Verbose logs redact Authorization, API-key, token, secret and cookie headers,
including known request credentials reflected by response headers; raw body and
explicit `-i` header output remain server data. Avoid verbose mode for private
URLs. Requests do not load curl configuration or a persistent cookie jar.

The shell's existing single- and double-quote rules apply; single-quote JSON,
paths containing spaces, and URLs with `&`. K Shell does not yet implement
internal pipes, `>`/`>>` redirection, variable expansion, or conditional scripting.
Use `-o` inside K Shell; external host pipes/redirection preserve raw stdout.
HTTP tests use only private loopback fixtures, including OS pipes and file handles.
