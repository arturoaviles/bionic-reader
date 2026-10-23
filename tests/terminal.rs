#![cfg(unix)]

use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

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
        .unwrap();
    // Deliver the same resize notification a terminal emulator sends.
    unsafe {
        libc::kill(terminal.child.process_id().unwrap() as i32, libc::SIGWINCH);
    }
    terminal.send(b"stty size\n");
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
    assert_eq!(terminal.wait(), 0);
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
    assert_eq!(terminal.wait(), 7);
}

struct Terminal {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<Vec<u8>>,
    plain: Vec<u8>,
    consumed: usize,
    _home: tempfile::TempDir,
}

impl Terminal {
    fn start() -> Self {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join(".bashrc"),
            "PS1='TERMINAL_READY> '\nPS2='CONTINUE> '\n",
        )
        .unwrap();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
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
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();
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
        self.writer.write_all(input).unwrap();
        self.writer.flush().unwrap();
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
            if let Some(status) = self.child.try_wait().unwrap() {
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
