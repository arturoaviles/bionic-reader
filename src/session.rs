//! Interactive PTY reading sessions and per-session controls.
//!
//! [`run`] starts a child shell, temporarily changes the caller's terminal mode, and creates a
//! private control socket. Terminal mode, the child process, temporary files, and workers are
//! cleaned up when the session ends or startup fails. [`control`] and [`bypass`] use the active
//! session identified by its environment variables.
//!
//! # Examples
//!
//! ```no_run
//! use bionic_reader::session;
//! use bionic_reader::settings::{Overrides, Settings};
//!
//! let status = session::run(Some("/bin/bash"), Settings::default(), None, Overrides::default())?;
//! # Ok::<(), bionic_reader::Error>(())
//! ```

use std::collections::HashSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, Permissions};
use std::io::{self, Read, Write};
use std::mem::{self, MaybeUninit};
use std::net::Shutdown;
use std::ops::{Deref, DerefMut};
use std::os::fd::RawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus as ProcessExitStatus};
use std::str;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize};
use signal_hook::consts::signal;
use signal_hook::iterator::{Handle as SignalHandle, Signals};

use crate::Error;
use crate::settings::{Overrides, Settings};
use crate::shell;
use crate::transformer::{self, Transformer};

// SAFETY JUSTIFICATION: The session must poll inherited terminal descriptors, read raw terminal
// input, manipulate termios, query terminal dimensions, and signal process groups. The standard
// library does not expose safe APIs for these POSIX operations. Each unsafe operation below is
// confined to a helper or a small call site with a valid descriptor, buffer, or child process ID.

// Bound untrusted shell metadata and stalled partial controls.
const MARKER_LIMIT: usize = 64 * 1024;
const PENDING_FLUSH: Duration = Duration::from_millis(50);
// Short polling keeps shutdown responsive without busy-spinning.
const WORKER_POLL_MS: i32 = 100;
const LOOP_POLL: Duration = Duration::from_millis(10);
// Give the shell a chance to handle TERM/HUP before killing its process groups.
const TERMINATION_GRACE: Duration = Duration::from_millis(250);
// A local client must not hold the control worker or its shutdown join indefinitely.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const CONTROL_SOCKET_ENV: &str = "BIONIC_READER_CONTROL_SOCKET";
const SESSION_ENV: &str = "BIONIC_READER_SESSION";
const MARKER_TOKEN_ENV: &str = "BIONIC_READER_MARKER_TOKEN";

/// Run a login shell inside a pseudo-terminal and transform marked command output.
///
/// # Errors
///
/// Returns an error if the caller is not attached to a terminal, the shell cannot be prepared,
/// or a required terminal, process, or session-control operation fails.
#[expect(
    clippy::too_many_lines,
    reason = "keep PTY ownership, event handling, and teardown order visible together"
)]
pub fn run(
    shell: Option<&str>,
    settings: Settings,
    config_path: Option<PathBuf>,
    overrides: Overrides,
) -> Result<i32, Error> {
    use std::io::IsTerminal;
    use std::os::unix::fs::PermissionsExt;

    settings.validate()?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::new(
            "reading sessions require a terminal on both stdin and stdout".to_owned(),
        ));
    }

    let (shell_kind, shell_program) = resolve_shell(shell)?;
    let terminal_mode = TerminalMode::try_new(libc::STDIN_FILENO).map_err(|error| {
        Error::with_source("could not enter terminal raw mode".to_owned(), error)
    })?;
    let session_temp = tempfile::Builder::new()
        .prefix("bionic-reader-session-")
        .tempdir()
        .map_err(|error| {
            Error::with_source("could not create session directory".to_owned(), error)
        })?;
    let marker_token = random_token()?;
    let socket_path = session_temp.path().join("control.sock");
    let listener = UnixListener::bind(&socket_path).map_err(|error| {
        Error::with_source("could not create session control socket".to_owned(), error)
    })?;
    fs::set_permissions(&socket_path, Permissions::from_mode(0o600)).map_err(|error| {
        Error::with_source("could not protect session control socket".to_owned(), error)
    })?;
    listener.set_nonblocking(true).map_err(|error| {
        Error::with_source(
            "could not configure session control socket".to_owned(),
            error,
        )
    })?;

    let initial_enabled = settings.style.enabled(true);
    let runtime = Arc::new(Mutex::new(RuntimeState {
        settings,
        enabled: initial_enabled,
        enabled_override: None,
        bypass_tokens: HashSet::new(),
        ended_bypass_tokens: HashSet::new(),
    }));
    let reload_source = ReloadSource {
        config_path,
        overrides,
    };

    let integration_path = session_temp
        .path()
        .join(format!("integration.{shell_kind}"));
    fs::write(&integration_path, shell::init(shell_kind)?).map_err(|error| {
        Error::with_source("could not prepare shell integration".to_owned(), error)
    })?;

    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(current_terminal_size())
        .map_err(|error| {
            Error::with_source("could not create reading-session PTY".to_owned(), error)
        })?;
    let master = pair.master;
    let reader = master.try_clone_reader().map_err(|error| {
        Error::with_source(
            "could not read from the reading-session PTY".to_owned(),
            error,
        )
    })?;
    let reader_fd = master
        .as_raw_fd()
        .ok_or_else(|| Error::new("reading-session PTY has no file descriptor".to_owned()))?;
    let writer = master.take_writer().map_err(|error| {
        Error::with_source(
            "could not write to the reading-session PTY".to_owned(),
            error,
        )
    })?;

    let mut command = CommandBuilder::new(&shell_program);
    configure_shell(
        &mut command,
        shell_kind,
        &integration_path,
        session_temp.path(),
    )?;
    command.env(SESSION_ENV, "1");
    command.env(MARKER_TOKEN_ENV, &marker_token);
    command.env(CONTROL_SOCKET_ENV, &socket_path);
    command.env("BIONIC_READER_DISABLE", "0");

    let (output_sender, output_receiver) = mpsc::sync_channel(8);
    let mut workers = Workers::new(output_receiver);
    let child = pair.slave.spawn_command(command).map_err(|error| {
        Error::with_source(
            format!("could not start {shell_kind} reading session"),
            error,
        )
    })?;
    drop(pair.slave);
    let mut child = ChildCleanup {
        child,
        master: master.as_ref(),
    };
    let control_thread = spawn_control_server(
        listener,
        Arc::clone(&runtime),
        reload_source,
        Arc::clone(&workers.stop),
    )
    .map_err(|error| {
        Error::with_source("could not start session control worker".to_owned(), error)
    })?;
    workers.threads.push(control_thread);

    let reader_stop = Arc::clone(&workers.stop);
    let reader_thread = thread::Builder::new()
        .name("bionic-output".to_owned())
        .spawn(move || {
            let mut reader = reader;
            let mut buffer = [0_u8; 8192];
            while !reader_stop.load(Ordering::Relaxed) {
                match wait_for_descriptor(reader_fd, WORKER_POLL_MS) {
                    Ok(false) => continue,
                    Ok(true) => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                        continue;
                    }
                    Err(_) => break,
                }
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if output_sender
                            .send(OutputEvent::Bytes(buffer[..count].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            let _ = output_sender.send(OutputEvent::Closed);
        })
        .map_err(|error| {
            Error::with_source("could not start session output worker".to_owned(), error)
        })?;
    workers.threads.push(reader_thread);

    let input_thread = spawn_input_pump(writer, Arc::clone(&workers.stop)).map_err(|error| {
        Error::with_source("could not start session input worker".to_owned(), error)
    })?;
    workers.threads.push(input_thread);
    let (signal_sender, signal_receiver) = mpsc::channel();
    let (signal_handle, signal_thread) = spawn_signal_pump(signal_sender).map_err(|error| {
        Error::with_source(
            "could not install session signal handlers".to_owned(),
            error,
        )
    })?;
    workers.signal_handle = Some(signal_handle);
    workers.threads.push(signal_thread);

    let mut parser = MarkerParser::new(&marker_token);
    let mut output_state = OutputState::new(Arc::clone(&runtime));
    let mut output = io::stdout().lock();
    let mut child_status = None;
    let mut output_closed = false;
    let mut loop_error = None;
    let mut termination_signal = None;
    let mut shutdown_deadline = None;
    let mut termination_escalated = false;

    while !(child_status.is_some() && output_closed) {
        for signal in signal_receiver.try_iter() {
            if signal == signal::SIGWINCH {
                if let Err(error) = master.resize(current_terminal_size()) {
                    loop_error = Some(Error::with_source(
                        "could not resize reading-session PTY".to_owned(),
                        error,
                    ));
                    break;
                }
            } else {
                forward_to_foreground_group(master.as_ref(), child.process_id(), signal);
                if signal == signal::SIGTERM || signal == signal::SIGHUP {
                    termination_signal.get_or_insert(signal);
                    if let Some(pid) = child.process_id() {
                        signal_process_group(process_id(pid), signal);
                    }
                    shutdown_deadline.get_or_insert_with(|| Instant::now() + TERMINATION_GRACE);
                }
            }
        }
        if loop_error.is_some() {
            let _ = child.kill();
            break;
        }

        match workers.receive(LOOP_POLL) {
            Ok(OutputEvent::Bytes(bytes)) => {
                for event in parser.push(&bytes) {
                    if let Err(error) = output_state.process(event, &mut output) {
                        loop_error = Some(Error::with_source(
                            "could not write reading-session output".to_owned(),
                            error,
                        ));
                        let _ = child.kill();
                        break;
                    }
                }
                if loop_error.is_some() {
                    break;
                }
            }
            Ok(OutputEvent::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                output_closed = true;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        if child_status.is_none() {
            match child.try_wait() {
                Ok(status) => child_status = status,
                Err(error) => {
                    loop_error = Some(Error::with_source(
                        "could not wait for reading session".to_owned(),
                        error,
                    ));
                    let _ = child.kill();
                    break;
                }
            }
        }

        if !termination_escalated
            && shutdown_deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            if let Some(pid) = child.process_id() {
                kill_session_groups(master.as_ref(), pid);
            }
            let _ = child.kill();
            termination_escalated = true;
        }

        for event in parser.flush_pending() {
            if let Err(error) = output_state.process(event, &mut output) {
                loop_error = Some(Error::with_source(
                    "could not flush partial terminal control".to_owned(),
                    error,
                ));
                let _ = child.kill();
                break;
            }
        }
        if loop_error.is_some() {
            break;
        }
        if let Err(error) = output_state.flush_pending(&mut output) {
            loop_error = Some(Error::with_source(
                "could not flush reading-session output".to_owned(),
                error,
            ));
            let _ = child.kill();
            break;
        }
        // Prompts, control sequences, and interactive programs often do not
        // end their output with a newline. Rust's stdout line buffer must be
        // flushed at each PTY batch so the terminal can display them now.
        if let Err(error) = output.flush() {
            loop_error = Some(Error::with_source(
                "could not flush terminal output".to_owned(),
                error,
            ));
            break;
        }
    }

    if loop_error.is_some() {
        if let Some(pid) = child.process_id() {
            kill_session_groups(master.as_ref(), pid);
        }
        let _ = child.kill();
        if child_status.is_none() {
            child_status = child.wait().ok();
        }
    }

    drop(workers);

    if let Some(error) = loop_error {
        drop(output);
        drop(terminal_mode);
        return Err(error);
    }
    for event in parser.finish() {
        output_state.process(event, &mut output).map_err(|error| {
            Error::with_source("could not finish reading-session output".to_owned(), error)
        })?;
    }
    output_state.finish(&mut output).map_err(|error| {
        Error::with_source("could not finish reading-session output".to_owned(), error)
    })?;
    output
        .flush()
        .map_err(|error| Error::with_source("could not flush terminal output".to_owned(), error))?;
    drop(output);
    drop(terminal_mode);

    let status = child_status.unwrap_or_else(|| ExitStatus::with_exit_code(1));
    Ok(termination_signal.map_or_else(|| pty_exit_code(&status), signal_exit_code))
}

/// Change the active reading-session control state.
///
/// # Errors
///
/// Returns an error for an unsupported control command, when no reading session is active, or
/// when the active session rejects or cannot receive the request.
pub fn control(command: &str) -> Result<(), Error> {
    match command {
        "on" | "off" | "toggle" | "reload" => {}
        _ => {
            return Err(Error::new(format!(
                "unknown reading-session control `{command}`"
            )));
        }
    }
    let socket = env::var_os(CONTROL_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            Error::new(format!(
                "`bionic-reader {command}` requires an active reading session"
            ))
        })?;
    send_control_request(&socket, command)
}

/// Run a command with session transformation suspended until it completes.
///
/// # Errors
///
/// Returns an error if the command is missing, no reading session is active, or the bypass
/// command or its session-control request cannot be started.
pub fn bypass(command: &[OsString]) -> Result<i32, Error> {
    let Some(program) = command.first() else {
        return Err(Error::new(
            "usage: bionic-reader bypass -- <command> [args ...]".to_owned(),
        ));
    };
    if env::var_os(SESSION_ENV).as_deref() != Some(OsStr::new("1")) {
        return Err(Error::new(
            "`bionic-reader bypass` requires an active reading session".to_owned(),
        ));
    }
    let marker_token = env::var(MARKER_TOKEN_ENV).map_err(|error| {
        Error::with_source_hidden(
            "the active reading session has no marker token".to_owned(),
            error,
        )
    })?;
    let bypass_id = random_token()?;
    let socket = env::var_os(CONTROL_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| Error::new("the active reading session has no control socket".to_owned()))?;
    let mut marker_guard = BypassMarkerGuard {
        socket,
        token: bypass_id,
        marker_token,
        terminal_marker: false,
        active: true,
    };
    // Own rollback before activation: a lost acknowledgment can follow a successful start.
    let status = marker_guard
        .start()
        .and_then(|()| run_bypass_command(program, &command[1..]));
    complete_bypass(status, marker_guard.finish())
}

fn complete_bypass(status: Result<i32, Error>, cleanup: Result<(), Error>) -> Result<i32, Error> {
    match (status, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("bypass cleanup also failed ({cleanup:#})")))
        }
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(error),
    }
}

fn write_marker(marker: &[u8]) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(marker)?;
    stdout.flush()
}

struct BypassMarkerGuard {
    socket: PathBuf,
    token: String,
    marker_token: String,
    terminal_marker: bool,
    active: bool,
}

impl BypassMarkerGuard {
    fn start(&mut self) -> Result<(), Error> {
        use std::io::IsTerminal;

        send_control_request(&self.socket, &format!("bypass-start:{}", self.token))?;
        if io::stdout().is_terminal() {
            // A partial write also needs an end-marker attempt during cleanup.
            self.terminal_marker = true;
            write_marker(&dcs_marker(
                &self.marker_token,
                &format!("BYPASS+{}", self.token),
            ))
            .map_err(|error| Error::with_source("could not start bypass".to_owned(), error))?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        if !self.active {
            return Ok(());
        }
        let marker_result = if self.terminal_marker {
            let marker = dcs_marker(&self.marker_token, &format!("BYPASS-{}", self.token));
            write_marker(&marker)
                .map_err(|error| Error::with_source("could not finish bypass".to_owned(), error))
        } else {
            Ok(())
        };
        // Always restore control state, even when writing the terminal marker failed.
        send_control_request(&self.socket, &format!("bypass-end:{}", self.token))?;
        self.active = false;
        marker_result
    }
}

impl Drop for BypassMarkerGuard {
    fn drop(&mut self) {
        // Explicit completion reports errors; unwinding can only attempt best-effort cleanup.
        let _ = self.finish();
    }
}

fn run_bypass_command(program: &OsStr, arguments: &[OsString]) -> Result<i32, Error> {
    let mut signals = Signals::new([
        signal::SIGINT,
        signal::SIGHUP,
        signal::SIGQUIT,
        signal::SIGTERM,
    ])
    .map_err(|error| {
        Error::with_source("could not install bypass signal handlers".to_owned(), error)
    })?;
    let signal_handle = signals.handle();
    let mut child = Command::new(program)
        .args(arguments)
        .spawn()
        .map_err(|error| Error::with_source("could not run bypass command".to_owned(), error))?;
    let (signal_sender, signal_receiver) = mpsc::channel();
    let signal_thread = match thread::Builder::new()
        .name("bionic-bypass-signals".to_owned())
        .spawn(move || {
            for signal in signals.forever() {
                if signal_sender.send(signal).is_err() {
                    break;
                }
            }
        }) {
        Ok(worker) => worker,
        Err(error) => {
            signal_handle.close();
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::with_source(
                "could not start bypass signal worker".to_owned(),
                error,
            ));
        }
    };
    let status = loop {
        for signal in signal_receiver.try_iter() {
            signal_process(process_id(child.id()), signal);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(LOOP_POLL),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                signal_handle.close();
                let _ = signal_thread.join();
                return Err(Error::with_source(
                    "could not wait for bypass command".to_owned(),
                    error,
                ));
            }
        }
    };
    signal_handle.close();
    let _ = signal_thread.join();
    Ok(process_exit_code(status))
}

fn send_control_request(socket: &Path, request: &str) -> Result<(), Error> {
    let mut stream = UnixStream::connect(socket).map_err(|error| {
        Error::with_source(
            "could not reach the active reading session".to_owned(),
            error,
        )
    })?;
    set_control_timeout(&stream).map_err(|error| {
        Error::with_source(
            "could not configure session control timeout".to_owned(),
            error,
        )
    })?;
    stream
        .write_all(request.as_bytes())
        .map_err(|error| Error::with_source("could not send session control".to_owned(), error))?;
    stream.shutdown(Shutdown::Write).map_err(|error| {
        Error::with_source("could not finish session control request".to_owned(), error)
    })?;
    let mut response = String::new();
    stream
        .take(64 * 1024)
        .read_to_string(&mut response)
        .map_err(|error| {
            Error::with_source("could not read session control response".to_owned(), error)
        })?;
    if response.trim() == "ok" {
        Ok(())
    } else {
        Err(Error::new(
            response
                .strip_prefix("error:")
                .unwrap_or("reading session rejected the control request")
                .trim()
                .to_owned(),
        ))
    }
}

#[must_use]
fn process_exit_code(status: ProcessExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;

    status
        .code()
        .unwrap_or_else(|| status.signal().map_or(1, |signal| 128 + signal))
}

#[must_use]
fn pty_exit_code(status: &ExitStatus) -> i32 {
    if let Some(signal) = status.signal() {
        match signal {
            "Hangup" => 129,
            "Interrupt" => 130,
            "Quit" => 131,
            "Terminated" => 143,
            _ => i32::try_from(status.exit_code()).unwrap_or(1),
        }
    } else {
        i32::try_from(status.exit_code()).unwrap_or(1)
    }
}

#[must_use]
fn signal_exit_code(signal: i32) -> i32 {
    match signal {
        signal::SIGHUP => 129,
        signal::SIGINT => 130,
        signal::SIGQUIT => 131,
        signal::SIGTERM => 143,
        _ => 128 + signal,
    }
}

fn forward_to_foreground_group(master: &dyn MasterPty, fallback_pid: Option<u32>, signal: i32) {
    let group = master
        .process_group_leader()
        .or_else(|| fallback_pid.map(process_id));
    if let Some(group) = group {
        signal_process_group(group, signal);
    }
}

fn set_control_timeout(stream: &UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
    stream.set_write_timeout(Some(CONTROL_TIMEOUT))
}

#[must_use]
fn process_id(pid: u32) -> libc::pid_t {
    libc::pid_t::try_from(pid).expect("a live POSIX child PID fits in pid_t")
}

fn signal_process_group(group: libc::pid_t, signal: i32) {
    if group > 0 {
        // SAFETY: the negative pid targets the process group returned by tcgetpgrp
        // or the child process-group leader created by portable-pty.
        unsafe { libc::kill(-group, signal) };
    }
}

fn signal_process(process_id: libc::pid_t, signal: i32) {
    if process_id > 0 {
        // SAFETY: the PID is returned by `std::process::Child::id` for a live child process.
        let _ = unsafe { libc::kill(process_id, signal) };
    }
}

fn kill_session_groups(master: &dyn MasterPty, shell_pid: u32) {
    if let Some(foreground) = master.process_group_leader() {
        if foreground != process_id(shell_pid) {
            signal_process_group(foreground, signal::SIGKILL);
        }
    }
    signal_process_group(process_id(shell_pid), signal::SIGKILL);
}

struct RuntimeState {
    settings: Settings,
    enabled: bool,
    enabled_override: Option<bool>,
    bypass_tokens: HashSet<String>,
    ended_bypass_tokens: HashSet<String>,
}

struct ReloadSource {
    config_path: Option<PathBuf>,
    overrides: Overrides,
}

impl ReloadSource {
    fn load(&self) -> Result<Settings, Error> {
        Settings::load(self.config_path.as_deref(), &self.overrides)
    }
}

fn spawn_control_server(
    listener: UnixListener,
    state: Arc<Mutex<RuntimeState>>,
    reload_source: ReloadSource,
    stop: Arc<AtomicBool>,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let result = set_control_timeout(&stream)
                        .map_err(|error| {
                            Error::with_source(
                                "could not configure session control timeout".to_owned(),
                                error,
                            )
                        })
                        .and_then(|()| handle_control_request(&mut stream, &state, &reload_source));
                    let response = match result {
                        Ok(()) => "ok\n".to_owned(),
                        Err(error) => format!("error:{error}\n"),
                    };
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(LOOP_POLL);
                }
                Err(_) => break,
            }
        }
    })
}

fn handle_control_request(
    stream: &mut UnixStream,
    state: &Mutex<RuntimeState>,
    reload_source: &ReloadSource,
) -> Result<(), Error> {
    let mut request = String::new();
    stream
        .take(64)
        .read_to_string(&mut request)
        .map_err(|error| Error::with_source("could not read control request".to_owned(), error))?;
    let request = request.trim();
    // Disk I/O and TOML parsing must not hold the lock used by terminal output.
    let refreshed = (request == "reload")
        .then(|| reload_source.load())
        .transpose()?;
    let mut state = state
        .lock()
        .map_err(|_poisoned| Error::new("session control state is unavailable".to_owned()))?;
    if let Some(refreshed) = refreshed {
        state.settings = refreshed;
        if state.enabled_override.is_none() {
            state.enabled = state.settings.style.enabled(true);
        }
        return Ok(());
    }
    match request {
        "on" => {
            state.enabled = true;
            state.enabled_override = Some(true);
        }
        "off" => {
            state.enabled = false;
            state.enabled_override = Some(false);
        }
        "toggle" => {
            state.enabled = !state.enabled;
            state.enabled_override = Some(state.enabled);
        }
        request if request.starts_with("bypass-start:") => {
            let token = &request["bypass-start:".len()..];
            if !is_marker_id(token.as_bytes()) {
                return Err(Error::new("invalid bypass token".to_owned()));
            }
            state.bypass_tokens.insert(token.to_owned());
        }
        request if request.starts_with("bypass-end:") => {
            let token = &request["bypass-end:".len()..];
            if !is_marker_id(token.as_bytes()) {
                return Err(Error::new("invalid bypass token".to_owned()));
            }
            if state.bypass_tokens.contains(token) {
                state.ended_bypass_tokens.insert(token.to_owned());
            }
        }
        _ => return Err(Error::new("invalid session control request".to_owned())),
    }
    Ok(())
}

#[derive(Debug)]
enum OutputEvent {
    Bytes(Vec<u8>),
    Closed,
}

// Owns the receiving end so a blocked bounded-channel send is released before joining.
struct Workers {
    stop: Arc<AtomicBool>,
    signal_handle: Option<SignalHandle>,
    threads: Vec<JoinHandle<()>>,
    receiver: Option<mpsc::Receiver<OutputEvent>>,
}

impl Workers {
    #[must_use]
    fn new(receiver: mpsc::Receiver<OutputEvent>) -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            signal_handle: None,
            threads: Vec::with_capacity(4),
            receiver: Some(receiver),
        }
    }

    fn receive(&self, timeout: Duration) -> Result<OutputEvent, mpsc::RecvTimeoutError> {
        self.receiver
            .as_ref()
            .expect("the output receiver exists until worker teardown")
            .recv_timeout(timeout)
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = &self.signal_handle {
            handle.close();
        }
        drop(self.receiver.take());
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

// Created after Workers, so a failed startup kills/reaps the child before joining workers.
struct ChildCleanup<'pty> {
    child: Box<dyn Child + Send + Sync>,
    master: &'pty dyn MasterPty,
}

impl Deref for ChildCleanup<'_> {
    type Target = dyn Child + Send + Sync;

    fn deref(&self) -> &Self::Target {
        self.child.as_ref()
    }
}

impl DerefMut for ChildCleanup<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.child.as_mut()
    }
}

impl Drop for ChildCleanup<'_> {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            if let Some(pid) = self.child.process_id() {
                kill_session_groups(self.master, pid);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn wait_for_descriptor(fd: RawFd, timeout_ms: i32) -> io::Result<bool> {
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: `descriptor` is initialized and points to this stack frame for the call.
    let ready = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
    match ready {
        -1 => Err(io::Error::last_os_error()),
        0 => Ok(false),
        _ => Ok(true),
    }
}

fn read_descriptor(fd: RawFd, buffer: &mut [u8]) -> io::Result<usize> {
    // SAFETY: `buffer` is a valid writable slice and remains alive for the call.
    let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
    if count < 0 {
        Err(io::Error::last_os_error())
    } else {
        // POSIX `read` never reports more bytes than the buffer length on success.
        Ok(usize::try_from(count).expect("a successful POSIX read returns a nonnegative count"))
    }
}

fn spawn_input_pump(
    mut writer: Box<dyn Write + Send>,
    stop: Arc<AtomicBool>,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().spawn(move || {
        let mut buffer = [0_u8; 4096];
        while !stop.load(Ordering::Relaxed) {
            match wait_for_descriptor(libc::STDIN_FILENO, WORKER_POLL_MS) {
                Ok(false) => continue,
                Ok(true) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(_) => break,
            }
            let Ok(count) = read_descriptor(libc::STDIN_FILENO, &mut buffer) else {
                break;
            };
            if count == 0 {
                break;
            }
            if writer.write_all(&buffer[..count]).is_err() || writer.flush().is_err() {
                break;
            }
        }
    })
}

fn spawn_signal_pump(sender: mpsc::Sender<i32>) -> io::Result<(SignalHandle, JoinHandle<()>)> {
    let mut signals = Signals::new([
        signal::SIGWINCH,
        signal::SIGHUP,
        signal::SIGINT,
        signal::SIGQUIT,
        signal::SIGTERM,
    ])?;
    let handle = signals.handle();
    let thread = thread::Builder::new()
        .spawn(move || {
            for signal in signals.forever() {
                if sender.send(signal).is_err() {
                    break;
                }
            }
        })
        .inspect_err(|_error| handle.close())?;
    Ok((handle, thread))
}

struct TerminalMode {
    fd: RawFd,
    original: libc::termios,
}

impl TerminalMode {
    fn try_new(fd: RawFd) -> io::Result<Self> {
        let mut original: MaybeUninit<libc::termios> = MaybeUninit::uninit();
        // SAFETY: tcgetattr initializes the termios value when it returns zero.
        if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: tcgetattr succeeded above.
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        // SAFETY: raw points to a valid termios value.
        unsafe { libc::cfmakeraw(&mut raw) };
        // SAFETY: raw is initialized and fd is the active terminal descriptor.
        if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { fd, original })
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        // SAFETY: original is the termios value captured from this terminal.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSAFLUSH, &self.original);
        }
    }
}

#[must_use]
fn current_terminal_size() -> PtySize {
    let mut size: MaybeUninit<libc::winsize> = MaybeUninit::zeroed();
    // SAFETY: size is a valid winsize output buffer for TIOCGWINSZ.
    let result = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) };
    if result == 0 {
        // SAFETY: ioctl succeeded and initialized winsize.
        let size = unsafe { size.assume_init() };
        PtySize {
            rows: size.ws_row.max(1),
            cols: size.ws_col.max(1),
            pixel_width: size.ws_xpixel,
            pixel_height: size.ws_ypixel,
        }
    } else {
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

fn random_token() -> Result<String, Error> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; 16];
    let mut random = File::open("/dev/urandom").map_err(|error| {
        Error::with_source("could not access system randomness".to_owned(), error)
    })?;
    random.read_exact(&mut bytes).map_err(|error| {
        Error::with_source("could not create a session token".to_owned(), error)
    })?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(char::from(HEX[usize::from(byte >> 4)]));
        token.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(token)
}

fn resolve_shell(shell: Option<&str>) -> Result<(&'static str, OsString), Error> {
    let requested = shell
        .map(OsString::from)
        .or_else(|| env::var_os("SHELL"))
        .unwrap_or_else(|| OsString::from("/bin/bash"));
    let requested_string = requested.to_string_lossy();
    let kind = shell::normalize_shell(&requested_string)?;
    Ok((kind, requested))
}

fn configure_shell(
    command: &mut CommandBuilder,
    shell_kind: &str,
    integration_path: &Path,
    temp_dir: &Path,
) -> Result<(), Error> {
    let integration_path = integration_path.to_str().ok_or_else(|| {
        Error::new("temporary shell integration path is not valid UTF-8".to_owned())
    })?;
    let temp_dir = temp_dir
        .to_str()
        .ok_or_else(|| Error::new("temporary session path is not valid UTF-8".to_owned()))?;
    command.env("BIONIC_READER_INTEGRATION", integration_path);
    match shell_kind {
        "bash" => {
            let user_rc = env::var_os("HOME")
                .map_or_else(|| PathBuf::from("."), PathBuf::from)
                .join(".bashrc");
            command.env("BIONIC_READER_USER_RC", user_rc);
            let rc_path = Path::new(temp_dir).join("bashrc");
            let integration = fs::read_to_string(integration_path).map_err(|error| {
                Error::with_source("could not read Bash integration".to_owned(), error)
            })?;
            let startup = format!(
                "if [[ -r ${{BIONIC_READER_USER_RC-}} ]]; then source \"$BIONIC_READER_USER_RC\"; fi\n{integration}"
            );
            fs::write(&rc_path, startup).map_err(|error| {
                Error::with_source("could not prepare Bash startup".to_owned(), error)
            })?;
            command.args(["--noprofile", "--rcfile"]);
            command.arg(rc_path);
            command.arg("-i");
        }
        "zsh" => {
            let user_zdotdir = env::var_os("ZDOTDIR")
                .or_else(|| env::var_os("HOME"))
                .unwrap_or_else(|| OsString::from("."));
            command.env("__BIONIC_READER_USER_ZDOTDIR", &user_zdotdir);
            command.env("ZDOTDIR", temp_dir);
            write_zsh_startup(temp_dir)?;
            command.args(["--interactive", "--login"]);
        }
        "fish" => {
            command.args(["--interactive", "--init-command"]);
            command.arg("source \"$BIONIC_READER_INTEGRATION\"");
        }
        _ => return Err(Error::new(format!("unsupported shell `{shell_kind}`"))),
    }
    Ok(())
}

fn write_zsh_startup(temp_dir: &str) -> Result<(), Error> {
    for file in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
        let variable = file.trim_start_matches('.').replace('.', "_");
        let mut script = format!(
            "typeset __br_wrapper_zdotdir_{variable}=$ZDOTDIR\n\
             if [[ -n $__BIONIC_READER_USER_ZDOTDIR && -r \"$__BIONIC_READER_USER_ZDOTDIR/{file}\" ]]; then\n\
               export ZDOTDIR=$__BIONIC_READER_USER_ZDOTDIR\n\
               source \"$ZDOTDIR/{file}\"\n\
               export __BIONIC_READER_USER_ZDOTDIR=$ZDOTDIR\n\
               export ZDOTDIR=$__br_wrapper_zdotdir_{variable}\n\
             fi\n"
        );
        if file == ".zshrc" {
            script.push_str(
                "if [[ -n $BIONIC_READER_INTEGRATION ]]; then source $BIONIC_READER_INTEGRATION; fi\n",
            );
        }
        if file == ".zlogin" {
            script.push_str(
                "if [[ -n $__BIONIC_READER_USER_ZDOTDIR ]]; then export ZDOTDIR=$__BIONIC_READER_USER_ZDOTDIR; fi\n\
                 unset __BIONIC_READER_USER_ZDOTDIR\n",
            );
        }
        let path = Path::new(temp_dir).join(file);
        fs::write(path, script).map_err(|error| {
            Error::with_source("could not prepare Zsh startup".to_owned(), error)
        })?;
    }
    Ok(())
}

#[derive(Debug)]
enum Marker {
    Begin(Vec<u8>),
    End,
    BypassPush(String),
    BypassPop(String),
    UnsafeDcs(Vec<u8>),
}

#[derive(Debug)]
enum ParsedEvent {
    Bytes(Vec<u8>),
    Marker(Marker),
}

struct MarkerParser {
    pending: Vec<u8>,
    token_prefix: Vec<u8>,
    discard_dcs: bool,
    pending_since: Option<Instant>,
}

impl MarkerParser {
    #[must_use]
    fn new(token: &str) -> Self {
        Self {
            pending: Vec::new(),
            token_prefix: format!("+BIONIC_READER;{token};").into_bytes(),
            discard_dcs: false,
            pending_since: None,
        }
    }

    #[must_use]
    fn push(&mut self, input: &[u8]) -> Vec<ParsedEvent> {
        if self.pending.is_empty() && !input.is_empty() {
            self.pending_since = Some(Instant::now());
        }
        self.pending.extend_from_slice(input);
        let mut events = Vec::new();
        loop {
            if self.discard_dcs {
                if let Some(end) = find_subslice(&self.pending, b"\x1b\\") {
                    let consumed = end + 2;
                    let bytes = self.pending.drain(..consumed).collect();
                    events.push(ParsedEvent::Bytes(bytes));
                    self.discard_dcs = false;
                    continue;
                }
                let keep = usize::from(self.pending.last() == Some(&0x1b));
                if self.pending.len() > keep {
                    let count = self.pending.len() - keep;
                    let bytes = self.pending.drain(..count).collect();
                    events.push(ParsedEvent::Bytes(bytes));
                }
                break;
            }

            let Some(start) = find_subslice(&self.pending, b"\x1bP") else {
                let keep = usize::from(self.pending.last() == Some(&0x1b));
                if self.pending.len() > keep {
                    let count = self.pending.len() - keep;
                    let bytes = self.pending.drain(..count).collect();
                    events.push(ParsedEvent::Bytes(bytes));
                }
                break;
            };
            if start > 0 {
                let bytes = self.pending.drain(..start).collect();
                events.push(ParsedEvent::Bytes(bytes));
                continue;
            }
            let Some(end) = find_subslice(&self.pending[2..], b"\x1b\\") else {
                if self.pending.len() > MARKER_LIMIT {
                    let keep = usize::from(self.pending.last() == Some(&0x1b));
                    let count = self.pending.len() - keep;
                    let bytes = self.pending.drain(..count).collect();
                    events.push(ParsedEvent::Marker(Marker::UnsafeDcs(bytes)));
                    self.discard_dcs = true;
                }
                break;
            };
            let consumed = 2 + end + 2;
            let sequence: Vec<u8> = self.pending.drain(..consumed).collect();
            let payload = &sequence[2..sequence.len() - 2];
            if let Some(marker) = self.parse_marker(payload) {
                events.push(ParsedEvent::Marker(marker));
            } else {
                events.push(ParsedEvent::Marker(Marker::UnsafeDcs(sequence)));
            }
        }
        if self.pending.is_empty() {
            self.pending_since = None;
        } else if self.pending_since.is_none() {
            self.pending_since = Some(Instant::now());
        }
        events
    }

    #[must_use]
    fn flush_pending(&mut self) -> Vec<ParsedEvent> {
        let stalled = self
            .pending_since
            .is_some_and(|since| since.elapsed() >= PENDING_FLUSH);
        if !stalled || self.pending.is_empty() {
            return Vec::new();
        }
        let bytes = mem::take(&mut self.pending);
        if bytes.starts_with(b"\x1bP") {
            self.discard_dcs = true;
        }
        self.pending_since = None;
        vec![ParsedEvent::Marker(Marker::UnsafeDcs(bytes))]
    }

    #[must_use]
    fn finish(&mut self) -> Vec<ParsedEvent> {
        if self.pending.is_empty() {
            Vec::new()
        } else {
            vec![ParsedEvent::Marker(Marker::UnsafeDcs(mem::take(
                &mut self.pending,
            )))]
        }
    }

    #[must_use]
    fn parse_marker(&self, payload: &[u8]) -> Option<Marker> {
        let rest = payload.strip_prefix(self.token_prefix.as_slice())?;
        if rest == b"END" || rest == b"END:" {
            return Some(Marker::End);
        }
        if rest == b"BEGIN" {
            return Some(Marker::Begin(Vec::new()));
        }
        if let Some(metadata) = rest.strip_prefix(b"BEGIN:") {
            return Some(Marker::Begin(metadata.to_vec()));
        }
        if let Some(id) = rest.strip_prefix(b"BYPASS+") {
            if is_marker_id(id) {
                return Some(Marker::BypassPush(String::from_utf8_lossy(id).into_owned()));
            }
        }
        if let Some(id) = rest.strip_prefix(b"BYPASS-") {
            if is_marker_id(id) {
                return Some(Marker::BypassPop(String::from_utf8_lossy(id).into_owned()));
            }
        }
        None
    }
}

#[must_use]
fn is_marker_id(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= 64 && bytes.iter().all(u8::is_ascii_hexdigit)
}

#[must_use]
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

struct OutputState {
    runtime: Arc<Mutex<RuntimeState>>,
    transformer: Option<Transformer>,
    in_command: bool,
    transforming: bool,
    unsafe_interval: bool,
    bypass_tokens: HashSet<String>,
    pending_since: Option<Instant>,
    utf8_boundary: Utf8Boundary,
}

impl OutputState {
    #[must_use]
    fn new(runtime: Arc<Mutex<RuntimeState>>) -> Self {
        Self {
            runtime,
            transformer: None,
            in_command: false,
            transforming: false,
            unsafe_interval: false,
            bypass_tokens: HashSet::new(),
            pending_since: None,
            utf8_boundary: Utf8Boundary::default(),
        }
    }

    fn process(&mut self, event: ParsedEvent, output: &mut impl Write) -> io::Result<()> {
        match event {
            ParsedEvent::Bytes(bytes) => self.write_data(&bytes, output),
            ParsedEvent::Marker(Marker::Begin(metadata)) => self.begin(&metadata, output),
            ParsedEvent::Marker(Marker::End) => self.end(output),
            ParsedEvent::Marker(Marker::BypassPush(token)) => self.bypass_push(token, output),
            ParsedEvent::Marker(Marker::BypassPop(_token)) => {
                // Retain bypass until the trusted shell END, including downstream pipeline output.
                Ok(())
            }
            ParsedEvent::Marker(Marker::UnsafeDcs(bytes)) => {
                if self.in_command {
                    self.flush_transformer(output)?;
                    self.transforming = false;
                    self.unsafe_interval = true;
                }
                output.write_all(&bytes)
            }
        }
    }

    fn begin(&mut self, metadata: &[u8], output: &mut impl Write) -> io::Result<()> {
        if self.in_command {
            self.end(output)?;
        }
        retire_ended_bypass(&self.runtime, &mut self.bypass_tokens)?;
        let runtime = self
            .runtime
            .lock()
            .map_err(|_poisoned| io::Error::other("reading-session runtime state was poisoned"))?;
        let command_allowed = !command_is_excluded(metadata, &runtime.settings);
        self.in_command = true;
        self.unsafe_interval = false;
        self.bypass_tokens = runtime.bypass_tokens.clone();
        self.transforming = runtime.enabled && command_allowed && self.bypass_tokens.is_empty();
        self.transformer = self
            .transforming
            .then(|| Transformer::new(&runtime.settings, /* enabled */ true));
        self.pending_since = None;
        self.utf8_boundary = Utf8Boundary::default();
        Ok(())
    }

    fn end(&mut self, output: &mut impl Write) -> io::Result<()> {
        if self.in_command {
            self.flush_transformer(output)?;
        }
        self.transformer = None;
        self.pending_since = None;
        self.in_command = false;
        self.transforming = false;
        self.unsafe_interval = false;
        retire_ended_bypass(&self.runtime, &mut self.bypass_tokens)?;
        Ok(())
    }

    fn bypass_push(&mut self, token: String, output: &mut impl Write) -> io::Result<()> {
        if self.bypass_tokens.insert(token) && self.transforming {
            self.flush_transformer(output)?;
            self.transforming = false;
            self.transformer = None;
        }
        Ok(())
    }

    fn write_data(&mut self, bytes: &[u8], output: &mut impl Write) -> io::Result<()> {
        self.sync_bypass(output)?;
        if self.in_command && self.transforming && !self.unsafe_interval {
            if has_unsafe_control(bytes, &mut self.utf8_boundary) {
                self.flush_transformer(output)?;
                self.transforming = false;
                self.unsafe_interval = true;
                return output.write_all(bytes);
            }
            if let Some(transformer) = &mut self.transformer {
                let rendered = transformer.push(bytes);
                if !transformer.has_pending() {
                    self.pending_since = None;
                } else if self.pending_since.is_none() || !rendered.is_empty() {
                    self.pending_since = Some(Instant::now());
                }
                output.write_all(&rendered)?;
            } else {
                output.write_all(bytes)?;
            }
        } else {
            output.write_all(bytes)?;
        }
        Ok(())
    }

    fn flush_transformer(&mut self, output: &mut impl Write) -> io::Result<()> {
        if let Some(transformer) = &mut self.transformer {
            output.write_all(&transformer.finish())?;
        }
        self.transformer = None;
        self.pending_since = None;
        Ok(())
    }

    fn flush_pending(&mut self, output: &mut impl Write) -> io::Result<()> {
        if self
            .pending_since
            .is_some_and(|started| started.elapsed() >= PENDING_FLUSH)
        {
            if let Some(transformer) = &mut self.transformer {
                output.write_all(&transformer.flush_pending())?;
                self.pending_since = transformer.has_pending().then(Instant::now);
            }
        }
        Ok(())
    }

    fn finish(&mut self, output: &mut impl Write) -> io::Result<()> {
        self.end(output)
    }

    fn sync_bypass(&mut self, output: &mut impl Write) -> io::Result<()> {
        let current = self
            .runtime
            .lock()
            .map_err(|_poisoned| io::Error::other("reading-session runtime state was poisoned"))?
            .bypass_tokens
            .clone();
        let was_empty = self.bypass_tokens.is_empty();
        self.bypass_tokens.extend(current);
        if was_empty && !self.bypass_tokens.is_empty() && self.transforming {
            self.flush_transformer(output)?;
            self.transforming = false;
        }
        Ok(())
    }
}

fn retire_ended_bypass(
    runtime: &Mutex<RuntimeState>,
    active: &mut HashSet<String>,
) -> io::Result<()> {
    let ended = {
        let mut state = runtime
            .lock()
            .map_err(|_poisoned| io::Error::other("reading-session runtime state was poisoned"))?;
        let ended = mem::take(&mut state.ended_bypass_tokens);
        for token in &ended {
            state.bypass_tokens.remove(token);
        }
        ended
    };
    active.retain(|token| !ended.contains(token));
    Ok(())
}

#[must_use]
fn command_is_excluded(metadata: &[u8], settings: &Settings) -> bool {
    if metadata.len() > MARKER_LIMIT
        || metadata
            .iter()
            .any(|byte| *byte == 0x1b || (*byte < 0x20 && *byte != b'\t') || *byte == 0x7f)
    {
        return true;
    }
    let Ok(encoded) = str::from_utf8(metadata) else {
        return true;
    };
    let decoded = decode_ansi_c_quote(encoded);
    let command_line = match decoded {
        Some(command) => command,
        None => match shell_words::split(encoded) {
            Ok(words) if words.len() == 1 => words[0].clone(),
            Ok(_) => encoded.to_owned(),
            Err(_) => return true,
        },
    };
    let Ok(words) = shell_words::split(&command_line) else {
        return true;
    };
    if words.is_empty() {
        return true;
    }
    if words.iter().any(|word| {
        matches!(
            word.as_str(),
            ";" | "|" | "||" | "&" | "&&" | ">" | ">>" | "<" | "<<"
        )
    }) {
        return true;
    }
    let mut index = 0;
    while let Some(word) = words.get(index).map(String::as_str) {
        match word {
            "command" | "builtin" | "exec" | "nohup" | "time" => index += 1,
            "env" => {
                index += 1;
                while let Some(word) = words.get(index) {
                    if word.starts_with('-') || word.contains('=') {
                        index += 1;
                    } else {
                        break;
                    }
                }
            }
            "sudo" => {
                index += 1;
                while let Some(option) = words.get(index) {
                    if option == "--" {
                        index += 1;
                        break;
                    }
                    if option.starts_with('-') {
                        index += 1;
                        if matches!(option.as_str(), "-u" | "-g" | "-h" | "-p") {
                            index = index.saturating_add(1);
                        }
                    } else {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
    let Some(executable) = words.get(index) else {
        return true;
    };
    let basename = executable.rsplit('/').next().unwrap_or(executable);
    basename == "bionic-reader"
        || settings
            .exclude_commands
            .iter()
            .any(|excluded| excluded.rsplit('/').next().unwrap_or(excluded) == basename)
}

// Bash 3.2's %q uses ANSI-C quoting for non-ASCII bytes under LC_ALL=C.
// Decode its finite escape alphabet as data; never evaluate shell metadata.
#[must_use]
fn decode_ansi_c_quote(encoded: &str) -> Option<String> {
    let bytes = encoded.strip_prefix("$'")?.strip_suffix('\'')?.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            output.push(bytes[index]);
            index += 1;
            continue;
        }
        index += 1;
        let escaped = *bytes.get(index)?;
        if (b'0'..=b'7').contains(&escaped) {
            let mut value = 0_u16;
            let mut digits = 0;
            while digits < 3 && index < bytes.len() && (b'0'..=b'7').contains(&bytes[index]) {
                value = value * 8 + u16::from(bytes[index] - b'0');
                index += 1;
                digits += 1;
            }
            output.push(u8::try_from(value).ok()?);
            continue;
        }
        output.push(match escaped {
            b'a' => 7,
            b'b' => 8,
            b'e' | b'E' => 27,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 11,
            b'\\' | b'\'' | b'"' => escaped,
            _ => return None,
        });
        index += 1;
    }
    String::from_utf8(output).ok()
}

#[derive(Default)]
struct Utf8Boundary {
    remaining: u8,
    next_min: u8,
    next_max: u8,
}

impl Utf8Boundary {
    #[must_use]
    fn consumes(&mut self, byte: u8) -> bool {
        if self.remaining > 0 {
            if (self.next_min..=self.next_max).contains(&byte) {
                self.remaining -= 1;
                self.next_min = 0x80;
                self.next_max = 0xbf;
                return true;
            }
            self.remaining = 0;
        }
        let (remaining, next_min, next_max) = match byte {
            0xc2..=0xdf => (1, 0x80, 0xbf),
            0xe0 => (2, 0xa0, 0xbf),
            0xe1..=0xec | 0xee..=0xef => (2, 0x80, 0xbf),
            0xed => (2, 0x80, 0x9f),
            0xf0 => (3, 0x90, 0xbf),
            0xf1..=0xf3 => (3, 0x80, 0xbf),
            0xf4 => (3, 0x80, 0x8f),
            _ => return false,
        };
        self.remaining = remaining;
        self.next_min = next_min;
        self.next_max = next_max;
        true
    }
}

#[must_use]
fn has_unsafe_control(bytes: &[u8], utf8_boundary: &mut Utf8Boundary) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if utf8_boundary.consumes(bytes[index]) {
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"\x1b[") {
            let start = index + 2;
            let Some(relative_end) = bytes[start..]
                .iter()
                .position(|byte| (0x40..=0x7e).contains(byte))
            else {
                return true;
            };
            let end = start + relative_end;
            if bytes[end] != b'm'
                && !transformer::is_presentation_neutral_control(&bytes[index..=end])
            {
                return true;
            }
            index = end + 1;
        } else if bytes[index..].starts_with(b"\x1b]") {
            let payload_start = index + 2;
            let mut end = payload_start;
            while end < bytes.len() {
                if bytes[end] == 0x07 || bytes[end..].starts_with(b"\x1b\\") {
                    break;
                }
                end += 1;
            }
            if end == bytes.len() || !transformer::is_safe_osc(&bytes[payload_start..end]) {
                return true;
            }
            index = end + if bytes[end] == 0x07 { 1 } else { 2 };
        } else if bytes
            .get(index..index + 3)
            .is_some_and(transformer::is_presentation_neutral_control)
        {
            index += 3;
        } else if matches!(bytes[index], 0x1b | 0x90 | 0x9b | 0x9c | 0x9d | 0x9e | 0x9f) {
            return true;
        } else {
            index += 1;
        }
    }
    false
}

#[must_use]
fn dcs_marker(token: &str, kind: &str) -> Vec<u8> {
    format!("\x1bP+BIONIC_READER;{token};{kind}\x1b\\").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::{
        BypassMarkerGuard, ChildCleanup, Marker, OutputEvent, OutputState, ParsedEvent,
        RuntimeState, Workers,
    };
    use crate::settings::Settings;
    use std::collections::HashSet;
    use std::io;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn dropping_workers_releases_a_blocked_output_sender_and_joins() {
        use std::sync::atomic::Ordering;
        use std::sync::mpsc;

        let (sender, receiver) = mpsc::sync_channel(0);
        let mut workers = Workers::new(receiver);
        let stop = Arc::clone(&workers.stop);
        let (started, ready) = mpsc::channel();
        let (completed, outcome) = mpsc::channel();
        workers.threads.push(thread::spawn(move || {
            started.send(()).expect("announce the worker is ready");
            let released = sender.send(OutputEvent::Closed).is_err();
            completed
                .send((released, stop.load(Ordering::Relaxed)))
                .expect("report shutdown behavior");
        }));
        ready.recv().expect("wait for the output worker");
        // The zero-capacity sender cannot complete until this drops the receiver.
        drop(workers);
        assert_eq!(
            outcome
                .recv()
                .expect("worker completed before teardown returned"),
            (true, true)
        );
    }

    #[test]
    fn dropping_child_cleanup_kills_and_reaps_a_running_shell() {
        use portable_pty::{CommandBuilder, PtySize};
        use std::process::Command;

        let pair = portable_pty::native_pty_system()
            .openpty(PtySize::default())
            .expect("open a test PTY");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "exec sleep 30"]);
        let child = pair
            .slave
            .spawn_command(command)
            .expect("start the test child");
        drop(pair.slave);
        let pid = child.process_id().expect("a live child has a PID");
        let cleanup = ChildCleanup {
            child,
            master: pair.master.as_ref(),
        };
        drop(cleanup);
        let status = Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .output()
            .expect("query whether the child still exists")
            .status;
        assert!(
            !status.success(),
            "startup cleanup must kill and reap the child"
        );
    }

    #[test]
    fn simultaneous_bypass_and_cleanup_failures_keep_the_primary_source() {
        let primary = crate::Error::with_source(
            "could not run bypass command".to_owned(),
            io::Error::new(io::ErrorKind::NotFound, "missing executable"),
        );
        let cleanup = crate::Error::with_source(
            "could not reach the active reading session".to_owned(),
            io::Error::new(io::ErrorKind::ConnectionRefused, "socket closed"),
        );
        let error = super::complete_bypass(Err(primary), Err(cleanup))
            .expect_err("both failures must be reported");
        assert_eq!(error.io_kind(), Some(io::ErrorKind::NotFound));
        let message = format!("{error:#}");
        assert!(message.contains("missing executable"));
        assert!(message.contains("bypass cleanup also failed"));
        assert!(message.contains("socket closed"));
    }

    #[test]
    fn bypass_cleanup_runs_on_an_early_error() {
        use std::io::{Read, Write};
        use std::net::Shutdown;
        use std::os::unix::net::UnixListener;

        let temporary = tempfile::tempdir().expect("create a socket directory");
        let socket = temporary.path().join("control.sock");
        let listener = UnixListener::bind(&socket).expect("bind the test control socket");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept cleanup request");
            let mut request = String::new();
            stream
                .read_to_string(&mut request)
                .expect("read cleanup request");
            stream.write_all(b"ok\n").expect("acknowledge cleanup");
            stream
                .shutdown(Shutdown::Write)
                .expect("finish the response");
            request
        });
        let result: Result<(), io::Error> = {
            let _guard = BypassMarkerGuard {
                socket,
                token: "abcd".to_owned(),
                marker_token: "test".to_owned(),
                terminal_marker: false,
                active: true,
            };
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "marker write failed",
            ))
        };
        assert_eq!(
            result.expect_err("simulate a failed marker write").kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(
            server.join().expect("join the test server"),
            "bypass-end:abcd"
        );
    }

    #[test]
    fn poisoned_runtime_rejects_command_output_and_bypass_updates() {
        let runtime = Arc::new(Mutex::new(RuntimeState {
            settings: Settings::default(),
            enabled: true,
            enabled_override: None,
            bypass_tokens: HashSet::new(),
            ended_bypass_tokens: HashSet::new(),
        }));
        let worker_runtime = Arc::clone(&runtime);
        thread::spawn(move || {
            let _guard = worker_runtime.lock().expect("lock the fresh runtime state");
            panic!("simulate a worker failure while updating the runtime");
        })
        .join()
        .expect_err("the simulated worker failure must poison the runtime");

        let mut state = OutputState::new(runtime);
        let mut output = Vec::new();
        let error = state
            .process(
                ParsedEvent::Marker(Marker::Begin(b"printf reading".to_vec())),
                &mut output,
            )
            .expect_err("a command must not read potentially inconsistent runtime state");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(
            output.is_empty(),
            "no command bytes may escape after runtime corruption"
        );

        let error = state
            .sync_bypass(&mut output)
            .expect_err("bypass updates must also reject potentially inconsistent runtime state");
        assert_eq!(error.kind(), io::ErrorKind::Other);
    }
    #[test]
    fn session_metadata_rejects_malformed_commands_and_recognizes_wrapped_exclusions() {
        let settings = Settings::default();
        for command in [
            b"".as_slice(),
            b"'unterminated",
            b"reading | less",
            b"reading && vim",
            b"\xff",
            b"bad\x1bcommand",
            b"env -i TERM=xterm /bin/vim",
            b"sudo -u root -- /bin/less",
            b"sudo -g staff -h host -p prompt /bin/vim",
            b"command builtin exec nohup time vim",
            b"env -i",
            b"sudo --",
            b"bionic-reader read file",
        ] {
            assert!(
                super::command_is_excluded(command, &settings),
                "{command:?}"
            );
        }
        assert!(!super::command_is_excluded(
            b"env -i A=B printf reading",
            &settings
        ));
        assert!(!super::command_is_excluded(
            b"sudo -- printf reading",
            &settings
        ));
        assert!(super::command_is_excluded(
            &vec![b'x'; super::MARKER_LIMIT + 1],
            &settings
        ));
    }

    #[test]
    fn shell_metadata_ansi_c_quoting_decodes_only_its_finite_data_alphabet() {
        for (escaped, expected) in [
            ("a", '\x07'),
            ("b", '\x08'),
            ("e", '\x1b'),
            ("E", '\x1b'),
            ("f", '\x0c'),
            ("n", '\n'),
            ("r", '\r'),
            ("t", '\t'),
            ("v", '\x0b'),
        ] {
            assert_eq!(
                super::decode_ansi_c_quote(&format!("$'\\{escaped}'")),
                Some(expected.to_string())
            );
        }
        assert_eq!(
            super::decode_ansi_c_quote(r"$'\101\12'"),
            Some("A\n".to_owned())
        );
        for text in [r"$'\q'", r"$'\777'", r"$'\377'", "plain"] {
            assert_eq!(super::decode_ansi_c_quote(text), None, "{text}");
        }
    }

    #[test]
    fn exit_status_mapping_preserves_shell_and_signal_conventions() {
        use std::os::unix::process::ExitStatusExt;
        for (name, signal) in [
            ("Hangup", 1),
            ("Interrupt", 2),
            ("Quit", 3),
            ("Terminated", 15),
        ] {
            assert_eq!(
                super::pty_exit_code(&portable_pty::ExitStatus::with_signal(name)),
                128 + signal
            );
            assert_eq!(super::signal_exit_code(signal), 128 + signal);
            assert_eq!(
                super::process_exit_code(std::process::ExitStatus::from_raw(signal)),
                128 + signal
            );
        }
        assert_eq!(super::signal_exit_code(12), 140);
        assert_eq!(
            super::pty_exit_code(&portable_pty::ExitStatus::with_signal("Unknown")),
            1
        );
        assert_eq!(
            super::pty_exit_code(&portable_pty::ExitStatus::with_exit_code(u32::MAX)),
            1
        );
        assert_eq!(
            super::process_exit_code(std::process::ExitStatus::from_raw(7 << 8)),
            7
        );
        assert!(
            super::complete_bypass(Ok(0), Err(crate::Error::new("cleanup".to_owned())))
                .expect_err("cleanup failure propagates")
                .to_string()
                .contains("cleanup")
        );
    }
    #[test]
    fn control_protocol_updates_state_and_rejects_invalid_utf8_requests_and_tokens() {
        use std::io::Write;
        use std::net::Shutdown;
        use std::os::unix::net::UnixStream;
        let state = Mutex::new(RuntimeState {
            settings: Settings::new(),
            enabled: true,
            enabled_override: None,
            bypass_tokens: HashSet::new(),
            ended_bypass_tokens: HashSet::new(),
        });
        let source = super::ReloadSource {
            config_path: None,
            overrides: crate::settings::Overrides::new(),
        };
        for request in [
            "off",
            "toggle",
            "on",
            "bypass-start:abcd",
            "bypass-end:abcd",
            "bypass-end:deadbeef",
        ] {
            let (mut client, mut server) = UnixStream::pair().expect("create protocol socket pair");
            client.write_all(request.as_bytes()).expect("send request");
            client.shutdown(Shutdown::Write).expect("finish request");
            super::handle_control_request(&mut server, &state, &source)
                .expect("apply protocol operation");
        }
        let state_guard = state.lock().expect("inspect control state");
        assert!(state_guard.enabled);
        assert_eq!(state_guard.enabled_override, Some(true));
        assert!(state_guard.ended_bypass_tokens.contains("abcd"));
        assert_eq!(state_guard.ended_bypass_tokens.len(), 1);
        drop(state_guard);
        for request in [
            b"unknown".as_slice(),
            b"bypass-start:!",
            b"bypass-end:",
            b"\xff",
        ] {
            let (mut client, mut server) =
                UnixStream::pair().expect("create invalid-request socket pair");
            client.write_all(request).expect("send invalid request");
            client
                .shutdown(Shutdown::Write)
                .expect("finish invalid request");
            assert!(
                super::handle_control_request(&mut server, &state, &source).is_err(),
                "{request:?}"
            );
        }
    }

    #[test]
    fn unsafe_terminal_guard_distinguishes_unicode_from_active_sequences() {
        for source in ["ࠀ", "퟿", "𐀀", "😀", "\u{10ffff}"] {
            assert!(!super::has_unsafe_control(
                source.as_bytes(),
                &mut super::Utf8Boundary::default()
            ));
        }
        for source in [
            b"\x1b[2J".as_slice(),
            b"\x1b[",
            b"\x1b]52;c;x\x07",
            b"\x1b]0;title",
            b"\x1bPunsafe\x1b\\",
            b"\x9b",
        ] {
            assert!(
                super::has_unsafe_control(source, &mut super::Utf8Boundary::default()),
                "{source:?}"
            );
        }
        for source in [
            b"\x1b]0;title\x07".as_slice(),
            b"\x1b]8;;https://example.test\x1b\\",
            b"\x1b(B",
            b"\x1b[?2004h",
        ] {
            assert!(
                !super::has_unsafe_control(source, &mut super::Utf8Boundary::default()),
                "{source:?}"
            );
        }
    }
    #[test]
    fn oversized_and_stalled_markers_recover_after_a_split_terminator() {
        let mut parser = super::MarkerParser::new("secret");
        let mut oversized = b"\x1bP".to_vec();
        oversized.extend(std::iter::repeat_n(b'x', super::MARKER_LIMIT));
        oversized.push(0x1b);
        let events = parser.push(&oversized);
        assert!(matches!(
            events.first(),
            Some(ParsedEvent::Marker(Marker::UnsafeDcs(_)))
        ));
        let events = parser.push(b"\\\x1bP+BIONIC_READER;secret;BEGIN\x1b\\");
        assert!(events.iter().any(
            |event| matches!(event, ParsedEvent::Marker(Marker::Begin(bytes)) if bytes.is_empty())
        ));
        assert!(parser.finish().is_empty());
        assert!(parser.push(b"\x1bPstalled").is_empty());
        parser.pending_since = Some(
            std::time::Instant::now()
                .checked_sub(super::PENDING_FLUSH)
                .expect("clock supports the test timeout"),
        );
        assert!(matches!(
            parser.flush_pending().first(),
            Some(ParsedEvent::Marker(Marker::UnsafeDcs(_)))
        ));
        assert!(!parser.push(b"data\x1b").is_empty());
        let events = parser.push(b"\\\x1bP+BIONIC_READER;secret;END:\x1b\\");
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ParsedEvent::Marker(Marker::End)))
        );
        for payload in [
            b"+BIONIC_READER;secret;BYPASS+!".as_slice(),
            b"+BIONIC_READER;secret;BYPASS-",
            b"+BIONIC_READER;secret;OTHER",
        ] {
            assert!(parser.parse_marker(payload).is_none());
        }
        assert!(parser.push(b"\x1bPunfinished").is_empty());
        assert!(matches!(
            parser.finish().first(),
            Some(ParsedEvent::Marker(Marker::UnsafeDcs(_)))
        ));
    }

    #[test]
    fn operating_system_helpers_return_typed_errors_for_invalid_descriptors() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        assert!(super::TerminalMode::try_new(-1).is_err());
        super::read_descriptor(-1, &mut [0]).expect_err("invalid descriptor must fail");
        let (reader, mut writer) =
            std::os::unix::net::UnixStream::pair().expect("create descriptor pair");
        assert!(!super::wait_for_descriptor(reader.as_raw_fd(), 0).expect("poll idle descriptor"));
        writer.write_all(b"x").expect("make descriptor readable");
        assert!(
            super::wait_for_descriptor(reader.as_raw_fd(), 0).expect("poll readable descriptor")
        );
        let mut byte = [0];
        assert_eq!(
            super::read_descriptor(reader.as_raw_fd(), &mut byte).expect("read descriptor"),
            1
        );
        assert_eq!(byte, [b'x']);
        super::signal_process(0, 0);
        reader
            .shutdown(std::net::Shutdown::Read)
            .expect("close reader");
    }

    #[test]
    fn control_client_handles_rejection_malformed_responses_and_missing_sockets() {
        use std::io::{Read, Write};
        use std::net::Shutdown;
        let directory = tempfile::tempdir().expect("create control socket directory");
        let missing = directory.path().join("missing");
        assert_eq!(
            super::send_control_request(&missing, "on")
                .expect_err("socket is absent")
                .io_kind(),
            Some(io::ErrorKind::NotFound)
        );
        for (index, response) in [
            b"ok\n".as_slice(),
            b"error:fixture rejection\n",
            b"unexpected",
            b"\xff",
            b"okay\n",
            b"ok\nerror:request rejected\n",
        ]
        .into_iter()
        .enumerate()
        {
            let socket = directory.path().join(format!("control-{index}"));
            let listener =
                std::os::unix::net::UnixListener::bind(&socket).expect("bind response fixture");
            let server = thread::spawn(move || {
                let (mut connection, _) = listener.accept().expect("accept control client");
                let mut request = String::new();
                connection
                    .read_to_string(&mut request)
                    .expect("read request");
                assert_eq!(request, "on");
                connection
                    .write_all(response)
                    .expect("write fixture response");
                connection
                    .shutdown(Shutdown::Write)
                    .expect("finish fixture response");
            });
            let result = super::send_control_request(&socket, "on");
            if index == 0 {
                result.expect("accept success response");
            } else {
                let error = result.expect_err("reject invalid response");
                if index == 1 {
                    assert_eq!(format!("{error:#}"), "fixture rejection");
                }
            }
            server.join().expect("join response fixture");
        }
    }

    #[test]
    fn shell_startup_reports_missing_files_invalid_paths_and_unwritable_destinations() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        use std::path::PathBuf;

        let directory = tempfile::tempdir().expect("create startup directory");
        let integration = directory.path().join("integration.sh");
        let invalid = PathBuf::from(OsString::from_vec(vec![0xff]));
        for (kind, source, destination, message) in [
            (
                "bash",
                invalid.as_path(),
                directory.path(),
                "integration path",
            ),
            (
                "bash",
                integration.as_path(),
                invalid.as_path(),
                "session path",
            ),
            (
                "bash",
                integration.as_path(),
                directory.path(),
                "read Bash integration",
            ),
            (
                "unsupported",
                integration.as_path(),
                directory.path(),
                "unsupported shell",
            ),
        ] {
            let mut command = portable_pty::CommandBuilder::new("/bin/sh");
            let error = super::configure_shell(&mut command, kind, source, destination)
                .expect_err("invalid startup parameters must fail");
            assert!(format!("{error:#}").contains(message));
        }
        std::fs::write(&integration, "# test integration\n").expect("write integration fixture");
        // A regular file cannot contain the generated startup files, even for root.
        for (kind, message) in [
            ("bash", "prepare Bash startup"),
            ("zsh", "prepare Zsh startup"),
        ] {
            let mut command = portable_pty::CommandBuilder::new("/bin/sh");
            let error = super::configure_shell(&mut command, kind, &integration, &integration)
                .expect_err("startup destination is not a directory");
            assert!(format!("{error:#}").contains(message));
            assert!(error.io_kind().is_some());
        }
    }

    #[test]
    fn bypass_child_reports_spawn_failures_normal_status_and_signal_status() {
        use std::ffi::{OsStr, OsString};
        let missing = tempfile::tempdir().expect("create missing-program parent");
        let error = super::run_bypass_command(missing.path().join("missing").as_os_str(), &[])
            .expect_err("missing bypass executable must fail");
        assert_eq!(error.io_kind(), Some(io::ErrorKind::NotFound));
        assert!(format!("{error:#}").contains("could not run bypass command"));
        for (script, expected) in [("exit 7", 7), ("kill -TERM $$", 143)] {
            let status = super::run_bypass_command(
                OsStr::new("/bin/sh"),
                &[OsString::from("-c"), OsString::from(script)],
            )
            .expect("run bypass subprocess");
            assert_eq!(status, expected);
        }
    }

    fn enabled_runtime() -> Arc<Mutex<RuntimeState>> {
        Arc::new(Mutex::new(RuntimeState {
            settings: Settings::default(),
            enabled: true,
            enabled_override: None,
            bypass_tokens: HashSet::new(),
            ended_bypass_tokens: HashSet::new(),
        }))
    }

    #[test]
    fn session_rendering_propagates_output_errors_at_boundaries_and_timed_flushes() {
        use std::io::Write;
        use std::time::Instant;

        struct ClosedOutput;
        impl Write for ClosedOutput {
            fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "downstream closed",
                ))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        for boundary in ["end", "begin", "bypass", "unsafe", "timer", "data"] {
            let mut state = OutputState::new(enabled_runtime());
            let mut prefix = Vec::new();
            state
                .begin(b"printf reading", &mut prefix)
                .expect("begin command");
            state
                .write_data(b"reading", &mut prefix)
                .expect("buffer an unfinished word");
            assert!(prefix.is_empty());
            let mut closed = ClosedOutput;
            let result = match boundary {
                "end" => state.end(&mut closed),
                "begin" => state.begin(b"printf next", &mut closed),
                "bypass" => state.bypass_push("abcd".to_owned(), &mut closed),
                "unsafe" => state.process(
                    ParsedEvent::Marker(Marker::UnsafeDcs(b"\x1bPunsafe\x1b\\".to_vec())),
                    &mut closed,
                ),
                "timer" => {
                    state.pending_since = Some(
                        Instant::now()
                            .checked_sub(super::PENDING_FLUSH)
                            .expect("clock supports the flush deadline"),
                    );
                    state.flush_pending(&mut closed)
                }
                _ => state.write_data(b" next ", &mut closed),
            };
            assert_eq!(
                result.expect_err("output failure must propagate").kind(),
                io::ErrorKind::BrokenPipe,
                "{boundary}"
            );
        }
    }

    #[test]
    fn control_worker_rejects_corrupted_state_and_can_shutdown_after_a_bad_request() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let directory = tempfile::tempdir().expect("create control worker directory");
        let socket = directory.path().join("control.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind worker socket");
        listener
            .set_nonblocking(true)
            .expect("configure worker socket");
        let state = enabled_runtime();
        let poisoned = Arc::clone(&state);
        thread::spawn(move || {
            let _lock = poisoned.lock().expect("lock fresh state");
            panic!("simulate a failed state update");
        })
        .join()
        .expect_err("poison the runtime");
        let stop = Arc::new(AtomicBool::new(false));
        let worker = super::spawn_control_server(
            listener,
            state,
            super::ReloadSource {
                config_path: None,
                overrides: crate::settings::Overrides::default(),
            },
            Arc::clone(&stop),
        )
        .expect("spawn worker");
        let result = super::send_control_request(&socket, "on");
        stop.store(true, Ordering::Relaxed);
        worker
            .join()
            .expect("worker must stop without a blocked request");
        let error = result.expect_err("corrupted state must reject the request");
        assert!(format!("{error:#}").contains("session control state is unavailable"));
    }

    #[test]
    fn control_worker_serializes_concurrent_updates_and_reports_protocol_errors() {
        use std::os::unix::net::UnixListener;
        use std::sync::Barrier;
        use std::sync::atomic::{AtomicBool, Ordering};

        struct WorkerGuard {
            stop: Arc<AtomicBool>,
            worker: Option<thread::JoinHandle<()>>,
        }

        impl WorkerGuard {
            fn stop_and_join(mut self) {
                self.stop.store(true, Ordering::Relaxed);
                self.worker
                    .take()
                    .expect("worker handle is present until shutdown")
                    .join()
                    .expect("control worker must stop after its clients finish");
            }
        }

        impl Drop for WorkerGuard {
            fn drop(&mut self) {
                self.stop.store(true, Ordering::Relaxed);
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
            }
        }

        let directory = tempfile::tempdir().expect("create control worker directory");
        let socket = directory.path().join("control.sock");
        let listener = UnixListener::bind(&socket).expect("bind control worker socket");
        listener
            .set_nonblocking(true)
            .expect("configure nonblocking control socket");
        let state = enabled_runtime();
        let stop = Arc::new(AtomicBool::new(false));
        let worker = super::spawn_control_server(
            listener,
            Arc::clone(&state),
            super::ReloadSource {
                config_path: None,
                overrides: crate::settings::Overrides::default(),
            },
            Arc::clone(&stop),
        )
        .expect("spawn control worker");
        let worker = WorkerGuard {
            stop: Arc::clone(&stop),
            worker: Some(worker),
        };

        let barrier = Arc::new(Barrier::new(8));
        let clients = (0..8)
            .map(|_| {
                let socket = socket.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    super::send_control_request(&socket, "on")
                })
            })
            .collect::<Vec<_>>();
        for client in clients {
            client
                .join()
                .expect("control client must finish")
                .expect("concurrent idempotent control updates must succeed");
        }

        let error = super::send_control_request(&socket, "unknown")
            .expect_err("the worker must reject an unsupported protocol command");
        assert!(format!("{error:#}").contains("invalid session control request"));
        let state = state.lock().expect("inspect the shared session state");
        assert!(state.enabled);
        assert_eq!(state.enabled_override, Some(true));
        drop(state);

        worker.stop_and_join();
    }
}
