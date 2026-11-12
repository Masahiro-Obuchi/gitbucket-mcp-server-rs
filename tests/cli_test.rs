use serde_json::{json, Value};
use std::{
    path::Path,
    process::{Output, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const SECRET: &str = "p13-fake-secret-never-echo";
fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitbucket-mcp-server"));
    c.env_clear()
        .env("GITBUCKET_MCP_CONFIG_DIR", dir)
        .env("TOKIO_WORKER_THREADS", "2")
        .kill_on_drop(true);
    c
}
fn configured(dir: &Path, url: &str) -> Command {
    let mut c = command(dir);
    c.env("GITBUCKET_URL", url).env("GITBUCKET_TOKEN", SECRET);
    c
}
async fn output(mut c: Command) -> Output {
    tokio::time::timeout(Duration::from_secs(10), c.output())
        .await
        .expect("CLI command must terminate")
        .unwrap()
}
fn status(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains(SECRET));
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn help_and_version_ignore_configuration_logging_and_runtime_environment() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        format!("token = \"{SECRET}"),
    )
    .unwrap();
    for flag in ["--help", "-h", "--version", "-V"] {
        let mut c = command(dir.path());
        c.arg(flag)
            .env("GITBUCKET_URL", SECRET)
            .env("GITBUCKET_USERNAME", SECRET)
            .env("GITBUCKET_API_PROFILE", SECRET)
            .env("GITBUCKET_READ_ONLY", SECRET)
            .env("TOKIO_WORKER_THREADS", SECRET)
            .env("RUST_LOG", "trace");
        let out = output(c).await;
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(!stdout.contains(SECRET));
        if flag == "--help" || flag == "-h" {
            assert!(stdout.contains("--check-config") && stdout.contains("--check-connection"));
        } else {
            assert_eq!(
                stdout.trim(),
                format!("gitbucket-mcp-server {}", env!("CARGO_PKG_VERSION"))
            );
        }
    }
}

#[tokio::test]
async fn invalid_or_combined_commands_exit_two_without_echoing_arguments() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec![SECRET],
        vec!["--help", "--version"],
        vec!["--check-config", SECRET],
    ] {
        let mut c = command(dir.path());
        c.args(args);
        let out = output(c).await;
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains(SECRET));
    }
}

#[tokio::test]
async fn local_config_check_skips_unused_toml_and_never_calls_http_or_password_signin() {
    let http = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "broken TOML").unwrap();
    let mut c = configured(dir.path(), &http.uri());
    c.arg("--check-config")
        .env("GITBUCKET_API_PROFILE", "standard")
        .env("GITBUCKET_READ_ONLY", "true")
        .env("GITBUCKET_USERNAME", "alice")
        .env("GITBUCKET_PASSWORD", SECRET)
        .env("RUST_LOG", "trace");
    let out = output(c).await;
    let data = status(&out);
    assert_eq!(data["mode"], "configuration");
    assert_eq!(data["connection_checked"], false);
    assert_eq!(data["web_session_checked"], false);
    assert_eq!(data["api_profile"], "standard");
    assert_eq!(data["access_mode"], "read_only");
    assert_eq!(data["web_credentials_configured"], true);
    assert!(http.received_requests().await.unwrap().is_empty());
    assert!(!String::from_utf8_lossy(&out.stderr).contains(SECRET));
}

#[tokio::test]
async fn config_check_validates_all_startup_settings_without_network() {
    let http = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    for (key, value) in [
        ("GITBUCKET_USERNAME", SECRET),
        ("GITBUCKET_PASSWORD", SECRET),
        ("GITBUCKET_REQUEST_TIMEOUT_SECS", SECRET),
        ("GITBUCKET_API_PROFILE", SECRET),
        ("GITBUCKET_READ_ONLY", SECRET),
        ("GITBUCKET_URL", "ftp://user:secret@example.test"),
        ("GITBUCKET_TOKEN", "bad\nheader"),
    ] {
        let mut c = configured(dir.path(), &http.uri());
        c.arg("--check-config").env(key, value);
        let out = output(c).await;
        assert!(!out.status.success(), "{key}");
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains(SECRET));
        assert!(!stderr.contains("user:secret"));
    }
    assert!(http.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn explicit_connection_check_reads_only_pat_identity_and_does_not_print_it() {
    let http = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"login":"alice","email":SECRET})),
        )
        .expect(1)
        .mount(&http)
        .await;
    let mut c = configured(dir.path(), &http.uri());
    c.arg("--check-connection")
        .env("GITBUCKET_API_PROFILE", "standard")
        .env("GITBUCKET_USERNAME", "alice")
        .env("GITBUCKET_PASSWORD", SECRET)
        .env("RUST_LOG", "trace");
    let out = output(c).await;
    let data = status(&out);
    assert_eq!(data["connection_checked"], true);
    assert_eq!(data["web_session_checked"], false);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("alice"));
    assert!(!String::from_utf8_lossy(&out.stderr).contains(&http.uri()));
    let requests = http.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].headers["authorization"],
        format!("token {SECRET}")
    );
}

#[tokio::test]
async fn connection_errors_and_identity_mismatch_are_secret_free_and_never_submit_password() {
    for (code, body) in [
        (403, json!({"message":SECRET})),
        (200, json!({"login":"different-user"})),
        (200, json!({"login":" "})),
    ] {
        let http = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        Mock::given(method("GET"))
            .and(path("/api/v3/user"))
            .respond_with(ResponseTemplate::new(code).set_body_json(body))
            .expect(1)
            .mount(&http)
            .await;
        let mut c = configured(dir.path(), &http.uri());
        c.arg("--check-connection")
            .env("GITBUCKET_USERNAME", "alice")
            .env("GITBUCKET_PASSWORD", SECRET);
        let out = output(c).await;
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains(SECRET));
        assert!(!stderr.contains("different-user"));
        assert!(!stderr.contains(&http.uri()));
        assert!(http
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method == "GET"));
    }
}

#[tokio::test]
async fn partial_environment_requires_toml_and_rejects_unsupported_file_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    for text in [
        format!("token = \"{SECRET}"),
        format!("token = '{SECRET}'\nusername = '{SECRET}'"),
        String::new(),
    ] {
        std::fs::write(&file, text).unwrap();
        let mut c = command(dir.path());
        c.arg("--check-config")
            .env("GITBUCKET_URL", "https://example.test");
        let out = output(c).await;
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains(SECRET));
    }
    std::fs::write(&file, format!("token = '{SECRET}'")).unwrap();
    let mut c = command(dir.path());
    c.arg("--check-config")
        .env("GITBUCKET_URL", "https://example.test");
    assert_eq!(status(&output(c).await)["connection_checked"], false);
}

async fn spawn_stdio(
    mut command: Command,
) -> (
    Child,
    tokio::process::ChildStdin,
    BufReader<tokio::process::ChildStdout>,
    tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr.next_line().await {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (child, input, stdout, rx)
}
async fn line(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>, needle: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let line = rx.recv().await.expect("stderr ended");
            assert!(!line.contains(SECRET));
            if line.contains(needle) {
                break;
            }
        }
    })
    .await
    .unwrap();
}
async fn send(input: &mut tokio::process::ChildStdin, value: Value) {
    input
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
    input.flush().await.unwrap();
}
async fn response(stdout: &mut BufReader<tokio::process::ChildStdout>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(10), stdout.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).expect("stdout must contain JSON-RPC only")
}
async fn initialize(
    input: &mut tokio::process::ChildStdin,
    stdout: &mut BufReader<tokio::process::ChildStdout>,
) {
    send(input,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"startup-test","version":"1"}}})).await;
    assert_eq!(response(stdout).await["id"], 1);
}

#[tokio::test]
async fn stdio_readiness_waits_for_initialized_notification_and_stdout_stays_protocol_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "bad TOML").unwrap();
    let mut c = configured(dir.path(), "http://127.0.0.1:1");
    c.env("GITBUCKET_READ_ONLY", "true");
    let (mut child, mut input, mut stdout, mut stderr) = spawn_stdio(c).await;
    line(&mut stderr, "waiting for MCP initialization").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), stderr.recv())
            .await
            .is_err()
    );
    initialize(&mut input, &mut stdout).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), stderr.recv())
            .await
            .is_err()
    );
    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    line(&mut stderr, "ready (MCP initialized)").await;
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(
        response(&mut stdout).await["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        21
    );
    drop(input);
    assert!(tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap()
        .success());
}

#[tokio::test]
async fn malformed_initialization_does_not_echo_client_payload_or_report_ready() {
    let dir = tempfile::tempdir().unwrap();
    let (mut child, mut input, mut stdout, mut stderr) =
        spawn_stdio(configured(dir.path(), "http://127.0.0.1:1")).await;
    line(&mut stderr, "waiting for MCP initialization").await;
    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":99,"reason":SECRET}}),
    )
    .await;
    drop(input);
    assert!(!tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap()
        .success());
    let mut remaining = String::new();
    while let Some(line) = stderr.recv().await {
        remaining.push_str(&line);
    }
    assert!(!remaining.contains(SECRET));
    assert!(!remaining.contains("ready"));
    assert!(remaining.contains("MCP initialization failed"));
    let mut out = String::new();
    stdout.read_to_string(&mut out).await.unwrap();
    assert!(!out.contains(SECRET));
}

async fn tls_server() -> (
    String,
    Arc<Mutex<Vec<(String, String)>>>,
    tokio::task::JoinHandle<()>,
) {
    use tokio_rustls::{
        rustls::{
            pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
            ServerConfig,
        },
        TlsAcceptor,
    };
    let certs = CertificateDer::pem_slice_iter(include_bytes!("fixtures/tls/server.pem"))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/tls/server-key.pem")).unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap(),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            let calls = observed.clone();
            tokio::spawn(async move {
                let Ok(mut stream) = acceptor.accept(stream).await else {
                    return;
                };
                let mut data = Vec::new();
                let mut buf = [0u8; 2048];
                let end = loop {
                    let Ok(n) = stream.read(&mut buf).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&buf[..n]);
                    if let Some(i) = data.windows(4).position(|b| b == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let header = String::from_utf8_lossy(&data[..end]).into_owned();
                let mut parts = header.lines().next().unwrap().split_whitespace();
                let method = parts.next().unwrap();
                let path = parts.next().unwrap();
                let length = header
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    .unwrap_or(0);
                while data.len() < end + length {
                    let Ok(n) = stream.read(&mut buf).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&buf[..n]);
                }
                calls.lock().unwrap().push((method.into(), path.into()));
                let lower = header.to_ascii_lowercase();
                let (status,extra,body)=match (method,path) {
                ("GET","/api/v3/user") if lower.contains("authorization:")||lower.contains("cookie:")=>("200 OK","",json!({"login":"alice"}).to_string()),
                ("GET","/api/v3/user")=>("401 Unauthorized","","{}".into()),
                ("POST","/signin") if !lower.contains("authorization:")=>("303 See Other","Location: /\r\nSet-Cookie: JSESSIONID=test; Path=/; Secure\r\n",String::new()),
                ("GET","/")=>("200 OK","","<html>Authenticated landing</html>".into()),
                ("GET","/api/v3/repos/owner/repo/pulls/1")=>("200 OK","",json!({"number":1,"title":"PR","state":"open","head":{"ref":"feature","sha":"b".repeat(40)},"base":{"ref":"main","sha":"a".repeat(40)}}).to_string()),
                ("POST","/api/v3/repos/owner/repo/issues")=>("201 Created","",json!({"number":2,"title":"TLS","state":"open"}).to_string()),
                ("GET","/owner/repo/pull/1")=>("200 OK","","<span id='show-title'>PR</span><ul id='pullreq-tab'><li class='active'><a href='/owner/repo/pull/1'>Conversation<span class='badge'>0</span></a></li><li><a href='/owner/repo/pull/1/commits'>Commits</a></li><li><a href='/owner/repo/pull/1/files'>Files</a></li></ul><div id='comment-list'><div class='issue-comment-box'><div id='issueContent'>body</div></div></div>".into()),
                _=>("404 Not Found","","{}".into()),
            };
                let response=format!("HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (format!("https://{address}"), calls, task)
}

#[tokio::test]
async fn custom_ca_requires_trust_and_is_shared_by_diagnostics_rest_writes_and_web_sessions() {
    let (url, calls, task) = tls_server().await;
    let dir = tempfile::tempdir().unwrap();
    let mut untrusted = configured(dir.path(), &url);
    untrusted.arg("--check-connection");
    let out = output(untrusted).await;
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&out.stderr).contains(&url));
    assert!(calls.lock().unwrap().is_empty());
    let ca = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tls/ca.pem");
    let empty = dir.path().join("empty-roots");
    std::fs::create_dir(&empty).unwrap();
    let trust = |c: &mut Command| {
        c.env("SSL_CERT_FILE", &ca)
            .env("SSL_CERT_DIR", &empty)
            .env("GITBUCKET_API_PROFILE", "standard")
            .env("GITBUCKET_USERNAME", "alice")
            .env("GITBUCKET_PASSWORD", SECRET);
    };
    let mut check = configured(dir.path(), &url);
    check.arg("--check-connection");
    trust(&mut check);
    assert_eq!(status(&output(check).await)["connection_checked"], true);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[("GET".into(), "/api/v3/user".into())]
    );
    let mut c = configured(dir.path(), &url);
    trust(&mut c);
    let (mut child, mut input, mut stdout, mut stderr) = spawn_stdio(c).await;
    line(&mut stderr, "waiting for MCP initialization").await;
    initialize(&mut input, &mut stdout).await;
    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    line(&mut stderr, "ready").await;
    for (id, name, args) in [
        (
            2,
            "create_issue",
            json!({"owner":"owner","repo":"repo","title":"TLS"}),
        ),
        (
            3,
            "list_pull_request_review_comments",
            json!({"owner":"owner","repo":"repo","pull_number":1}),
        ),
    ] {
        send(&mut input,json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})).await;
        let result = response(&mut stdout).await;
        assert_eq!(result["result"]["isError"], false, "{result}");
    }
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|(m, p)| m == "POST" && p == "/signin"));
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|(m, p)| m == "POST" && p.ends_with("/issues")));
    drop(input);
    assert!(tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap()
        .success());
    task.abort();
}
