//! Interactive PTY reading sessions and per-session controls.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::RawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus as ProcessExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, ExitStatus, PtySize, native_pty_system};
use signal_hook::consts::signal::*;
use signal_hook::iterator::{Handle as SignalHandle, Signals};

use crate::settings::{Overrides, Settings, Style};
use crate::transform::Transformer;

const MARKER_LIMIT: usize = 64 * 1024;
const PENDING_FLUSH: Duration = Duration::from_millis(50);
const CONTROL_SOCKET_ENV: &str = "BIONIC_READER_CONTROL_SOCKET";
const SESSION_ENV: &str = "BIONIC_READER_SESSION";
const MARKER_TOKEN_ENV: &str = "BIONIC_READER_MARKER_TOKEN";

/// Run a login shell inside a pseudo-terminal and transform marked command output.
pub fn run(
    shell: Option<&str>,
    settings: Settings,
    config_path: Option<PathBuf>,
    overrides: Overrides,
) -> Result<i32, String> {
    if !is_terminal(libc::STDIN_FILENO) || !is_terminal(libc::STDOUT_FILENO) {
        return Err("reading sessions require a terminal on both stdin and stdout".to_owned());
    }

    let (shell_kind, shell_program) = resolve_shell(shell)?;
    let terminal_mode = TerminalMode::raw(libc::STDIN_FILENO)
        .map_err(|error| format!("could not enter terminal raw mode: {error}"))?;
    let session_temp = tempfile::Builder::new()
        .prefix("bionic-reader-session-")
        .tempdir()
        .map_err(|error| format!("could not create session directory: {error}"))?;
    let marker_token = random_token()?;
    let socket_path = session_temp.path().join("control.sock");
    let listener = UnixListener::bind(&socket_path)
        .map_err(|error| format!("could not create session control socket: {error}"))?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("could not protect session control socket: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("could not configure session control socket: {error}"))?;

    let initial_enabled = settings.style != Style::Never;
    let runtime = Arc::new(Mutex::new(RuntimeState {
        settings,
        enabled: initial_enabled,
        enabled_override: None,
        bypass_tokens: HashSet::new(),
        ended_bypass_tokens: HashSet::new(),
    }));
    let stop_control = Arc::new(AtomicBool::new(false));
    let control_thread = spawn_control_server(
        listener,
        runtime.clone(),
        ReloadSource {
            config_path,
            overrides,
        },
        stop_control.clone(),
    );

    let integration_path = session_temp
        .path()
        .join(format!("integration.{shell_kind}"));
    fs::write(&integration_path, crate::shell::init(shell_kind)?)
        .map_err(|error| format!("could not prepare shell integration: {error}"))?;

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(current_terminal_size())
        .map_err(|error| format!("could not create reading-session PTY: {error}"))?;
    let master = pair.master;
    let reader = master
        .try_clone_reader()
        .map_err(|error| format!("could not read from the reading-session PTY: {error}"))?;
    let reader_fd = master
        .as_raw_fd()
        .ok_or_else(|| "reading-session PTY has no file descriptor".to_owned())?;
    let writer = master
        .take_writer()
        .map_err(|error| format!("could not write to the reading-session PTY: {error}"))?;

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

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("could not start {shell_kind} reading session: {error}"))?;
    drop(pair.slave);

    let (output_sender, output_receiver) = mpsc::sync_channel(8);
    let stop_reader = Arc::new(AtomicBool::new(false));
    let reader_stop = stop_reader.clone();
    let reader_thread = thread::spawn(move || {
        let mut reader = reader;
        let mut buffer = [0_u8; 8192];
        while !reader_stop.load(Ordering::Relaxed) {
            let mut descriptor = libc::pollfd {
                fd: reader_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: descriptor is initialized and points to this thread's stack for the call.
            let ready = unsafe { libc::poll(&mut descriptor, 1, 100) };
            if ready == 0 {
                continue;
            }
            if ready < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break;
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
    });

    let stop_input = Arc::new(AtomicBool::new(false));
    let input_thread = spawn_input_pump(writer, stop_input.clone());
    let (signal_sender, signal_receiver) = mpsc::channel();
    let (signal_handle, signal_thread) = spawn_signal_pump(signal_sender)
        .map_err(|error| format!("could not install session signal handlers: {error}"))?;

    let mut parser = MarkerParser::new(&marker_token);
    let mut output_state = OutputState::new(runtime.clone());
    let mut output = io::stdout().lock();
    let mut child_status = None;
    let mut output_closed = false;
    let mut loop_error = None;
    let mut termination_signal = None;
    let mut shutdown_deadline = None;
    let mut termination_escalated = false;

    while !(child_status.is_some() && output_closed) {
        for signal in signal_receiver.try_iter() {
            if signal == SIGWINCH {
                if let Err(error) = master.resize(current_terminal_size()) {
                    loop_error = Some(format!("could not resize reading-session PTY: {error}"));
                    break;
                }
            } else {
                forward_to_foreground_group(master.as_ref(), child.process_id(), signal);
                if signal == SIGTERM || signal == SIGHUP {
                    termination_signal.get_or_insert(signal);
                    if let Some(pid) = child.process_id() {
                        signal_process_group(pid as libc::pid_t, signal);
                    }
                    shutdown_deadline
                        .get_or_insert_with(|| Instant::now() + Duration::from_millis(250));
                }
            }
        }
        if loop_error.is_some() {
            let _ = child.kill();
            break;
        }

        match output_receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(OutputEvent::Bytes(bytes)) => {
                for event in parser.push(&bytes) {
                    if let Err(error) = output_state.process(event, &mut output) {
                        loop_error =
                            Some(format!("could not write reading-session output: {error}"));
                        let _ = child.kill();
                        break;
                    }
                }
                if loop_error.is_some() {
                    break;
                }
            }
            Ok(OutputEvent::Closed) => output_closed = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => output_closed = true,
        }

        if child_status.is_none() {
            match child.try_wait() {
                Ok(status) => child_status = status,
                Err(error) => {
                    loop_error = Some(format!("could not wait for reading session: {error}"));
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
                loop_error = Some(format!("could not flush partial terminal control: {error}"));
                let _ = child.kill();
                break;
            }
        }
        if loop_error.is_some() {
            break;
        }
        if let Err(error) = output_state.flush_pending(&mut output) {
            loop_error = Some(format!("could not flush reading-session output: {error}"));
            let _ = child.kill();
            break;
        }
        // Prompts, control sequences, and interactive programs often do not
        // end their output with a newline. Rust's stdout line buffer must be
        // flushed at each PTY batch so the terminal can display them now.
        if let Err(error) = output.flush() {
            loop_error = Some(format!("could not flush terminal output: {error}"));
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

    stop_input.store(true, Ordering::Relaxed);
    stop_control.store(true, Ordering::Relaxed);
    signal_handle.close();
    let _ = input_thread.join();
    stop_reader.store(true, Ordering::Relaxed);
    drop(output_receiver);
    let _ = reader_thread.join();
    let _ = control_thread.join();
    let _ = signal_thread.join();

    if let Some(error) = loop_error {
        drop(output);
        drop(terminal_mode);
        return Err(error);
    }
    for event in parser.finish() {
        output_state
            .process(event, &mut output)
            .map_err(|error| format!("could not finish reading-session output: {error}"))?;
    }
    output_state
        .finish(&mut output)
        .map_err(|error| format!("could not finish reading-session output: {error}"))?;
    output
        .flush()
        .map_err(|error| format!("could not flush terminal output: {error}"))?;
    drop(output);
    drop(terminal_mode);

    let status = child_status.unwrap_or_else(|| ExitStatus::with_exit_code(1));
    Ok(termination_signal
        .map(signal_exit_code)
        .unwrap_or_else(|| pty_exit_code(&status)))
}

/// Change the active reading-session control state.
pub fn control(command: &str) -> Result<(), String> {
    match command {
        "on" | "off" | "toggle" | "reload" => {}
        _ => return Err(format!("unknown reading-session control `{command}`")),
    }
    let socket = std::env::var_os(CONTROL_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("`bionic-reader {command}` requires an active reading session"))?;
    send_control_request(&socket, command)
}

/// Run a command with session transformation suspended until it completes.
pub fn bypass(command: &[OsString]) -> Result<i32, String> {
    let Some(program) = command.first() else {
        return Err("usage: bionic-reader bypass -- <command> [args ...]".to_owned());
    };
    if std::env::var_os(SESSION_ENV).as_deref() != Some(OsStr::new("1")) {
        return Err("`bionic-reader bypass` requires an active reading session".to_owned());
    }
    let marker_token = std::env::var(MARKER_TOKEN_ENV)
        .map_err(|_| "the active reading session has no marker token".to_owned())?;
    let bypass_id = random_token()?;
    let socket = std::env::var_os(CONTROL_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| "the active reading session has no control socket".to_owned())?;
    send_control_request(&socket, &format!("bypass-start:{bypass_id}"))?;
    let terminal_marker = is_terminal(libc::STDOUT_FILENO);
    if terminal_marker {
        write_marker(&dcs_marker(&marker_token, &format!("BYPASS+{bypass_id}")))
            .map_err(|error| format!("could not start bypass: {error}"))?;
    }

    let mut marker_guard = BypassMarkerGuard {
        socket,
        token: bypass_id,
        marker_token,
        terminal_marker,
        active: true,
    };
    let status = run_bypass_command(program, &command[1..]);
    marker_guard.finish();
    status
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
    fn finish(&mut self) {
        if self.active {
            if self.terminal_marker {
                let marker = dcs_marker(&self.marker_token, &format!("BYPASS-{}", self.token));
                let _ = write_marker(&marker);
            }
            let _ = send_control_request(&self.socket, &format!("bypass-end:{}", self.token));
            self.active = false;
        }
    }
}

impl Drop for BypassMarkerGuard {
    fn drop(&mut self) {
        self.finish();
    }
}

fn run_bypass_command(program: &OsStr, arguments: &[OsString]) -> Result<i32, String> {
    let mut signals = Signals::new([SIGINT, SIGHUP, SIGQUIT, SIGTERM])
        .map_err(|error| format!("could not install bypass signal handlers: {error}"))?;
    let signal_handle = signals.handle();
    let (signal_sender, signal_receiver) = mpsc::channel();
    let signal_thread = thread::spawn(move || {
        for signal in signals.forever() {
            if signal_sender.send(signal).is_err() {
                break;
            }
        }
    });
    let mut child = Command::new(program)
        .args(arguments)
        .spawn()
        .map_err(|error| format!("could not run bypass command: {error}"))?;
    let status = loop {
        for signal in signal_receiver.try_iter() {
            // SAFETY: child.id() is a live process ID and the signal is from signal-hook.
            unsafe { libc::kill(child.id() as libc::pid_t, signal) };
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                signal_handle.close();
                let _ = signal_thread.join();
                return Err(format!("could not wait for bypass command: {error}"));
            }
        }
    };
    signal_handle.close();
    let _ = signal_thread.join();
    Ok(process_exit_code(status))
}

fn send_control_request(socket: &Path, request: &str) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("could not reach the active reading session: {error}"))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("could not send session control: {error}"))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| format!("could not finish session control request: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("could not read session control response: {error}"))?;
    if response.starts_with("ok") {
        Ok(())
    } else {
        Err(response
            .strip_prefix("error:")
            .unwrap_or("reading session rejected the control request")
            .trim()
            .to_owned())
    }
}

fn process_exit_code(status: ProcessExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| status.signal().map(|signal| 128 + signal).unwrap_or(1))
}

fn pty_exit_code(status: &ExitStatus) -> i32 {
    if let Some(signal) = status.signal() {
        match signal {
            "Hangup" => 129,
            "Interrupt" => 130,
            "Quit" => 131,
            "Terminated" => 143,
            _ => status.exit_code() as i32,
        }
    } else {
        status.exit_code() as i32
    }
}

fn signal_exit_code(signal: i32) -> i32 {
    match signal {
        SIGHUP => 129,
        SIGINT => 130,
        SIGQUIT => 131,
        SIGTERM => 143,
        _ => 128 + signal,
    }
}

fn forward_to_foreground_group(
    master: &dyn portable_pty::MasterPty,
    fallback_pid: Option<u32>,
    signal: i32,
) {
    let group = master
        .process_group_leader()
        .or_else(|| fallback_pid.map(|pid| pid as libc::pid_t));
    if let Some(group) = group {
        signal_process_group(group, signal);
    }
}

fn signal_process_group(group: libc::pid_t, signal: i32) {
    if group > 0 {
        // SAFETY: the negative pid targets the process group returned by tcgetpgrp
        // or the child process-group leader created by portable-pty.
        unsafe { libc::kill(-group, signal) };
    }
}

fn kill_session_groups(master: &dyn portable_pty::MasterPty, shell_pid: u32) {
    if let Some(foreground) = master.process_group_leader() {
        if foreground != shell_pid as libc::pid_t {
            signal_process_group(foreground, SIGKILL);
        }
    }
    signal_process_group(shell_pid as libc::pid_t, SIGKILL);
}

#[derive(Clone)]
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
    fn load(&self) -> Result<Settings, String> {
        Settings::load(self.config_path.as_deref(), &self.overrides)
    }
}

fn spawn_control_server(
    listener: UnixListener,
    state: Arc<Mutex<RuntimeState>>,
    reload_source: ReloadSource,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let result = handle_control_request(&mut stream, &state, &reload_source);
                    let response = match result {
                        Ok(()) => "ok\n".to_owned(),
                        Err(error) => format!("error:{error}\n"),
                    };
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    })
}

fn handle_control_request(
    stream: &mut UnixStream,
    state: &Arc<Mutex<RuntimeState>>,
    reload_source: &ReloadSource,
) -> Result<(), String> {
    let mut request = String::new();
    stream
        .take(64)
        .read_to_string(&mut request)
        .map_err(|error| format!("could not read control request: {error}"))?;
    let request = request.trim();
    let mut state = state
        .lock()
        .map_err(|_| "session control state is unavailable".to_owned())?;
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
        "reload" => {
            let refreshed = reload_source.load()?;
            state.settings = refreshed;
            if state.enabled_override.is_none() {
                state.enabled = state.settings.style != Style::Never;
            }
        }
        request if request.starts_with("bypass-start:") => {
            let token = &request["bypass-start:".len()..];
            if !is_marker_id(token.as_bytes()) {
                return Err("invalid bypass token".to_owned());
            }
            state.bypass_tokens.insert(token.to_owned());
        }
        request if request.starts_with("bypass-end:") => {
            let token = &request["bypass-end:".len()..];
            if !is_marker_id(token.as_bytes()) {
                return Err("invalid bypass token".to_owned());
            }
            if state.bypass_tokens.contains(token) {
                state.ended_bypass_tokens.insert(token.to_owned());
            }
        }
        _ => return Err("invalid session control request".to_owned()),
    }
    Ok(())
}

#[derive(Debug)]
enum OutputEvent {
    Bytes(Vec<u8>),
    Closed,
}

fn spawn_input_pump(mut writer: Box<dyn Write + Send>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while !stop.load(Ordering::Relaxed) {
            let mut descriptor = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: descriptor is initialized and points to this thread's stack for the call.
            let ready = unsafe { libc::poll(&mut descriptor, 1, 100) };
            if ready < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            if ready == 0 {
                continue;
            }
            // SAFETY: buffer is writable and STDIN_FILENO is the session's inherited tty.
            let count =
                unsafe { libc::read(libc::STDIN_FILENO, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count <= 0 {
                break;
            }
            if writer.write_all(&buffer[..count as usize]).is_err() || writer.flush().is_err() {
                break;
            }
        }
    })
}

fn spawn_signal_pump(sender: mpsc::Sender<i32>) -> io::Result<(SignalHandle, JoinHandle<()>)> {
    let mut signals = Signals::new([SIGWINCH, SIGHUP, SIGINT, SIGQUIT, SIGTERM])?;
    let handle = signals.handle();
    let thread = thread::spawn(move || {
        for signal in signals.forever() {
            if sender.send(signal).is_err() {
                break;
            }
        }
    });
    Ok((handle, thread))
}

struct TerminalMode {
    fd: RawFd,
    original: libc::termios,
}

impl TerminalMode {
    fn raw(fd: RawFd) -> io::Result<Self> {
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
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

fn is_terminal(fd: RawFd) -> bool {
    // SAFETY: isatty only inspects the file descriptor and does not retain it.
    unsafe { libc::isatty(fd) == 1 }
}

fn current_terminal_size() -> PtySize {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    // SAFETY: size is a valid winsize output buffer for TIOCGWINSZ.
    let result = unsafe {
        libc::ioctl(
            libc::STDOUT_FILENO,
            libc::TIOCGWINSZ as _,
            size.as_mut_ptr(),
        )
    };
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

fn random_token() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    let mut random = fs::File::open("/dev/urandom")
        .map_err(|error| format!("could not access system randomness: {error}"))?;
    random
        .read_exact(&mut bytes)
        .map_err(|error| format!("could not create a session token: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn resolve_shell(shell: Option<&str>) -> Result<(&'static str, OsString), String> {
    let requested = shell
        .map(OsString::from)
        .or_else(|| std::env::var_os("SHELL"))
        .unwrap_or_else(|| OsString::from("/bin/bash"));
    let requested_string = requested.to_string_lossy();
    let kind = crate::shell::normalize_shell(&requested_string)?;
    Ok((kind, requested))
}

fn configure_shell(
    command: &mut CommandBuilder,
    shell_kind: &str,
    integration_path: &Path,
    temp_dir: &Path,
) -> Result<(), String> {
    let integration_path = integration_path
        .to_str()
        .ok_or_else(|| "temporary shell integration path is not valid UTF-8".to_owned())?;
    let temp_dir = temp_dir
        .to_str()
        .ok_or_else(|| "temporary session path is not valid UTF-8".to_owned())?;
    command.env("BIONIC_READER_INTEGRATION", integration_path);
    match shell_kind {
        "bash" => {
            let user_rc = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".bashrc");
            command.env("BIONIC_READER_USER_RC", user_rc);
            let rc_path = Path::new(temp_dir).join("bashrc");
            let integration = fs::read_to_string(integration_path)
                .map_err(|error| format!("could not read Bash integration: {error}"))?;
            let startup = format!(
                "if [[ -r ${{BIONIC_READER_USER_RC-}} ]]; then source \"$BIONIC_READER_USER_RC\"; fi\n{integration}"
            );
            fs::write(&rc_path, startup)
                .map_err(|error| format!("could not prepare Bash startup: {error}"))?;
            command.args(["--noprofile", "--rcfile"]);
            command.arg(rc_path);
            command.arg("-i");
        }
        "zsh" => {
            let user_zdotdir = std::env::var_os("ZDOTDIR")
                .or_else(|| std::env::var_os("HOME"))
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
        _ => return Err(format!("unsupported shell `{shell_kind}`")),
    }
    Ok(())
}

fn write_zsh_startup(temp_dir: &str) -> Result<(), String> {
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
        fs::write(path, script)
            .map_err(|error| format!("could not prepare Zsh startup: {error}"))?;
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
    fn new(token: &str) -> Self {
        Self {
            pending: Vec::new(),
            token_prefix: format!("+BIONIC_READER;{token};").into_bytes(),
            discard_dcs: false,
            pending_since: None,
        }
    }

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
                    let bytes = std::mem::take(&mut self.pending);
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

    fn flush_pending(&mut self) -> Vec<ParsedEvent> {
        let stalled = self
            .pending_since
            .is_some_and(|since| since.elapsed() >= PENDING_FLUSH);
        if !stalled || self.pending.is_empty() {
            return Vec::new();
        }
        let bytes = std::mem::take(&mut self.pending);
        if bytes.starts_with(b"\x1bP") {
            self.discard_dcs = true;
        }
        self.pending_since = None;
        vec![ParsedEvent::Marker(Marker::UnsafeDcs(bytes))]
    }

    fn finish(&mut self) -> Vec<ParsedEvent> {
        if self.pending.is_empty() {
            Vec::new()
        } else {
            vec![ParsedEvent::Marker(Marker::UnsafeDcs(std::mem::take(
                &mut self.pending,
            )))]
        }
    }

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

fn is_marker_id(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= 64 && bytes.iter().all(u8::is_ascii_hexdigit)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

struct OutputState {
    runtime: Arc<Mutex<RuntimeState>>,
    transformer: Option<Transformer>,
    in_command: bool,
    command_allowed: bool,
    command_enabled: bool,
    transforming: bool,
    unsafe_interval: bool,
    bypass_tokens: HashSet<String>,
    command_settings: Option<Settings>,
    pending_since: Option<Instant>,
    utf8_boundary: Utf8Boundary,
}

impl OutputState {
    fn new(runtime: Arc<Mutex<RuntimeState>>) -> Self {
        Self {
            runtime,
            transformer: None,
            in_command: false,
            command_allowed: false,
            command_enabled: false,
            transforming: false,
            unsafe_interval: false,
            bypass_tokens: HashSet::new(),
            command_settings: None,
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
            ParsedEvent::Marker(Marker::BypassPop(token)) => self.bypass_pop(&token, output),
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
        retire_ended_bypass(&self.runtime, &mut self.bypass_tokens);
        let snapshot = self
            .runtime
            .lock()
            .map(|state| state.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        self.command_allowed = !command_is_excluded(metadata, &snapshot.settings);
        self.command_settings = Some(snapshot.settings.clone());
        self.in_command = true;
        self.command_enabled = snapshot.enabled;
        self.unsafe_interval = false;
        self.bypass_tokens = snapshot.bypass_tokens.clone();
        self.transforming = snapshot.enabled
            && self.command_allowed
            && self.bypass_tokens.is_empty()
            && snapshot.bypass_tokens.is_empty();
        self.transformer = self
            .transforming
            .then(|| Transformer::new(snapshot.settings, true));
        self.pending_since = None;
        self.utf8_boundary = Utf8Boundary::default();
        Ok(())
    }

    fn end(&mut self, output: &mut impl Write) -> io::Result<()> {
        if self.in_command {
            self.flush_transformer(output)?;
        }
        self.transformer = None;
        self.command_settings = None;
        self.pending_since = None;
        self.in_command = false;
        self.command_allowed = false;
        self.command_enabled = false;
        self.transforming = false;
        self.unsafe_interval = false;
        retire_ended_bypass(&self.runtime, &mut self.bypass_tokens);
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

    fn bypass_pop(&mut self, token: &str, _output: &mut impl Write) -> io::Result<()> {
        // Keep bypass active until the trusted shell END marker. This also
        // covers data emitted by the rest of a pipeline after the bypassed
        // process itself has exited.
        let _ = token;
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
            .map(|state| state.bypass_tokens.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().bypass_tokens.clone());
        let was_empty = self.bypass_tokens.is_empty();
        self.bypass_tokens.extend(current);
        if was_empty && !self.bypass_tokens.is_empty() && self.transforming {
            self.flush_transformer(output)?;
            self.transforming = false;
        }
        Ok(())
    }
}

fn retire_ended_bypass(runtime: &Arc<Mutex<RuntimeState>>, active: &mut HashSet<String>) {
    let ended = {
        let mut state = runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let ended = std::mem::take(&mut state.ended_bypass_tokens);
        for token in &ended {
            state.bypass_tokens.remove(token);
        }
        ended
    };
    active.retain(|token| !ended.contains(token));
}

fn command_is_excluded(metadata: &[u8], settings: &Settings) -> bool {
    if metadata.len() > MARKER_LIMIT
        || metadata
            .iter()
            .any(|byte| *byte == 0x1b || (*byte < 0x20 && *byte != b'\t') || *byte == 0x7f)
    {
        return true;
    }
    let Ok(encoded) = std::str::from_utf8(metadata) else {
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
    let words = match shell_words::split(&command_line) {
        Ok(words) => words,
        Err(_) => return true,
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
                && !crate::transform::is_presentation_neutral_control(&bytes[index..=end])
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
            if end == bytes.len() || !crate::transform::is_safe_osc(&bytes[payload_start..end]) {
                return true;
            }
            index = end + if bytes[end] == 0x07 { 1 } else { 2 };
        } else if bytes
            .get(index..index + 3)
            .is_some_and(crate::transform::is_presentation_neutral_control)
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

fn dcs_marker(token: &str, kind: &str) -> Vec<u8> {
    format!("\x1bP+BIONIC_READER;{token};{kind}\x1b\\").into_bytes()
}
