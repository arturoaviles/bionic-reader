#![cfg(unix)]
//! End-to-end tests for the interactive terminal behavior of reading sessions.

use std::fs;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use tempfile::TempDir;

// SAFETY JUSTIFICATION: The terminal contract under test includes POSIX signal delivery. The
// standard library has no safe API to signal a child process, so the test uses its live child PID.

const PROMPT: &[u8] = b"TERMINAL_READY> ";

#[test]
fn prompts_are_immediate_and_resize_editing_multiline_and_eof_work() {
    let mut terminal = Terminal::start();
    terminal
        .master
        .resize(PtySize {
            rows: 37,
            cols: 113,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("resize the terminal under test");
    // Deliver the same resize notification a terminal emulator sends.
    let process_id = terminal
        .child
        .process_id()
        .expect("the terminal child should have a process ID");
    let result = {
        // SAFETY: `process_id` belongs to the live child process under test.
        unsafe { libc::kill(process_id as libc::pid_t, libc::SIGWINCH) }
    };
    assert_eq!(
        result, 0,
        "deliver the terminal resize signal to the test child"
    );
    // Signal forwarding and input forwarding run independently. Observe the
    // inner PTY's new dimensions before continuing with the editing checks.
    terminal.send(
        b"expected_rows=37; expected_columns=113; \
        for ((attempt = 0; attempt < 200; attempt++)); do \
        size=$(stty size); \
        if [[ $size == \"$expected_rows $expected_columns\" ]]; then \
        printf '%s\\n' \"$size\"; break; fi; sleep 0.01; done\n",
    );
    terminal.expect(b"37 113\r\n");
    terminal.expect(PROMPT);
    terminal.send(b"printf 'editingX");
    terminal.send(b"\x7f'\n");
    terminal.expect(b"editing");
    terminal.expect(PROMPT);
    terminal.send(b"printf '%s\\n' 'multi\nline'\n");
    terminal.expect(b"multi\r\nline\r\n");
    terminal.expect(PROMPT);
    terminal.send(b"\x04");
    assert_eq!(
        terminal.wait(),
        0,
        "the shell should exit successfully after receiving EOF"
    );
}

#[test]
fn stopped_background_and_foreground_jobs_keep_terminal_control() {
    let mut terminal = Terminal::start();
    terminal.send(b"printf 'JOB_STARTED\\n'; sleep 30\n");
    terminal.expect(b"JOB_STARTED\r\n");
    thread::sleep(Duration::from_millis(100));
    terminal.send(b"\x1a");
    terminal.expect(b"Stopped");
    terminal.expect(PROMPT);
    terminal.send(b"bg\n");
    terminal.expect(PROMPT);
    terminal.send(b"fg\n");
    terminal.expect(b"sleep 30");
    thread::sleep(Duration::from_millis(100));
    terminal.send(b"\x03");
    terminal.expect(PROMPT);
    terminal.send(b"printf 'status:%s\\n' \"$?\"\n");
    terminal.expect(b"status:130\r\n");
    terminal.expect(PROMPT);
    terminal.send(b"exit 7\n");
    assert_eq!(
        terminal.wait(),
        7,
        "the shell should preserve its requested exit status"
    );
}

struct Terminal {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<Vec<u8>>,
    plain: Vec<u8>,
    consumed: usize,
    _home: TempDir,
}

impl Terminal {
    fn start() -> Self {
        let home = tempfile::tempdir().expect("create a temporary home for the terminal test");
        fs::write(
            home.path().join(".bashrc"),
            "PS1='TERMINAL_READY> '\nPS2='CONTINUE> '\n",
        )
        .expect("write the terminal test shell configuration");
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pseudo-terminal for the terminal test");
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_bionic-reader"));
        command.args(["session", "--shell", "/bin/bash"]);
        command.env("HOME", home.path());
        command.env("XDG_CONFIG_HOME", home.path().join(".config"));
        command.env("BASH_SILENCE_DEPRECATION_WARNING", "1");
        command.env("TERM", "dumb");
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
            .expect("start the terminal session process");
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .expect("clone the terminal session reader");
        let writer = pair
            .master
            .take_writer()
            .expect("take the terminal session writer");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = [0; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut terminal = Self {
            child,
            master: pair.master,
            writer,
            receiver,
            plain: Vec::new(),
            consumed: 0,
            _home: home,
        };
        // No injected newline: this specifically verifies prompt flushing.
        terminal.expect(PROMPT);
        terminal
    }

    fn send(&mut self, input: &[u8]) {
        self.writer
            .write_all(input)
            .expect("write input to the terminal session");
        self.writer
            .flush()
            .expect("flush input to the terminal session");
    }

    fn expect(&mut self, expected: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(index) = self.plain[self.consumed..]
                .windows(expected.len())
                .position(|part| part == expected)
            {
                self.consumed += index + expected.len();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "terminal did not emit {:?}: {:?}",
                String::from_utf8_lossy(expected),
                String::from_utf8_lossy(&self.plain)
            );
            if let Ok(chunk) = self.receiver.recv_timeout(Duration::from_millis(50)) {
                self.plain.extend(chunk);
                self.plain = strip_sgr(&self.plain);
            }
        }
    }

    fn wait(&mut self) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .expect("polling the terminal session process should succeed")
            {
                return status.exit_code();
            }
            assert!(Instant::now() < deadline, "terminal child did not exit");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[must_use]
fn strip_sgr(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < input.len() {
        if input[index..].starts_with(b"\x1b[") {
            if let Some(end) = input[index + 2..]
                .iter()
                .position(|byte| (0x40..=0x7e).contains(byte))
            {
                if input[index + 2 + end] == b'm' {
                    index += 3 + end;
                    continue;
                }
            }
        }
        output.push(input[index]);
        index += 1;
    }
    output
}
