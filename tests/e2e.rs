//! End-to-end workflows through the actual executable, files, configuration, and pipes.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bionic-reader"));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .env_remove("BIONIC_READER_SESSION")
        .env_remove("BIONIC_READER_CONTROL_SOCKET");
    command
}

fn invoke(home: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = command(home)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start CLI workflow");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("feed CLI workflow");
    child.wait_with_output().expect("collect CLI workflow")
}

#[test]
fn redirected_ansi_file_can_be_read_back_as_exact_plain_source() {
    let home = tempfile::tempdir().expect("create workflow home");
    let source = "# Reading café cafe\u{301} niño 日本語 你好世界 👨‍👩‍👧‍👦\r\n\t  repeated spaces  \n";
    let styled = invoke(
        home.path(),
        &["--style", "always", "--emphasis", "underline"],
        source.as_bytes(),
    );
    assert!(styled.status.success(), "{:?}", styled.stderr);
    assert!(styled.stdout.windows(4).any(|bytes| bytes == b"\x1b[4m"));
    let saved = home.path().join("reading.ansi");
    fs::write(&saved, &styled.stdout).expect("save ANSI output");
    let plain = invoke(
        home.path(),
        &["--format", "plain"],
        &fs::read(saved).expect("read saved output"),
    );
    assert!(plain.status.success(), "{:?}", plain.stderr);
    assert_eq!(plain.stdout, source.as_bytes());
    assert!(plain.stderr.is_empty());
}

#[test]
fn markdown_export_reprocessing_is_repeat_safe_and_html_keeps_code_and_links() {
    let home = tempfile::tempdir().expect("create workflow home");
    let source = b"# Reading\n\n- [documentation](https://example.test) and `private code`.\n\n```rust\nlet value = 1;\n```\n";
    let args = ["--input-format", "markdown", "--format", "markdown"];
    let once = invoke(home.path(), &args, source);
    assert!(once.status.success(), "{:?}", once.stderr);
    let twice = invoke(home.path(), &args, &once.stdout);
    assert!(twice.status.success(), "{:?}", twice.stderr);
    assert_eq!(twice.stdout, once.stdout);
    let html = invoke(
        home.path(),
        &["--input-format", "markdown", "--format", "html"],
        &once.stdout,
    );
    assert!(html.status.success(), "{:?}", html.stderr);
    let html = String::from_utf8(html.stdout).expect("HTML export is UTF-8");
    assert!(html.contains("<h1>"), "{html}");
    assert!(html.contains("href=\"https://example.test\""), "{html}");
    assert!(html.contains("<code>private code</code>"), "{html}");
    assert!(html.contains("let value = 1;"), "{html}");
}

#[test]
fn configuration_preview_and_explicit_flags_use_the_same_settings() {
    let home = tempfile::tempdir().expect("create workflow home");
    let config = home.path().join("reading.toml");
    fs::write(&config, "profile='subtle'\nformat='html'\nemphasis='reverse'\nskip_code=true\nlow_latency=false\ncontrols='safe'\nlength_rules=[{max_length=8,fraction=1.0}]\n").expect("write full configuration");
    let path = config.to_str().expect("fixture path is UTF-8");
    let output = invoke(
        home.path(),
        &[
            "--config", path, "--every", "1", "preview", "--print", "--text", "reading",
        ],
        b"",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("<span class=\"bionic-reverse\">reading</span>")
    );
    let sample = invoke(
        home.path(),
        &["preview", "--print", "--format", "plain"],
        b"",
    );
    assert!(sample.status.success());
    assert!(String::from_utf8_lossy(&sample.stdout).contains("Café"));
}

#[test]
fn user_errors_keep_stdout_empty_and_explain_the_failure() {
    let home = tempfile::tempdir().expect("create workflow home");
    for args in [
        vec!["--length-rule", "bad", "-t", "reading"],
        vec!["--length-rule", "zero:0.5", "-t", "reading"],
        vec!["--length-rule", "4:bad", "-t", "reading"],
        vec!["--length-rule", "0:0.5", "-t", "reading"],
        vec!["--format", "html", "session"],
        vec!["-t", "reading", "read"],
        vec!["read", "/nonexistent/bionic-reader-document"],
        vec![
            "--input-format",
            "markdown",
            "--format",
            "markdown",
            "--skip-code=false",
            "-t",
            "`code`",
        ],
    ] {
        let output = invoke(home.path(), &args, b"");
        assert!(!output.status.success(), "arguments={args:?}");
        assert!(output.stdout.is_empty(), "arguments={args:?}");
        assert!(!output.stderr.is_empty(), "arguments={args:?}");
    }
    for format in ["html", "markdown"] {
        let output = invoke(home.path(), &["--format", format], b"\xff");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
    }
}
