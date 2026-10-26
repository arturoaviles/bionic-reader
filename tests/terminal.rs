#![cfg(unix)]
//! End-to-end tests for the interactive terminal behavior of reading sessions.

use std::fs;
use std::io::{Read, Write};
use std::mem::MaybeUninit;
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

#[test]
fn interrupted_bypass_restores_transforming_and_preserves_signal_status() {
    let mut terminal = Terminal::start();
    terminal.send(
        b"\"$BIONIC_READER_BIN\" bypass -- /bin/sh -c 'printf \"BYPASS_STARTED\\n\"; exec sleep 30'\n",
    );
    terminal.expect(b"BYPASS_STARTED\r\n");
    terminal.send(b"\x03");
    terminal.expect(PROMPT);
    terminal.send(b"printf 'status:%s\\n' \"$?\"\n");
    terminal.expect(b"status:130\r\n");
    terminal.expect(PROMPT);

    let output_start = terminal.raw.len();
    terminal.send(b"printf 'reading\\n'\n");
    terminal.expect(b"reading\r\n");
    assert!(
        terminal.raw[output_start..]
            .windows(b"\x1b[1mread\x1b[22ming".len())
            .any(|window| window == b"\x1b[1mread\x1b[22ming"),
        "transformation should resume after interrupted bypass: {}",
        String::from_utf8_lossy(&terminal.raw[output_start..])
    );
    terminal.expect(PROMPT);
    terminal.send(b"exit 0\n");
    assert_eq!(
        terminal.wait(),
        0,
        "the shell should exit after the interrupted bypass test"
    );
}

#[test]
fn unsupported_shell_failure_restores_terminal_and_leaves_no_session_files() {
    let home = tempfile::tempdir().expect("create a temporary home for startup failure");
    let temp = tempfile::tempdir().expect("create a temporary session directory root");
    let result = StartupAttempt::start("powershell", home.path(), temp.path(), None).finish();

    assert_eq!(result.status, 1, "unsupported shell should return an error");
    assert_contains(&result.output, b"unsupported shell `powershell`");
    assert_contains(&result.output, b"expected bash, zsh, or fish");
    assert!(
        result.restored,
        "terminal mode changed after startup failure"
    );
    assert_directory_empty(temp.path());
}

#[test]
fn missing_bash_executable_failure_restores_terminal_and_cleans_session_directory() {
    let home = tempfile::tempdir().expect("create a temporary home for startup failure");
    let temp = tempfile::tempdir().expect("create a temporary session directory root");
    let missing_bash = home.path().join("missing/bash");
    let missing_bash = missing_bash
        .to_str()
        .expect("the fixture executable path should be valid UTF-8");
    let result = StartupAttempt::start(missing_bash, home.path(), temp.path(), None).finish();

    assert_eq!(
        result.status, 1,
        "missing shell executable should return an error"
    );
    assert_contains(&result.output, b"could not start bash reading session");
    assert_contains(&result.output, b"No such file");
    assert!(
        result.restored,
        "terminal mode changed after startup failure"
    );
    assert_directory_empty(temp.path());
    assert!(!home.path().join("missing").exists());
}

#[test]
fn invalid_tmpdir_failure_restores_terminal_without_leaving_session_resources() {
    let home = tempfile::tempdir().expect("create a temporary home for startup failure");
    let invalid_tempdir_root = tempfile::tempdir().expect("create a temporary fixture directory");
    let temp_file = invalid_tempdir_root.path().join("not-a-directory");
    fs::write(&temp_file, b"keep this fixture").expect("create a regular-file TMPDIR");
    let result = StartupAttempt::start("/bin/bash", home.path(), &temp_file, None).finish();

    assert_eq!(result.status, 1, "invalid TMPDIR should return an error");
    assert_contains(&result.output, b"could not create session directory");
    assert!(
        result.restored,
        "terminal mode changed after startup failure"
    );
    assert_eq!(
        fs::read(&temp_file).expect("preserve TMPDIR fixture"),
        b"keep this fixture"
    );
    assert_eq!(
        fs::read_dir(invalid_tempdir_root.path())
            .expect("inspect TMPDIR fixture directory")
            .count(),
        1,
        "startup failure should not leave any session resources"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn low_child_file_limit_failure_restores_terminal_and_cleans_session_directory() {
    for (limit, diagnostic) in [
        (4, "could not prepare shell integration"),
        (5, "could not create reading-session PTY"),
        (6, "could not read from the reading-session PTY"),
        (7, "could not write to the reading-session PTY"),
        (8, "could not read Bash integration"),
    ] {
        let home = tempfile::tempdir().expect("create a temporary home for startup failure");
        let temp = tempfile::tempdir().expect("create a temporary session directory root");
        let result =
            StartupAttempt::start("/bin/bash", home.path(), temp.path(), Some(limit)).finish();

        assert_eq!(
            result.status, 1,
            "low child-only descriptor limit should fail startup"
        );
        assert_contains(&result.output, diagnostic.as_bytes());
        assert!(
            result.restored,
            "terminal mode changed after startup failure"
        );
        assert_directory_empty(temp.path());
    }
}

struct StartupResult {
    output: Vec<u8>,
    status: u32,
    restored: bool,
}

struct StartupAttempt {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<Vec<u8>>,
    original_mode: TerminalModeSnapshot,
}

impl StartupAttempt {
    fn start(
        shell: &str,
        home: &std::path::Path,
        tempdir: &std::path::Path,
        file_limit: Option<u64>,
    ) -> Self {
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pseudo-terminal for the startup failure test");
        let terminal_fd = pair
            .master
            .as_raw_fd()
            .expect("test PTY master should have a file descriptor");
        let original_mode = terminal_mode(terminal_fd);

        let mut command = if let Some(limit) = file_limit {
            let mut command = CommandBuilder::new("/bin/sh");
            let limit = limit.to_string();
            command.args([
                "-c",
                "ulimit -n \"$3\" || exit 125; exec \"$1\" session --shell \"$2\"",
                "bionic-reader-startup-test",
                env!("CARGO_BIN_EXE_bionic-reader"),
                shell,
                &limit,
            ]);
            command
        } else {
            let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_bionic-reader"));
            command.args(["session", "--shell", shell]);
            command
        };
        command.env("HOME", home);
        command.env("XDG_CONFIG_HOME", home.join(".config"));
        command.env("TMPDIR", tempdir);
        command.env("TERM", "xterm-256color");
        command.env_remove("NO_COLOR");
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
            .expect("start the CLI process for the startup failure test");
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .expect("clone the startup failure PTY reader");
        let writer = pair
            .master
            .take_writer()
            .expect("take the startup failure PTY writer");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = [0; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if sender.send(buffer[..count].to_vec()).is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = sender.send(Vec::new());
        });

        Self {
            child,
            master: pair.master,
            writer,
            receiver,
            original_mode,
        }
    }

    fn finish(mut self) -> StartupResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = Vec::new();
        let mut status = None;
        let mut output_closed = false;
        while status.is_none() || !output_closed {
            assert!(
                Instant::now() < deadline,
                "startup failure did not exit within its deadline: {}",
                String::from_utf8_lossy(&output)
            );
            if status.is_none() {
                status = self
                    .child
                    .try_wait()
                    .expect("poll the startup failure child process");
            }
            match self.receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(chunk) if chunk.is_empty() => output_closed = true,
                Ok(chunk) => output.extend_from_slice(&chunk),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => output_closed = true,
            }
        }

        let current_mode = terminal_mode(
            self.master
                .as_raw_fd()
                .expect("test PTY master should keep its file descriptor"),
        );
        StartupResult {
            output,
            status: status
                .expect("child status was observed before output closed")
                .exit_code(),
            restored: current_mode == self.original_mode,
        }
    }
}

impl Drop for StartupAttempt {
    fn drop(&mut self) {
        // EOF lets an unexpectedly successful interactive shell exit before the deadline panic
        // unwinds the harness. Kill the launcher as a final bounded cleanup step.
        let _ = self.writer.write_all(b"\x04");
        let _ = self.writer.flush();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TerminalModeSnapshot {
    input_flags: libc::tcflag_t,
    output_flags: libc::tcflag_t,
    control_flags: libc::tcflag_t,
    local_flags: libc::tcflag_t,
    control_chars: Vec<libc::cc_t>,
}

fn terminal_mode(fd: i32) -> TerminalModeSnapshot {
    let mut settings = MaybeUninit::<libc::termios>::uninit();
    let result = {
        // SAFETY: tcgetattr initializes the terminal settings on success; the PTY fd stays open.
        unsafe { libc::tcgetattr(fd, settings.as_mut_ptr()) }
    };
    assert_eq!(result, 0, "tcgetattr should inspect the open test PTY");
    // SAFETY: tcgetattr returned success and initialized the termios value.
    let settings = unsafe { settings.assume_init() };
    TerminalModeSnapshot {
        input_flags: settings.c_iflag,
        output_flags: settings.c_oflag,
        control_flags: settings.c_cflag,
        local_flags: settings.c_lflag,
        control_chars: settings.c_cc.to_vec(),
    }
}

fn assert_directory_empty(path: &std::path::Path) {
    let entries = fs::read_dir(path)
        .expect("inspect session temporary directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("read session temporary directory entries");
    assert!(
        entries.is_empty(),
        "left temporary session resources: {entries:?}"
    );
}

fn assert_contains(output: &[u8], expected: &[u8]) {
    assert!(
        output
            .windows(expected.len())
            .any(|window| window == expected),
        "expected {:?} in PTY output: {:?}",
        String::from_utf8_lossy(expected),
        String::from_utf8_lossy(output)
    );
}

struct Terminal {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<Vec<u8>>,
    raw: Vec<u8>,
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
        command.env("BIONIC_READER_BIN", env!("CARGO_BIN_EXE_bionic-reader"));
        command.env("XDG_CONFIG_HOME", home.path().join(".config"));
        command.env("BASH_SILENCE_DEPRECATION_WARNING", "1");
        command.env("TERM", "xterm-256color");
        command.env_remove("NO_COLOR");
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
            raw: Vec::new(),
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
                self.raw.extend(chunk);
                self.plain = strip_sgr(&self.raw);
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
