//! Integration tests for CLI settings, output, and stream behavior.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[must_use]
fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bionic-reader"));
    command
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BIONIC_READER_CONTROL_SOCKET")
        .env_remove("BIONIC_READER_SESSION")
        .env_remove("BIONIC_READER_MARKER_TOKEN");
    command
}

#[test]
fn latin_apostrophes_and_accents_form_complete_words() {
    let output = invoke(&["--style", "always", "-t", "we’re café a 中1"], b"");
    assert!(
        output.status.success(),
        "CLI failed to render accented text: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        "\x1b[1mwe\x1b[22m’re \x1b[1mca\x1b[22mfé a 中1".as_bytes()
    );
}

#[test]
fn every_nth_word_counts_only_eligible_words() {
    let output = invoke(
        &[
            "--style",
            "always",
            "--every",
            "2",
            "--fraction",
            "0.25",
            "-t",
            "a alpha beta gamma",
        ],
        b"",
    );
    assert!(
        output.status.success(),
        "CLI failed to apply the configured word frequency: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"a \x1b[1mal\x1b[22mpha beta \x1b[1mga\x1b[22mmma"
    );
}

#[test]
fn default_configuration_is_loaded_from_xdg_home() {
    let home = tempfile::tempdir().expect("create temporary home for XDG configuration test");
    let config_home = home.path().join("custom-config");
    fs::create_dir_all(config_home.join("bionic-reader"))
        .expect("create XDG application config directory");
    fs::write(
        config_home.join("bionic-reader/config.toml"),
        "style = 'always'\nfraction = 1.0\n",
    )
    .expect("write XDG configuration");
    let output = command(home.path())
        .env("XDG_CONFIG_HOME", config_home)
        .args(["-t", "word"])
        .output()
        .expect("run CLI with XDG configuration");
    assert!(
        output.status.success(),
        "CLI failed with XDG configuration: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"\x1b[1mword\x1b[22m");
}

#[test]
fn empty_xdg_home_falls_back_to_the_home_configuration_directory() {
    let home = tempfile::tempdir().expect("create temporary home for empty XDG test");
    let config = home.path().join(".config/bionic-reader");
    fs::create_dir_all(&config).expect("create fallback configuration directory");
    fs::write(
        config.join("config.toml"),
        "style = 'always'\nfraction = 1.0\n",
    )
    .expect("write fallback configuration");
    let output = command(home.path())
        .env("XDG_CONFIG_HOME", "")
        .args(["-t", "word"])
        .output()
        .expect("run CLI with empty XDG_CONFIG_HOME");
    assert!(
        output.status.success(),
        "CLI failed with an empty XDG_CONFIG_HOME: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"\x1b[1mword\x1b[22m");
}

#[test]
fn completion_generation_provides_each_supported_shell() {
    for (shell, expected) in [
        ("bash", "complete"),
        ("zsh", "#compdef"),
        ("fish", "complete -c"),
    ] {
        let output = invoke(&["completions", shell], b"");
        assert!(
            output.status.success(),
            "{shell} completion generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let generated =
            String::from_utf8(output.stdout).expect("completion output should be valid UTF-8");
        assert!(
            generated.contains(expected),
            "{shell} completion should contain {expected:?}"
        );
        assert!(generated.contains("bionic-reader"));
    }
}

#[must_use]
fn invoke(args: &[&str], input: &[u8]) -> Output {
    let home = tempfile::tempdir().expect("create temporary home for CLI invocation");
    let mut child = command(home.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn CLI process");
    let mut stdin = child.stdin.take().expect("CLI should have piped stdin");
    let input = input.to_vec();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().expect("collect CLI output");
    writer.join().expect("join CLI stdin writer");
    output
}

#[test]
fn explicit_text_uses_default_prefix_emphasis() {
    let home = tempfile::tempdir().expect("create temporary home for explicit text test");
    let output = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .env("HOME", home.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["--style", "always", "-t", "reading"])
        .output()
        .expect("run CLI with explicit text");
    assert!(
        output.status.success(),
        "CLI failed to render explicit text: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"\x1b[1mread\x1b[22ming");
}

#[test]
fn redirected_auto_output_preserves_input_bytes_and_formatting() {
    let input = b"\x1b[31mreading\x1b[0m\n\xff\0";
    let output = invoke(&[], input);
    assert!(
        output.status.success(),
        "CLI failed to preserve redirected input: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, input);
}

#[test]
fn a_large_stream_preserves_malformed_bytes_and_line_endings() {
    let input = b"reading\xff \r\n".repeat(65_536);
    let output = invoke(&["--style", "always"], &input);
    assert!(
        output.status.success(),
        "CLI failed to process the large stream: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"\x1b[1mread\x1b[22ming\xff \r\n".repeat(65_536)
    );
    assert!(
        output.stderr.is_empty(),
        "large stream should not report an error"
    );
}

#[test]
fn a_closed_output_pipe_exits_without_a_panic() {
    let home = tempfile::tempdir().expect("create temporary home for closed pipe test");
    let mut child = command(home.path())
        .args(["--style", "always"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn CLI for closed pipe test");
    // Hold stdin open until the only reader of stdout is gone, so the write
    // reliably exercises EPIPE rather than racing with process startup.
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .expect("CLI should have piped stdin")
        .write_all(b"reading ")
        .expect("write input before closing the output pipe");
    let output = child
        .wait_with_output()
        .expect("collect closed pipe status");
    assert_eq!(
        output.status.code(),
        Some(141),
        "closed-pipe CLI invocation exited with status {:?}",
        output.status
    );
    assert!(
        output.stderr.is_empty(),
        "closed output pipe should exit quietly"
    );
}

#[test]
fn explicit_text_does_not_wait_for_stdin_eof() {
    let home = tempfile::tempdir().expect("create temporary home for explicit text EOF test");
    let mut child = command(home.path())
        .args(["--style", "never", "-t", "selected"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn CLI with piped stdin and stdout");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().expect("poll explicit text process") {
            assert!(
                status.success(),
                "explicit text process exited with {status:?}"
            );
            break;
        }
        if Instant::now() > deadline {
            child.kill().expect("stop CLI after stdin EOF deadline");
            panic!("explicit text waited for stdin instead of taking precedence");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut actual = Vec::new();
    child
        .stdout
        .take()
        .expect("CLI should have piped stdout")
        .read_to_end(&mut actual)
        .expect("read explicit text output");
    assert_eq!(actual, b"selected");
}

#[test]
fn empty_explicit_text_and_empty_stream_succeed_without_a_newline() {
    for args in [vec!["--style", "always", "-t", ""], vec![]] {
        let output = invoke(&args, b"");
        assert!(output.status.success(), "CLI failed for arguments {args:?}");
        assert!(
            output.stdout.is_empty(),
            "CLI emitted output for arguments {args:?}"
        );
    }
}

#[test]
fn invalid_settings_are_reported_with_a_failure_status() {
    for (flag, value) in [
        ("--fraction", "NaN"),
        ("--every", "0"),
        ("--min-word-length", "0"),
    ] {
        let output = invoke(&[flag, value, "-t", "reading"], b"");
        assert_eq!(
            output.status.code(),
            Some(2),
            "CLI should reject {flag} {value}"
        );
        assert!(
            !output.stderr.is_empty(),
            "{flag} rejection should explain the error"
        );
        assert!(
            output.stdout.is_empty(),
            "{flag} rejection should not produce output"
        );
    }
}

#[test]
fn command_line_preferences_override_an_explicit_toml_file() {
    let home = tempfile::tempdir().expect("create temporary home for settings precedence test");
    let config = home.path().join("preferences.toml");
    fs::write(&config, "fraction = 1.0\nstyle = 'always'\n")
        .expect("write explicit preferences file");
    let output = command(home.path())
        .arg("--config")
        .arg(config)
        .args(["--fraction", "0.5", "-t", "reading"])
        .output()
        .expect("run CLI with explicit preferences");
    assert!(
        output.status.success(),
        "CLI failed to process command line preferences: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"\x1b[1mread\x1b[22ming");
}

#[test]
fn invalid_or_missing_explicit_configuration_is_not_silently_ignored() {
    let home = tempfile::tempdir().expect("create temporary home for invalid config test");
    let config = home.path().join("preferences.toml");
    for contents in [None, Some("unknown_option = true\n")] {
        if let Some(contents) = contents {
            fs::write(&config, contents).expect("write invalid preferences file");
        }
        let output = command(home.path())
            .arg("--config")
            .arg(&config)
            .args(["-t", "reading"])
            .output()
            .expect("run CLI with explicit config path");
        assert_eq!(
            output.status.code(),
            Some(2),
            "CLI should reject config contents {contents:?}"
        );
        assert!(
            !output.stderr.is_empty(),
            "config rejection should explain the error for {contents:?}"
        );
    }
}

#[test]
fn session_controls_outside_a_reading_session_fail_clearly() {
    for operation in ["on", "off", "toggle", "reload"] {
        let output = invoke(&[operation], b"");
        assert!(
            !output.status.success(),
            "session control {operation:?} should fail outside a session"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("session"),
            "session control {operation:?} should explain that no session is active"
        );
    }
}

#[test]
fn stalled_partial_word_is_flushed_and_its_remainder_stays_plain() {
    let home = tempfile::tempdir().expect("create temporary home for stalled word test");
    let mut child = command(home.path())
        .args(["--style", "always"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn CLI for stalled word test");
    let mut stdin = child.stdin.take().expect("CLI should have piped stdin");
    let mut stdout = child.stdout.take().expect("CLI should have piped stdout");
    stdin
        .write_all(b"wait")
        .expect("write the partial word to the CLI");
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut prefix = [0_u8; 4];
        stdout
            .read_exact(&mut prefix)
            .expect("read the flushed partial word");
        sender
            .send((prefix, stdout))
            .expect("send the partial word to the test thread");
    });
    let (prefix, mut stdout) = receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("CLI should flush a stalled partial word");
    assert_eq!(&prefix, b"wait");
    stdin
        .write_all(b"ing reading")
        .expect("write the rest of the input stream");
    drop(stdin);
    let mut remainder = Vec::new();
    stdout
        .read_to_end(&mut remainder)
        .expect("read output after the stalled partial word");
    assert_eq!(remainder, b"ing \x1b[1mread\x1b[22ming");
    let status = child.wait().expect("wait for stalled word process");
    assert!(
        status.success(),
        "stalled word process exited with {status:?}"
    );
    reader.join().expect("join stalled word reader");
}

#[test]
fn a_new_word_after_an_idle_period_is_transformed() {
    let home = tempfile::tempdir().expect("create temporary home for idle word test");
    let mut child = command(home.path())
        .args(["--style", "always"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn CLI for idle word test");
    let mut stdin = child.stdin.take().expect("CLI should have piped stdin");
    let mut stdout = child.stdout.take().expect("CLI should have piped stdout");
    stdin
        .write_all(b"hello ")
        .expect("write first word to the CLI");
    let mut prefix = [0_u8; b"\x1b[1mhel\x1b[22mlo ".len()];
    stdout
        .read_exact(&mut prefix)
        .expect("read first word output");
    assert_eq!(&prefix, b"\x1b[1mhel\x1b[22mlo ");
    thread::sleep(Duration::from_millis(95));
    stdin
        .write_all(b"reading ")
        .expect("write word after idle period");
    drop(stdin);
    let mut remainder = Vec::new();
    stdout
        .read_to_end(&mut remainder)
        .expect("read output after idle period");
    let status = child.wait().expect("wait for idle word process");
    assert!(status.success(), "idle word process exited with {status:?}");
    assert_eq!(remainder, b"\x1b[1mread\x1b[22ming ");
}
