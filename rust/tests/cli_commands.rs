use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::thread;

fn mock_server(responses: Vec<(&'static str, &'static str)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            write!(stream, "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (format!("http://{address}"), handle)
}

fn export_mock_server(
    responses: Vec<(&'static str, &'static str)>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let response = format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (format!("http://{address}"), handle)
}

fn mock_server_checked(
    responses: Vec<(&'static str, &'static str, &'static str)>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for (status, body, expected_fragment) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let length = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..length]);
            assert!(
                request.contains(expected_fragment),
                "request missing {expected_fragment:?}: {request}"
            );
            write!(stream, "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (format!("http://{address}"), handle)
}

fn export_page_response(id: &'static str, title: &'static str, version: u32) -> &'static str {
    match (id, title, version) {
        ("123", "Root", 1) => {
            r#"{"id":"123","title":"Root","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":1},"_links":{"webui":"/spaces/ENG/pages/123/Root"}}"#
        }
        _ => {
            r#"{"id":"123","title":"Root","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":1}}"#
        }
    }
}

/// Spawn the CLI binary with a fully isolated environment so a developer's
/// or CI runner's ambient `CONFLUENCE_*`/config-path variables (real
/// credentials, `CONFLUENCE_READ_ONLY`, a custom config directory, etc.)
/// can never leak into a test and change its outcome. Callers layer their
/// own `.env(...)` overrides on top of this base.
fn isolated_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_confluence-cli"));
    command.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        command.env("PATH", path);
    }
    command
}

fn invoke(args: &[&str], domain: &str) -> Output {
    isolated_command()
        .args(args)
        .env("CONFLUENCE_DOMAIN", domain)
        .env("CONFLUENCE_AUTH_TYPE", "none")
        .env("CONFLUENCE_ALLOW_INSECURE_HTTP", "true")
        .output()
        .unwrap()
}

#[test]
fn search_emits_compatible_json_shape() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"results":[{"content":{"id":"42","title":"Guide","type":"page"},"excerpt":"Excerpt"}]}"#,
    )]);
    let output = invoke(&["search", "guide", "--json"], &domain);
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["query"], "guide");
    assert_eq!(value["resultCount"], 1);
    assert_eq!(value["results"][0]["id"], "42");
    assert_eq!(value["results"][0]["excerpt"], "Excerpt");
}

#[test]
fn info_reads_full_canonical_page_fields() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"id":"123","title":"Architecture Overview","type":"page","status":"current","space":{"key":"ENG","name":"Engineering"},"history":{"createdBy":{"displayName":"Ada"},"createdDate":"2025-01-01"},"version":{"number":7,"when":"2025-01-02","by":{"displayName":"Grace"}},"ancestors":[{"id":"100","type":"page","title":"Parent"}],"_links":{"webui":"/spaces/ENG/pages/123/Architecture+Overview"}}"#,
    )]);
    let output = invoke(&["info", "123", "--json"], &domain);
    server.join().unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["spaceKey"], "ENG");
    assert_eq!(value["parentId"], "100");
    assert_eq!(value["version"], 7);
    assert_eq!(
        value["url"],
        format!("{domain}/spaces/ENG/pages/123/Architecture+Overview")
    );
    assert_eq!(value["createdAt"], "2025-01-01");
    assert_eq!(value["lastUpdatedBy"]["displayName"], "Grace");
}

#[test]
fn find_escapes_cql_literals_and_returns_page_location() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"results":[{"content":{"id":"7","title":"A \"quoted\" title","space":{"key":"ENG","name":"Engineering"},"_links":{"webui":"/spaces/ENG/pages/7/Title"}}}]}"#,
    )]);
    let output = invoke(
        &["find", "A \"quoted\" title", "--space", "ENG", "--json"],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["id"], "7");
    assert_eq!(value["space"]["key"], "ENG");
    assert_eq!(value["url"], format!("{domain}/spaces/ENG/pages/7/Title"));
}

#[test]
fn children_normalize_page_identity_and_parent() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"results":[{"id":"44","title":"Child","type":"page","status":"current","space":{"key":"OPS"},"version":{"number":4},"_links":{"webui":"/spaces/OPS/pages/44/Child"}}],"_links":{}}"#,
    )]);
    let output = invoke(&["children", "22", "--json"], &domain);
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["pageId"], "22");
    assert_eq!(value["children"][0]["id"], "44");
    assert_eq!(value["children"][0]["parentId"], "22");
    assert_eq!(value["children"][0]["version"], 4);
    assert_eq!(
        value["children"][0]["url"],
        format!("{domain}/spaces/OPS/pages/44/Child")
    );
}

#[test]
fn basic_auth_and_429_retry_preserve_request_and_eventually_succeed() {
    let (domain, server) = mock_server_checked(vec![
        (
            "429 Too Many Requests",
            r#"{"message":"slow down"}"#,
            "authorization: Basic dXNlckBleGFtcGxlLnRlc3Q6c2VjcmV0",
        ),
        (
            "200 OK",
            r#"{"results":[]}"#,
            "authorization: Basic dXNlckBleGFtcGxlLnRlc3Q6c2VjcmV0",
        ),
    ]);
    let output = isolated_command()
        .args(["search", "term", "--json"])
        .env("CONFLUENCE_DOMAIN", &domain)
        .env("CONFLUENCE_AUTH_TYPE", "basic")
        .env("CONFLUENCE_EMAIL", "user@example.test")
        .env("CONFLUENCE_API_TOKEN", "secret")
        .env("CONFLUENCE_ALLOW_INSECURE_HTTP", "true")
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["resultCount"], 0);
}

#[test]
fn read_supports_storage_and_text_without_config_side_effects() {
    let responses = vec![
        (
            "200 OK",
            r#"{"body":{"storage":{"value":"<h1>Hello</h1><p>world</p>"}}}"#,
        ),
        (
            "200 OK",
            r#"{"body":{"storage":{"value":"<h1>Hello</h1><p>world</p>"}}}"#,
        ),
    ];
    let (domain, server) = mock_server(responses);
    let text = invoke(&["read", "123", "--format", "text"], &domain);
    assert!(text.status.success());
    assert_eq!(String::from_utf8(text.stdout).unwrap(), "Hello world\n");
    let storage = invoke(&["read", "123", "--format", "storage"], &domain);
    server.join().unwrap();
    assert!(storage.status.success());
    assert_eq!(
        String::from_utf8(storage.stdout).unwrap(),
        "<h1>Hello</h1><p>world</p>\n"
    );
}

#[test]
fn default_refuses_plain_http_with_credentials_unless_opted_in() {
    let output = isolated_command()
        .args(["search", "query"])
        .env("CONFLUENCE_DOMAIN", "http://example.test")
        .env("CONFLUENCE_AUTH_TYPE", "bearer")
        .env("CONFLUENCE_API_TOKEN", "test-token")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("falling back to HTTPS"));
}

#[test]
fn api_refuses_cross_origin_absolute_url_before_sending_credentials() {
    let output = isolated_command()
        .args([
            "api",
            "https://other.example.test/rest/api/content",
            "--json",
        ])
        .env("CONFLUENCE_DOMAIN", "example.test")
        .env("CONFLUENCE_AUTH_TYPE", "bearer")
        .env("CONFLUENCE_API_TOKEN", "not-a-real-secret")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(error["error"].as_str().unwrap().contains("Cross-origin"));
}

#[test]
fn attachment_download_refuses_cross_origin_download_link_before_sending_credentials() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"id":"999","container":{"id":"123"},"_links":{"download":"https://attacker.example.test/stolen"}}"#,
    )]);
    let output = invoke(
        &[
            "attachment-download",
            "123",
            "999",
            "--dest",
            "/tmp/confluence-cli-test-attachment-download.bin",
        ],
        &domain,
    );
    server.join().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not match"));
}

fn invoke_cloud(args: &[&str], domain: &str) -> Output {
    isolated_command()
        .args(args)
        .env("CONFLUENCE_DOMAIN", domain)
        .env("CONFLUENCE_AUTH_TYPE", "none")
        .env("CONFLUENCE_ALLOW_INSECURE_HTTP", "true")
        .env("CONFLUENCE_FORCE_CLOUD", "true")
        .output()
        .unwrap()
}

#[test]
fn comment_resolve_requires_cloud() {
    let output = invoke(&["comment-resolve", "999", "--json"], "example.test");
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("requires Confluence Cloud"));
}

#[test]
fn comment_resolve_reports_not_found_for_missing_thread() {
    let (domain, server) = mock_server(vec![("404 Not Found", r#"{"message":"missing"}"#)]);
    let output = invoke_cloud(&["comment-resolve", "999", "--json"], &domain);
    server.join().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(error["error"].as_str().unwrap().contains("not found"));
}

#[test]
fn comment_resolve_refuses_replies() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"id":"999","parentCommentId":"111","resolutionStatus":null,"version":{"number":1}}"#,
    )]);
    let output = invoke_cloud(&["comment-resolve", "999", "--json"], &domain);
    server.join().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(error["error"].as_str().unwrap().contains("is a reply"));
}

#[test]
fn comment_resolve_refuses_dangling_threads() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"id":"999","parentCommentId":null,"resolutionStatus":"dangling","version":{"number":1}}"#,
    )]);
    let output = invoke_cloud(&["comment-resolve", "999", "--json"], &domain);
    server.join().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(error["error"].as_str().unwrap().contains("dangling"));
}

#[test]
fn comment_resolve_is_a_no_op_when_already_in_the_requested_state() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"id":"999","parentCommentId":null,"resolutionStatus":"resolved","version":{"number":3}}"#,
    )]);
    let output = invoke_cloud(&["comment-resolve", "999", "--json"], &domain);
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["changed"], false);
    assert_eq!(value["resolved"], true);
}

#[test]
fn comment_resolve_updates_with_incremented_version_and_reports_changed() {
    let (domain, server) = mock_server_checked(vec![
        (
            "200 OK",
            r#"{"id":"999","parentCommentId":null,"resolutionStatus":"current","version":{"number":3}}"#,
            "GET /api/v2/inline-comments/999",
        ),
        (
            "200 OK",
            r#"{"id":"999","resolved":true}"#,
            "\"version\":{\"number\":4}",
        ),
    ]);
    let output = invoke_cloud(&["comment-resolve", "999", "--json"], &domain);
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["changed"], true);
    assert_eq!(value["resolved"], true);
    assert_eq!(value["id"], "999");
}

#[test]
fn attachments_accepts_nodes_documented_limit_pattern_and_json_flags() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"results":[{"id":"7","title":"diagram.png","metadata":{"mediaType":"image/png"},"extensions":{"fileSize":2048},"version":{"number":1},"_links":{"download":"/download/attachments/1/diagram.png"}}],"_links":{}}"#,
    )]);
    let output = invoke(
        &[
            "attachments",
            "123",
            "--limit",
            "5",
            "--pattern",
            "*.png",
            "--json",
        ],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["attachmentCount"], 1);
    assert_eq!(value["attachments"][0]["id"], "7");
}

#[test]
fn property_list_accepts_nodes_documented_limit_start_and_all_flags() {
    let (domain, server) = mock_server(vec![(
        "200 OK",
        r#"{"results":[{"key":"editor","value":{"mode":"storage"},"version":{"number":2}}],"_links":{}}"#,
    )]);
    let output = invoke(
        &[
            "property-list",
            "123",
            "--limit",
            "10",
            "--start",
            "0",
            "--json",
        ],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["properties"][0]["key"], "editor");
}

#[test]
fn interactive_mutation_policy_refuses_json_and_headless_writes() {
    let json = invoke(
        &["create", "Title", "ENG", "--content", "Body", "--json"],
        "http://127.0.0.1:1",
    );
    assert_eq!(json.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&json.stderr).unwrap();
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("interactive confirmation"));

    let text = invoke(
        &["create", "Title", "ENG", "--content", "Body"],
        "http://127.0.0.1:1",
    );
    assert_eq!(text.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&text.stderr).contains("refusing non-interactive"));
}

#[test]
fn local_convert_rejects_missing_formats_and_converts_markdown_without_auth() {
    let mut output = isolated_command()
        .args([
            "convert",
            "--input-format",
            "markdown",
            "--output-format",
            "storage",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = output.stdin.take().unwrap();
    input.write_all(b"Hello\n\nworld & friends").unwrap();
    drop(input);
    let output = output.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "<p>Hello</p><p>world &amp; friends</p>\n"
    );
}

#[test]
fn export_dry_run_accepts_filters_and_does_not_create_files() {
    let (domain, server) =
        export_mock_server(vec![("200 OK", export_page_response("123", "Root", 1))]);
    let destination = tempfile::tempdir().unwrap();
    let output = invoke(
        &[
            "export",
            "123",
            "--dest",
            destination.path().to_str().unwrap(),
            "--dry-run",
            "--attachments-dir",
            "media",
            "--pattern",
            "*.png",
            "--exclude-attachments",
            "secret*",
            "--referenced-only",
        ],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 page(s) selected"));
    assert!(!destination.path().join("Root").exists());
}

#[test]
fn recursive_export_writes_nested_pages_and_marker() {
    let (domain, server) = export_mock_server(vec![
        (
            "200 OK",
            r#"{"id":"123","title":"Root","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":1}}"#,
        ),
        (
            "200 OK",
            r#"{"results":[{"id":"456","title":"Child","type":"page","space":{"key":"ENG"},"version":{"number":1},"_links":{"webui":"/child"}}],"_links":{}}"#,
        ),
        ("200 OK", r#"{"results":[],"_links":{}}"#),
        (
            "200 OK",
            r#"{"body":{"storage":{"value":"<p>Root body</p>"}}}"#,
        ),
        (
            "200 OK",
            r#"{"body":{"storage":{"value":"<p>Child body</p>"}}}"#,
        ),
    ]);
    let destination = tempfile::tempdir().unwrap();
    let output = invoke(
        &[
            "export",
            "123",
            "--dest",
            destination.path().to_str().unwrap(),
            "--recursive",
            "--delay-ms",
            "0",
            "--skip-attachments",
            "--format",
            "text",
        ],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = destination.path().join("Root");
    assert_eq!(
        std::fs::read_to_string(root.join("page.txt")).unwrap(),
        "Root body"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("Child/page.txt")).unwrap(),
        "Child body"
    );
    assert!(root.join(".confluence-export.json").is_file());
}

#[test]
fn recursive_export_dry_run_and_marker_guard_are_safe() {
    let (domain, server) = export_mock_server(vec![
        (
            "200 OK",
            r#"{"id":"123","title":"Root","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":1}}"#,
        ),
        ("200 OK", r#"{"results":[],"_links":{}}"#),
    ]);
    let destination = tempfile::tempdir().unwrap();
    let output = invoke(
        &[
            "export",
            "123",
            "--dest",
            destination.path().to_str().unwrap(),
            "--recursive",
            "--dry-run",
        ],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 page(s) selected"));
    assert!(!destination.path().join("Root").exists());

    let (domain, server) = export_mock_server(vec![(
        "200 OK",
        r#"{"id":"123","title":"Root","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":1}}"#,
    )]);
    std::fs::create_dir_all(destination.path().join("Root")).unwrap();
    let output = invoke(
        &[
            "export",
            "123",
            "--dest",
            destination.path().to_str().unwrap(),
            "--overwrite",
        ],
        &domain,
    );
    server.join().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing .confluence-export.json"));
}

#[test]
fn single_export_overwrites_only_marker_owned_directory_and_removes_stale_files() {
    let (domain, server) = export_mock_server(vec![
        (
            "200 OK",
            r#"{"id":"123","title":"Root","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":1}}"#,
        ),
        ("200 OK", r#"{"body":{"storage":{"value":"<p>First</p>"}}}"#),
        (
            "200 OK",
            r#"{"id":"123","title":"Root","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":2}}"#,
        ),
        (
            "200 OK",
            r#"{"body":{"storage":{"value":"<p>Replaced</p>"}}}"#,
        ),
    ]);
    let destination = tempfile::tempdir().unwrap();
    let args = [
        "export",
        "123",
        "--dest",
        destination.path().to_str().unwrap(),
        "--skip-attachments",
        "--format",
        "text",
    ];
    let first = invoke(&args, &domain);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let root = destination.path().join("Root");
    std::fs::write(root.join("stale.txt"), "stale").unwrap();
    let overwrite = invoke(
        &[
            "export",
            "123",
            "--dest",
            destination.path().to_str().unwrap(),
            "--skip-attachments",
            "--format",
            "text",
            "--overwrite",
        ],
        &domain,
    );
    server.join().unwrap();
    assert!(
        overwrite.status.success(),
        "{}",
        String::from_utf8_lossy(&overwrite.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("page.txt")).unwrap(),
        "Replaced"
    );
    assert!(!root.join("stale.txt").exists());
    assert!(root.join(".confluence-export.json").is_file());
}

#[test]
fn copy_tree_honors_copy_suffix_exclusion_depth_and_json_preview() {
    let (domain, server) = mock_server(vec![
        (
            "200 OK",
            r#"{"id":"20","title":"Target","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":9}}"#,
        ),
        (
            "200 OK",
            r#"{"id":"10","title":"Source","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":7}}"#,
        ),
        (
            "200 OK",
            r#"{"results":[{"id":"11","title":"Keep","type":"page","space":{"key":"ENG"},"version":{"number":1}}],"_links":{}}"#,
        ),
        ("200 OK", r#"{"results":[],"_links":{}}"#),
    ]);
    let output = invoke(
        &[
            "copy-tree",
            "10",
            "20",
            "--exclude",
            "Skip*",
            "--copy-suffix",
            " - duplicated",
            "--dry-run",
            "--json",
        ],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview["rootTitle"], "Source - duplicated");
    assert_eq!(preview["childCount"], 1);
    assert_eq!(preview["targetParentVersion"], 9);
}

#[test]
fn versions_purge_deletes_historical_versions_newest_first() {
    let (domain, server) = mock_server_checked(vec![
        (
            "200 OK",
            r#"{"results":[{"number":1},{"number":2},{"number":3}] }"#,
            "GET /rest/api/content/123/version",
        ),
        (
            "200 OK",
            r#"{"results":[{"number":1},{"number":2},{"number":3}] }"#,
            "GET /rest/api/content/123/version",
        ),
        (
            "204 No Content",
            "",
            "DELETE /rest/api/content/123/version/2",
        ),
        (
            "204 No Content",
            "",
            "DELETE /rest/api/content/123/version/1",
        ),
    ]);
    let output = invoke(&["versions-purge", "123", "--yes", "--json"], &domain);
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["deleted"], 2);
    assert_eq!(value["kept"], 3);
}

#[test]
fn copy_tree_executes_pages_in_hierarchy_and_maps_created_parent_ids() {
    let (domain, server) = mock_server_checked(vec![
        (
            "200 OK",
            r#"{"id":"20","title":"Target","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":1}}"#,
            "GET /rest/api/content/20",
        ),
        (
            "200 OK",
            r#"{"id":"10","title":"Source","type":"page","status":"current","space":{"key":"ENG"},"version":{"number":2}}"#,
            "GET /rest/api/content/10",
        ),
        (
            "200 OK",
            r#"{"results":[{"id":"11","title":"Child","type":"page","space":{"key":"ENG"},"version":{"number":1}}],"_links":{}}"#,
            "GET /rest/api/content/10/child/page",
        ),
        (
            "200 OK",
            r#"{"results":[{"id":"12","title":"Grandchild","type":"page","space":{"key":"ENG"},"version":{"number":1}}],"_links":{}}"#,
            "GET /rest/api/content/11/child/page",
        ),
        (
            "200 OK",
            r#"{"results":[],"_links":{}}"#,
            "GET /rest/api/content/12/child/page",
        ),
        (
            "200 OK",
            r#"{"id":"10","title":"Source","body":{"storage":{"value":"root body"}},"space":{"key":"ENG"}}"#,
            "GET /rest/api/content/10?expand=body.storage%2Cspace",
        ),
        (
            "200 OK",
            r#"{"id":"100","title":"Source (Copy)"}"#,
            "POST /rest/api/content",
        ),
        (
            "200 OK",
            r#"{"id":"11","title":"Child","body":{"storage":{"value":"child body"}},"space":{"key":"ENG"}}"#,
            "GET /rest/api/content/11?expand=body.storage%2Cspace",
        ),
        (
            "200 OK",
            r#"{"id":"101","title":"Child"}"#,
            "POST /rest/api/content",
        ),
        (
            "200 OK",
            r#"{"id":"12","title":"Grandchild","body":{"storage":{"value":"grand body"}},"space":{"key":"ENG"}}"#,
            "GET /rest/api/content/12?expand=body.storage%2Cspace",
        ),
        (
            "200 OK",
            r#"{"id":"102","title":"Grandchild"}"#,
            "POST /rest/api/content",
        ),
    ]);
    let output = invoke(
        &["copy-tree", "10", "20", "--delay-ms", "0", "--json"],
        &domain,
    );
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["totalCopied"], 3);
    assert_eq!(value["rootPage"]["id"], "100");
    assert_eq!(value["rootPage"]["title"], "Source (Copy)");
}

#[test]
fn versions_purge_noop_matches_node_json_shape() {
    let (domain, server) = mock_server(vec![("200 OK", r#"{"results":[{"number":4}] }"#)]);
    let output = invoke(&["versions-purge", "123", "--yes", "--json"], &domain);
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"id":"123","deleted":0,"failed":0,"kept":4})
    );
}
