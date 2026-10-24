#![cfg(unix)]
//! Integration coverage for the interactive document reader.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use tempfile::TempDir;

#[test]
fn read_supports_search_settings_resize_and_restores_terminal_state() {
    let home = tempfile::tempdir().expect("create a temporary reader home");
    let mut lines = Vec::new();
    for index in 0..50 {
        let line = match index {
            19 => "target first needle".to_owned(),
            34 => "target second needle".to_owned(),
            _ => format!("document line {index}"),
        };
        lines.push(line);
    }
    let document = format!("{}\n", lines.join("\n"));
    let path = home.path().join("document.txt");
    fs::write(&path, document).expect("write reader fixture");

    let mut terminal = Terminal::start(home, &path, false, false);
    terminal.expect_text(b"line 1/51");
    terminal.send(b"/needle\r");
    terminal.expect_text(b"target first needle");
    terminal.send(b"n");
    terminal.expect_text(b"target second needle");
    terminal.send(b"N");
    terminal.expect_text(b"target first needle");

    terminal.send(b"+]M");
    terminal.expect_text(b"emphasis 55% | every 2 | minimum 3");
    terminal.send(b"p");
    terminal.expect_text(b"preset subtle");
    terminal.send(b"?");
    terminal.expect_text(b"Bionic Reader");
    terminal.send(b"?");

    terminal.resize(6, 12);
    terminal.expect_raw(b"\x1b[6;1H");
    terminal.send(b"G");
    terminal.expect_text(b"line 51/51");
    terminal.send(b"g");
    terminal.expect_text(b"line 1/51");
    terminal.send(b"q");

    let status = terminal.wait();
    assert_eq!(status, 0, "read should exit successfully");
    terminal.drain_output();
    assert!(
        terminal
            .output
            .windows(8)
            .any(|part| part == b"\x1b[?1049h")
    );
    assert!(
        terminal
            .output
            .windows(8)
            .any(|part| part == b"\x1b[?1049l")
    );
    assert!(terminal.output.windows(6).any(|part| part == b"\x1b[?25h"));
    assert!(
        strip_terminal_controls(&terminal.output)
            .windows(b"TERMIO_RESTORED".len())
            .any(|part| part == b"TERMIO_RESTORED"),
        "the reader should restore termios before returning to its shell; output: {:?}",
        String::from_utf8_lossy(&strip_terminal_controls(&terminal.output))
    );
}

#[test]
fn read_spools_pipe_input_and_keeps_the_terminal_for_keys() {
    let home = tempfile::tempdir().expect("create a temporary reader home");
    let path = home.path().join("pipe-source.txt");
    fs::write(&path, "Piped document content.\n").expect("write piped reader fixture");

    let mut terminal = Terminal::start(home, &path, true, false);
    terminal.expect_text(b"Piped document content.");
    terminal.send(b"q");
    assert_eq!(terminal.wait(), 0, "read should exit successfully");
    assert!(
        strip_terminal_controls(&terminal.output)
            .windows(b"TERMIO_RESTORED".len())
            .any(|part| part == b"TERMIO_RESTORED"),
        "the reader should restore termios after reading stdin"
    );
}

#[test]
fn read_quits_on_control_c_key_and_restores_the_terminal() {
    let home = tempfile::tempdir().expect("create a temporary reader home");
    let path = home.path().join("ctrl-c-source.txt");
    fs::write(&path, "Control-C document.\n").expect("write Ctrl-C reader fixture");

    let mut terminal = Terminal::start(home, &path, false, false);
    terminal.expect_text(b"Control-C document.");
    terminal.send(b"\x03");
    assert_eq!(terminal.wait(), 0, "Ctrl-C should exit successfully");
    assert!(has_terminal_restoration_marker(&terminal.output));
}

#[test]
fn read_rejects_dumb_terminals_with_a_plain_output_hint() {
    let home = tempfile::tempdir().expect("create a temporary reader home");
    let path = home.path().join("dumb-terminal-source.txt");
    fs::write(&path, "Dumb terminal document.\n").expect("write reader fixture");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .env("HOME", home.path())
        .env("TERM", "dumb")
        .env_remove("XDG_CONFIG_HOME")
        .args(["--style", "always", "read"])
        .arg(&path)
        .output()
        .expect("run the reader with a dumb terminal type");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("preview --print"),
        "the error should explain how to get plain output: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn read_restores_the_terminal_for_external_termination_signals() {
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let home = tempfile::tempdir().expect("create a temporary reader home");
        let path = home.path().join("signal-source.txt");
        fs::write(&path, "Signal-test document.\n").expect("write signal reader fixture");

        let mut terminal = Terminal::start(home, &path, false, true);
        terminal.expect_text(b"Signal-test document.");
        terminal.signal_reader(signal);
        assert_eq!(
            terminal.wait(),
            0,
            "signal handler should exit successfully"
        );
        assert!(
            has_terminal_restoration_marker(&terminal.output),
            "terminal restoration should survive signal {signal}"
        );
    }
}

#[test]
fn preview_print_uses_the_requested_passage() {
    let home = tempfile::tempdir().expect("create a temporary preview home");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .env("HOME", home.path())
        .env_remove("XDG_CONFIG_HOME")
        .args([
            "--format",
            "plain",
            "preview",
            "--text",
            "A small preview passage.",
            "--print",
        ])
        .output()
        .expect("run the preview print command");

    assert!(
        output.status.success(),
        "preview failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"A small preview passage.");
}

fn has_terminal_restoration_marker(output: &[u8]) -> bool {
    strip_terminal_controls(output)
        .windows(b"TERMIO_RESTORED".len())
        .any(|part| part == b"TERMIO_RESTORED")
}

struct Terminal {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Option<Box<dyn Write + Send>>,
    receiver: mpsc::Receiver<Vec<u8>>,
    reader_thread: Option<JoinHandle<()>>,
    output: Vec<u8>,
    consumed_text: usize,
    reader_pid_file: PathBuf,
    _home: TempDir,
}

impl Terminal {
    fn start(home: TempDir, path: &Path, stdin_document: bool, background_reader: bool) -> Self {
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: 8,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pseudo-terminal for the reader");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args([
            "-c",
            r#"
                before=$(stty -g < /dev/tty)
                if [ "$BIONIC_READER_TEST_BACKGROUND" = 1 ]; then
                    "$BIONIC_READER_TEST_BIN" --style always read "$BIONIC_READER_TEST_DOC" &
                    reader_pid=$!
                    printf '%s\n' "$reader_pid" > "$BIONIC_READER_TEST_PID_FILE"
                    wait "$reader_pid"
                    status=$?
                elif [ "$BIONIC_READER_TEST_STDIN" = 1 ]; then
                    cat "$BIONIC_READER_TEST_DOC" | "$BIONIC_READER_TEST_BIN" --style always read
                    status=$?
                else
                    "$BIONIC_READER_TEST_BIN" --style always read "$BIONIC_READER_TEST_DOC"
                    status=$?
                fi
                after=$(stty -g < /dev/tty)
                if [ "$before" = "$after" ]; then
                    printf '\nTERMIO_RESTORED\n'
                else
                    printf '\nTERMIO_NOT_RESTORED:%s:%s\n' "$before" "$after"
                fi
                exit "$status"
            "#,
        ]);
        command.env(
            "BIONIC_READER_TEST_BIN",
            env!("CARGO_BIN_EXE_bionic-reader"),
        );
        command.env(
            "BIONIC_READER_TEST_DOC",
            path.to_str().expect("temporary path is valid UTF-8"),
        );
        command.env(
            "BIONIC_READER_TEST_STDIN",
            if stdin_document { "1" } else { "0" },
        );
        let reader_pid_file = home.path().join("reader.pid");
        command.env(
            "BIONIC_READER_TEST_BACKGROUND",
            if background_reader { "1" } else { "0" },
        );
        command.env("BIONIC_READER_TEST_PID_FILE", &reader_pid_file);
        command.env("HOME", home.path());
        command.env("TERM", "xterm-256color");
        command.env_remove("XDG_CONFIG_HOME");
        for name in [
            "BIONIC_READER_DISABLE",
            "BIONIC_READER_SESSION",
            "BIONIC_READER_CONTROL_SOCKET",
            "BIONIC_READER_MARKER_TOKEN",
        ] {
            command.env_remove(name);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .expect("start the reader in the pseudo-terminal");
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .expect("clone the pseudo-terminal reader");
        let writer = pair
            .master
            .take_writer()
            .expect("take the pseudo-terminal writer");
        let (sender, receiver) = mpsc::channel();
        let reader_thread = thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
        });

        Self {
            child,
            master: pair.master,
            writer: Some(writer),
            receiver,
            reader_thread: Some(reader_thread),
            output: Vec::new(),
            consumed_text: 0,
            reader_pid_file,
            _home: home,
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer
            .as_mut()
            .expect("the pseudo-terminal writer remains open")
            .write_all(bytes)
            .expect("write keys to the reader terminal");
        self.writer
            .as_mut()
            .expect("the pseudo-terminal writer remains open")
            .flush()
            .expect("flush keys to the reader terminal");
    }

    fn resize(&self, rows: u16, columns: u16) {
        self.master
            .resize(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("resize the reader terminal");
    }

    fn signal_reader(&mut self, signal: libc::c_int) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            let pid = fs::read_to_string(&self.reader_pid_file)
                .ok()
                .and_then(|contents| contents.trim().parse::<libc::pid_t>().ok());
            if let Some(pid) = pid {
                break pid;
            }
            assert!(Instant::now() < deadline, "reader did not publish its PID");
            self.read_chunk();
        };
        // SAFETY: the child PID comes from the shell that launched the reader.
        let result = unsafe { libc::kill(pid, signal) };
        assert_eq!(result, 0, "send signal {signal} to reader process");
    }

    fn expect_text(&mut self, expected: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = strip_terminal_controls(&self.output);
            if let Some(index) = text[self.consumed_text..]
                .windows(expected.len())
                .position(|part| part == expected)
            {
                self.consumed_text += index + expected.len();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "reader did not render {:?}: {:?}",
                String::from_utf8_lossy(expected),
                String::from_utf8_lossy(&text)
            );
            self.read_chunk();
        }
    }

    fn expect_raw(&mut self, expected: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self
            .output
            .windows(expected.len())
            .any(|part| part == expected)
        {
            assert!(
                Instant::now() < deadline,
                "reader did not emit {expected:?}"
            );
            self.read_chunk();
        }
    }

    fn wait(&mut self) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().expect("poll reader child status") {
                self.drain_output();
                return status.exit_code();
            }
            assert!(Instant::now() < deadline, "reader child did not exit");
            self.read_chunk();
        }
    }

    fn read_chunk(&mut self) {
        if let Ok(chunk) = self.receiver.recv_timeout(Duration::from_millis(50)) {
            self.output.extend(chunk);
        }
    }

    fn drain_output(&mut self) {
        while let Ok(chunk) = self.receiver.recv_timeout(Duration::from_millis(50)) {
            self.output.extend(chunk);
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        drop(self.writer.take());
        if let Some(reader_thread) = self.reader_thread.take() {
            let _ = reader_thread.join();
        }
    }
}

fn strip_terminal_controls(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] != 0x1b || index + 1 >= input.len() {
            output.push(input[index]);
            index += 1;
            continue;
        }
        match input[index + 1] {
            b'[' => {
                index += 2;
                while index < input.len() {
                    let byte = input[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            b']' => {
                index += 2;
                while index < input.len() {
                    if input[index] == 0x07 {
                        index += 1;
                        break;
                    }
                    if input[index] == 0x1b && input.get(index + 1) == Some(&b'\\') {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            _ => index += 2,
        }
    }
    output
}
