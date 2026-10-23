use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn command(home: &std::path::Path) -> Command {
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
    assert!(output.status.success());
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
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"a \x1b[1mal\x1b[22mpha beta \x1b[1mga\x1b[22mmma"
    );
}

#[test]
fn default_configuration_is_loaded_from_xdg_home() {
    let home = tempfile::tempdir().unwrap();
    let config_home = home.path().join("custom-config");
    std::fs::create_dir_all(config_home.join("bionic-reader")).unwrap();
    std::fs::write(
        config_home.join("bionic-reader/config.toml"),
        "style = 'always'\nfraction = 1.0\n",
    )
    .unwrap();
    let output = command(home.path())
        .env("XDG_CONFIG_HOME", config_home)
        .args(["-t", "word"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\x1b[1mword\x1b[22m");
}

#[test]
fn empty_xdg_home_falls_back_to_the_home_configuration_directory() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".config/bionic-reader");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("config.toml"),
        "style = 'always'\nfraction = 1.0\n",
    )
    .unwrap();
    let output = command(home.path())
        .env("XDG_CONFIG_HOME", "")
        .args(["-t", "word"])
        .output()
        .unwrap();
    assert!(output.status.success());
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
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let generated = String::from_utf8(output.stdout).unwrap();
        assert!(generated.contains(expected));
        assert!(generated.contains("bionic-reader"));
    }
}

fn invoke(args: &[&str], input: &[u8]) -> Output {
    let home = tempfile::tempdir().unwrap();
    let mut child = command(home.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    output
}

#[test]
fn explicit_text_uses_default_prefix_emphasis() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .env("HOME", home.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["--style", "always", "-t", "reading"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\x1b[1mread\x1b[22ming");
}

#[test]
fn redirected_auto_output_preserves_input_bytes_and_formatting() {
    let input = b"\x1b[31mreading\x1b[0m\n\xff\0";
    let output = invoke(&[], input);
    assert!(output.status.success());
    assert_eq!(output.stdout, input);
}

#[test]
fn a_large_stream_preserves_malformed_bytes_and_line_endings() {
    let input = b"reading\xff \r\n".repeat(65_536);
    let output = invoke(&["--style", "always"], &input);
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"\x1b[1mread\x1b[22ming\xff \r\n".repeat(65_536)
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn a_closed_output_pipe_exits_without_a_panic() {
    let home = tempfile::tempdir().unwrap();
    let mut child = command(home.path())
        .args(["--style", "always"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Hold stdin open until the only reader of stdout is gone, so the write
    // reliably exercises EPIPE rather than racing with process startup.
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(b"reading ").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(141));
    assert!(output.stderr.is_empty());
}

#[test]
fn explicit_text_does_not_wait_for_stdin_eof() {
    let home = tempfile::tempdir().unwrap();
    let mut child = command(home.path())
        .args(["--style", "never", "-t", "selected"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("explicit text waited for stdin instead of taking precedence");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut actual = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, b"selected");
}

#[test]
fn empty_explicit_text_and_empty_stream_succeed_without_a_newline() {
    for args in [vec!["--style", "always", "-t", ""], vec![]] {
        let output = invoke(&args, b"");
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
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
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn command_line_preferences_override_an_explicit_toml_file() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("preferences.toml");
    std::fs::write(&config, "fraction = 1.0\nstyle = 'always'\n").unwrap();
    let output = command(home.path())
        .arg("--config")
        .arg(config)
        .args(["--fraction", "0.5", "-t", "reading"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\x1b[1mread\x1b[22ming");
}

#[test]
fn invalid_or_missing_explicit_configuration_is_not_silently_ignored() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("preferences.toml");
    for contents in [None, Some("unknown_option = true\n")] {
        if let Some(contents) = contents {
            std::fs::write(&config, contents).unwrap();
        }
        let output = command(home.path())
            .arg("--config")
            .arg(&config)
            .args(["-t", "reading"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn session_controls_outside_a_reading_session_fail_clearly() {
    for operation in ["on", "off", "toggle", "reload"] {
        let output = invoke(&[operation], b"");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("session"));
    }
}

#[test]
fn stalled_partial_word_is_flushed_and_its_remainder_stays_plain() {
    let home = tempfile::tempdir().unwrap();
    let mut child = command(home.path())
        .args(["--style", "always"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    stdin.write_all(b"wait").unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut prefix = [0_u8; 4];
        stdout.read_exact(&mut prefix).unwrap();
        sender.send((prefix, stdout)).unwrap();
    });
    let (prefix, mut stdout) = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(&prefix, b"wait");
    stdin.write_all(b"ing reading").unwrap();
    drop(stdin);
    let mut remainder = Vec::new();
    stdout.read_to_end(&mut remainder).unwrap();
    assert_eq!(remainder, b"ing \x1b[1mread\x1b[22ming");
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
}

#[test]
fn a_new_word_receives_its_own_flush_deadline_after_an_idle_period() {
    let home = tempfile::tempdir().unwrap();
    let mut child = command(home.path())
        .args(["--style", "always"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    stdin.write_all(b"hello ").unwrap();
    let mut prefix = [0_u8; b"\x1b[1mhel\x1b[22mlo ".len()];
    stdout.read_exact(&mut prefix).unwrap();
    assert_eq!(&prefix, b"\x1b[1mhel\x1b[22mlo ");
    std::thread::sleep(Duration::from_millis(95));
    stdin.write_all(b"read").unwrap();
    std::thread::sleep(Duration::from_millis(10));
    stdin.write_all(b"ing ").unwrap();
    drop(stdin);
    let mut remainder = Vec::new();
    stdout.read_to_end(&mut remainder).unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(remainder, b"\x1b[1mread\x1b[22ming ");
}
