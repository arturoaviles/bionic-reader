//! PTY integration tests for shell hooks, session controls, and transformed command output.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize};

// SAFETY JUSTIFICATION: These tests send POSIX signals to their live session process and inspect
// the PTY's termios state. The standard library has no safe APIs for those checks; the calls use
// process IDs from the test child and initialized termios buffers owned by the test.

#[cfg(unix)]
#[test]
fn bash_prompt_sees_the_failed_commands_exit_status() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".bashrc"), "PS1='STATUS:$? READY> '\n")
        .expect("write the session test fixture");
    let (output, status) = run_session("/bin/bash", home.path(), b"false\nexit 0\n", None);
    assert_eq!(
        status, 0,
        "Bash session should exit after the test commands"
    );
    assert_contains(&output, b"STATUS:1 READY> ");
}

#[cfg(unix)]
#[test]
fn latin_utf8_continuation_bytes_are_not_terminal_controls() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write a session shell startup fixture");
    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        "printf 'Ûnicode reading\\n'\nexit 0\n".as_bytes(),
        None,
    );
    assert_eq!(
        status, 0,
        "Bash session should exit after the test commands"
    );
    assert_contains(&output, "\x1b[1mÛnic\x1b[22mode".as_bytes());
    assert_contains(&output, b"\x1b[1mread\x1b[22ming");
}

#[cfg(unix)]
#[test]
fn split_utf8_and_malformed_bytes_do_not_hang_or_disable_session_output() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write a session shell startup fixture");
    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        b"printf '\\303'; sleep 0.1; printf '\\233nicode reading\\n'\nprintf '\\377reading\\n'\nexit 0\n",
        None,
    );
    assert_eq!(
        status, 0,
        "Bash session should exit after the test commands"
    );
    let plain = strip_sgr(&output);
    assert_contains(&plain, "Ûnicode reading\r\n".as_bytes());
    assert_contains(&plain, b"\xffreading\r\n");
    assert!(
        count(&output, b"\x1b[1mread\x1b[22ming") >= 2,
        "output after the split UTF-8 sequence stopped being transformed: {}",
        display(&output)
    );
}

#[cfg(unix)]
#[test]
fn bash_command_output_is_emphasized_and_existing_hooks_are_preserved() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(
        home.path().join(".bashrc"),
        "PS1='READY> '\nPROMPT_COMMAND='printf \"<PROMPT:%s>\" \"$?\"'\ntrap 'printf \"<DEBUG>\" >&2' DEBUG\n",
    )
    .expect("write the session test fixture");

    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        b"printf 'reading\\n'\nfalse\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "Bash session failed: {}", display(&output));
    assert_contains(&output, b"\x1b[1mread\x1b[22ming");
    assert_contains(&output, b"<PROMPT:1>");
    let prompt = output
        .windows(b"READY> ".len())
        .position(|window| window == b"READY> ")
        .expect("the user prompt was not preserved");
    assert!(
        output[prompt..]
            .windows(b"<DEBUG>".len())
            .any(|window| window == b"<DEBUG>"),
        "the user DEBUG trap was not chained after startup: {}",
        display(&output)
    );
    assert_no_private_markers(&output);
}

#[cfg(unix)]
#[test]
fn bash_split_unsafe_terminal_control_disables_transforming_until_prompt() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write a session shell startup fixture");

    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        b"perl -e '$|=1; print \"\\033\"; select undef, undef, undef, 0.2; print \"[?1049hreading\\n\"'\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "Bash session failed: {}", display(&output));
    let control = output
        .windows(b"\x1b[?1049h".len())
        .position(|window| window == b"\x1b[?1049h")
        .expect("the split terminal control sequence was not passed through");
    assert!(
        output[control..]
            .windows(b"\x1b[?1049hreading\r\n".len())
            .any(|window| window == b"\x1b[?1049hreading\r\n"),
        "output after the split control was not passed through intact: {}",
        display(&output)
    );
    assert!(
        !output[control..]
            .windows(b"\x1b[1mread".len())
            .any(|window| window == b"\x1b[1mread"),
        "output was transformed after an unsafe terminal control: {}",
        display(&output)
    );
}

#[cfg(unix)]
#[test]
fn zsh_command_output_is_emphasized_and_prompt_hooks_stay_raw() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(
        home.path().join(".zshrc"),
        concat!(
            "PROMPT='READY> '\n",
            "user_precmd() { local br_user_status=$?; print -n \"<ZPROMPT:$br_user_status>\"; return $br_user_status; }\n",
            "user_preexec() { print -n '<ZPREEXEC>'; }\n",
            "precmd_functions+=(user_precmd)\n",
            "preexec_functions+=(user_preexec)\n",
        ),
    )
    .expect("write the session shell startup configuration");

    let (output, status) = run_session(
        "/bin/zsh",
        home.path(),
        b"printf 'reading\\n'\nfalse\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "Zsh session failed: {}", display(&output));
    assert_contains(&output, b"\x1b[1mread\x1b[22ming");
    assert_contains(&output, b"<ZPROMPT:1>");
    assert_contains(&output, b"READY> ");
    assert_no_private_markers(&output);
}

#[cfg(unix)]
#[test]
fn fish_command_output_is_emphasized_and_prompt_hooks_stay_raw() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    let fish_config = home.path().join(".config/fish");
    fs::create_dir_all(&fish_config).expect("create the session test fixture directory");
    fs::write(
        fish_config.join("config.fish"),
        concat!(
            "function fish_prompt; printf '<FPROMPT>READY> '; end\n",
            "function user_fish_preexec --on-event fish_preexec; printf '<FPREEXEC>'; end\n",
            "function user_fish_postexec --on-event fish_postexec; set -l br_status $status; printf '<FPOST:%s>' $br_status; end\n",
        ),
    )
    .expect("write the session shell startup configuration");
    let fish = env::var("FISH_BIN").unwrap_or_else(|_| "fish".to_owned());
    let fish_dir = Path::new(&fish).parent().unwrap_or_else(|| Path::new("."));

    let (output, status) = run_session(
        &fish,
        home.path(),
        b"printf 'reading\\n'\nfalse\nexit 0\n",
        Some(
            fish_dir
                .to_str()
                .expect("the test shell path should be valid UTF-8"),
        ),
    );
    assert_eq!(status, 0, "fish session failed: {}", display(&output));
    assert_contains(&output, b"\x1b[1mread\x1b[22ming");
    assert_contains(&strip_sgr(&output), b"<FPOST:1>");
    assert_contains(&output, b"<FPROMPT>READY> ");
    assert_no_private_markers(&output);
}

#[cfg(unix)]
#[test]
fn explicit_on_overrides_a_never_style_for_later_session_commands() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    let config = home.path().join(".config/bionic-reader");
    fs::create_dir_all(&config).expect("create the session test fixture directory");
    fs::write(config.join("config.toml"), "style = 'never'\n")
        .expect("write the session test fixture");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write the session test fixture");

    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        b"printf '<1>'; printf 'reading\\n'\nbionic-reader on\nprintf '<2>'; printf 'reading\\n'\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "Bash session failed: {}", display(&output));
    assert_contains(&output, b"<1>reading\r\n");
    assert_contains(&output, b"<2>\x1b[1mread\x1b[22ming\r\n");
}

#[cfg(unix)]
#[test]
fn invalid_reload_keeps_the_last_valid_settings_and_valid_reload_applies_next_command() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    let config = home.path().join(".config/bionic-reader");
    fs::create_dir_all(&config).expect("create the session test fixture directory");
    fs::write(config.join("config.toml"), "style = 'always'\n")
        .expect("write the session test fixture");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write the session test fixture");

    let commands = b"printf '<1>'; printf 'reading\\n'\nprintf \"fraction = 2.0\\n\" > \"$HOME/.config/bionic-reader/config.toml\"\nbionic-reader reload\nprintf '<2>'; printf 'reading\\n'\nprintf \"style = 'never'\\n\" > \"$HOME/.config/bionic-reader/config.toml\"\nbionic-reader reload\nprintf '<3>'; printf 'reading\\n'\nexit 0\n";
    let (output, status) = run_session("/bin/bash", home.path(), commands, None);
    assert_eq!(status, 0, "Bash session failed: {}", display(&output));
    assert_contains(&output, b"<1>\x1b[1mread\x1b[22ming\r\n");
    assert_contains(&output, b"<2>\x1b[1mread\x1b[22ming\r\n");
    assert_contains(&output, b"<3>reading\r\n");
    assert_no_private_markers(&output);
}

#[cfg(unix)]
#[test]
fn failed_bypass_commands_restore_transformation_for_the_next_command() {
    let home = tempfile::tempdir().expect("create a temporary session home");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n").expect("write the session fixture");
    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        b"bionic-reader bypass -- \"$HOME/missing-command\"\nprintf '<STATUS:%s>\\n' \"$?\"\nbionic-reader bypass -- /bin/sh -c 'exit 7'\nprintf '<STATUS:%s>\\n' \"$?\"\nprintf 'reading\\n'\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "session failed: {}", display(&output));
    let plain = strip_sgr(&output);
    assert_contains(&plain, b"could not run bypass command");
    assert_contains(&plain, b"<STATUS:1>");
    assert_contains(&plain, b"<STATUS:7>");
    assert_contains(&output, b"\x1b[1mread\x1b[22ming");
    assert_no_private_markers(&output);
}

#[cfg(unix)]
#[test]
fn redirected_bypass_keeps_file_bytes_and_restores_the_next_command() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write a session shell startup fixture");

    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        b"bionic-reader bypass -- printf 'raw\\n' > \"$HOME/bypass.out\"\nprintf 'reading\\n'\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "Bash session failed: {}", display(&output));
    assert_eq!(
        fs::read(home.path().join("bypass.out"))
            .expect("read the output file created by the bypass test"),
        b"raw\n"
    );
    assert_contains(&output, b"\x1b[1mread\x1b[22ming\r\n");
    assert_no_private_markers(&output);
}

#[cfg(unix)]
#[test]
fn bash_framework_adapters_share_one_idempotent_initializer() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    let startup = format!(
        "PS1='READY> '\n{}\n{}\n",
        include_str!("../integrations/adapters/bash-it/bionic-reader/bionic-reader.plugin.bash"),
        include_str!("../integrations/adapters/oh-my-bash/bionic-reader/bionic-reader.plugin.sh"),
    );
    fs::write(home.path().join(".bashrc"), startup).expect("write the session test fixture");

    let (output, status) = run_session(
        "/bin/bash",
        home.path(),
        b"printf 'reading\\n'\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "Bash session failed: {}", display(&output));
    assert_eq!(
        count(&output, b"\x1b[1mread"),
        1,
        "adapter hooks ran more than once: {}",
        display(&output)
    );
}

#[cfg(unix)]
#[test]
fn zsh_framework_adapters_share_one_idempotent_initializer() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    let startup = format!(
        "PROMPT='READY> '\n{}\n{}\n{}\n",
        include_str!("../integrations/adapters/oh-my-zsh/bionic-reader/bionic-reader.plugin.zsh"),
        include_str!("../integrations/adapters/prezto/bionic-reader/init.zsh"),
        include_str!("../integrations/adapters/zinit/bionic-reader/init.zsh"),
    );
    fs::write(home.path().join(".zshrc"), startup).expect("write the session test fixture");

    let (output, status) = run_session(
        "/bin/zsh",
        home.path(),
        b"printf 'reading\\n'\nexit 0\n",
        None,
    );
    assert_eq!(status, 0, "Zsh session failed: {}", display(&output));
    assert_eq!(
        count(&output, b"\x1b[1mread"),
        1,
        "adapter hooks ran more than once: {}",
        display(&output)
    );
}

#[cfg(unix)]
#[test]
fn fish_framework_adapters_share_one_idempotent_initializer() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    let fish_config = home.path().join(".config/fish");
    fs::create_dir_all(fish_config.join("conf.d"))
        .expect("create the session test fixture directory");
    let adapter_config = format!(
        "function fish_prompt; printf 'READY> '; end\n{}\n{}\n",
        include_str!("../integrations/adapters/fisher/bionic-reader/conf.d/bionic-reader.fish"),
        include_str!("../integrations/adapters/oh-my-fish/bionic-reader/init.fish"),
    );
    fs::write(fish_config.join("config.fish"), adapter_config)
        .expect("write the session test fixture");
    let fish = env::var("FISH_BIN").unwrap_or_else(|_| "fish".to_owned());
    let fish_dir = Path::new(&fish).parent().unwrap_or_else(|| Path::new("."));

    let (output, status) = run_session(
        &fish,
        home.path(),
        b"printf 'reading\\n'\nexit 0\n",
        Some(
            fish_dir
                .to_str()
                .expect("the test shell path should be valid UTF-8"),
        ),
    );
    assert_eq!(status, 0, "fish session failed: {}", display(&output));
    assert_eq!(
        count(&output, b"\x1b[1mread"),
        1,
        "adapter hooks ran more than once: {}",
        display(&output)
    );
}

#[cfg(unix)]
#[test]
fn external_term_exits_the_session_and_restores_terminal_mode() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write a session shell startup fixture");

    let result = run_session_with_term_signal("/bin/bash", home.path());
    assert_eq!(
        result.status,
        143,
        "session did not report SIGTERM: {}",
        display(&result.output)
    );
    assert!(result.raw_during, "session did not enter raw terminal mode");
    assert!(
        result.restored,
        "session did not restore the caller terminal mode"
    );
}

#[cfg(unix)]
#[test]
fn ctrl_c_key_interrupts_foreground_job_and_returns_to_the_shell() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".zshrc"), "PROMPT='READY> '\n")
        .expect("write a session shell startup fixture");

    let (output, status) = run_session_with_ctrl_c("/bin/zsh", home.path());
    assert_eq!(
        status,
        0,
        "Zsh session failed after Ctrl-C: {}",
        display(&output)
    );
    assert!(
        count(&output, b"READY> ") >= 2,
        "the shell did not return to its prompt after Ctrl-C: {}",
        display(&output)
    );
}

#[cfg(unix)]
#[test]
fn interactive_shell_keeps_isig_enabled_on_the_session_pty() {
    let home =
        tempfile::tempdir().expect("create a temporary home directory for this session test");
    fs::write(home.path().join(".bashrc"), "PS1='READY> '\n")
        .expect("write a session shell startup fixture");
    let (output, status) = run_session("/bin/bash", home.path(), b"stty -a\nexit 0\n", None);
    assert_eq!(status, 0, "Bash session failed: {}", display(&output));
    let plain = strip_sgr(&output);
    assert_contains(&plain, b"isig");
    assert!(
        !plain
            .windows(b"-isig".len())
            .any(|window| window == b"-isig"),
        "session PTY disabled signal-generating control keys: {}",
        display(&plain)
    );
}

#[cfg(unix)]
struct SessionResult {
    output: Vec<u8>,
    status: i32,
    raw_during: bool,
    restored: bool,
}

#[cfg(unix)]
fn run_session(
    shell: &str,
    home: &Path,
    commands: &[u8],
    path_prefix: Option<&str>,
) -> (Vec<u8>, i32) {
    let result = run_session_inner(shell, home, commands, path_prefix, None, None);
    (result.output, result.status)
}

#[cfg(unix)]
fn run_session_with_term_signal(shell: &str, home: &Path) -> SessionResult {
    run_session_inner(shell, home, b"", None, Some(libc::SIGTERM), None)
}

#[cfg(unix)]
fn run_session_with_ctrl_c(shell: &str, home: &Path) -> (Vec<u8>, i32) {
    let started = Instant::now();
    let result = run_session_inner(shell, home, b"sleep 2\n", None, None, Some(b"<1>\r\n"));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "Ctrl-C did not interrupt the foreground job promptly: {}",
        display(&result.output)
    );
    (result.output, result.status)
}

#[cfg(unix)]
#[expect(
    clippy::too_many_lines,
    reason = "Keep prompt readiness, signal handling, output closure, and child cleanup in one PTY state machine so its deadlines remain auditable."
)]
fn run_session_inner(
    shell: &str,
    home: &Path,
    commands: &[u8],
    path_prefix: Option<&str>,
    signal_after_prompt: Option<i32>,
    interrupt_after_echo: Option<&[u8]>,
) -> SessionResult {
    fs::create_dir_all(home.join(".config")).expect("create the session test fixture directory");
    fs::create_dir_all(home.join(".local/share"))
        .expect("create the session test fixture directory");
    fs::create_dir_all(home.join(".cache")).expect("create the session test fixture directory");
    let pair = portable_pty::native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("open a pseudo-terminal for the session test");
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_bionic-reader"));
    command.args(["session", "--shell", shell]);
    command.env("HOME", home);
    command.env("ZDOTDIR", home);
    command.env("XDG_CONFIG_HOME", home.join(".config"));
    command.env("XDG_DATA_HOME", home.join(".local/share"));
    command.env("XDG_CACHE_HOME", home.join(".cache"));
    command.env("TERM", "xterm-256color");
    command.env_remove("NO_COLOR");
    // This PTY harness has no terminal emulator to answer fish capability probes.
    command.env("fish_features", "no-query-term");
    command.env_remove("BIONIC_READER_DISABLE");
    command.env_remove("BIONIC_READER_SESSION");
    command.env_remove("BIONIC_READER_CONTROL_SOCKET");
    command.env_remove("BIONIC_READER_MARKER_TOKEN");
    let mut paths = vec![
        Path::new(env!("CARGO_BIN_EXE_bionic-reader"))
            .parent()
            .expect("Cargo binary path has a parent")
            .to_path_buf(),
    ];
    if let Some(prefix) = path_prefix {
        paths.insert(0, PathBuf::from(prefix));
    }
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    command.env(
        "PATH",
        env::join_paths(paths).expect("assemble the test command search path"),
    );
    // Capture the caller's mode before the child can switch the PTY to raw mode.
    let terminal_fd = pair
        .master
        .as_raw_fd()
        .expect("PTY master has a file descriptor");
    let original_lflag = terminal_lflag(terminal_fd);
    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("start the reading-session wrapper");
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .expect("clone the session PTY reader");
    let mut writer = pair
        .master
        .take_writer()
        .expect("take the session PTY writer");
    let (sender, receiver) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
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

    let mut output = Vec::new();
    let deadline = Instant::now()
        + if interrupt_after_echo.is_some() {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(8)
        };
    let prompt_started = Instant::now();
    let mut sent_prompt_probe = false;
    while !output
        .windows(b"READY> ".len())
        .any(|window| window == b"READY> ")
    {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!(
                "{shell} did not display its user prompt: {}",
                display(&output)
            );
        }
        if !sent_prompt_probe && prompt_started.elapsed() >= Duration::from_secs(1) {
            writer
                .write_all(b"\n")
                .expect("write input to the session PTY");
            sent_prompt_probe = true;
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) if !chunk.is_empty() => output.extend_from_slice(&chunk),
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!(
                    "{shell} closed its PTY before its prompt: {}",
                    display(&output)
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    let during_lflag = terminal_lflag(terminal_fd);
    let mut writer = Some(writer);
    if let Some(signal) = signal_after_prompt {
        let process_id = child
            .process_id()
            .expect("the running session process should have a process ID");
        let result = {
            // SAFETY: `process_id` belongs to the live session wrapper under test.
            unsafe { libc::kill(process_id as libc::pid_t, signal) }
        };
        assert_eq!(
            result, 0,
            "deliver the requested signal to the test session"
        );
    } else {
        writer
            .as_mut()
            .expect("the session input writer should still be available")
            .write_all(commands)
            .expect("write commands to the session PTY");
        if interrupt_after_echo.is_none() {
            drop(writer.take());
        }
    }

    let mut child_status = None;
    let mut output_closed = false;
    let mut ctrl_c_sent = false;
    let mut exit_after_interrupt_sent = false;
    let mut prompt_count_at_interrupt = 0;
    let mut interrupt_at =
        interrupt_after_echo.map(|_| Instant::now() + Duration::from_millis(200));
    let mut exit_deadline = if signal_after_prompt.is_some() {
        Instant::now() + Duration::from_secs(2)
    } else {
        deadline
    };
    let mut forced_kill = false;
    while !(child_status.is_some() && output_closed) {
        if !ctrl_c_sent && interrupt_at.is_some_and(|deadline| Instant::now() >= deadline) {
            prompt_count_at_interrupt = count(&output, b"READY> ");
            writer
                .as_mut()
                .expect("the session input writer should still be available")
                .write_all(b"\x03")
                .expect("send Ctrl-C to the session PTY");
            writer
                .as_mut()
                .expect("the session input writer should still be available")
                .flush()
                .expect("flush input to the session PTY");
            ctrl_c_sent = true;
            interrupt_at = None;
        }
        if child_status.is_none() {
            child_status = child
                .try_wait()
                .expect("polling the session process should succeed");
        }
        if child_status.is_none() && Instant::now() >= exit_deadline {
            if signal_after_prompt.is_some() && !forced_kill {
                // Keep the behavioral test bounded if the wrapper fails to handle TERM.
                let process_id = child
                    .process_id()
                    .expect("the test session process should have a process ID");
                // SAFETY: `process_id` is the wrapper process and remains live until it is reaped.
                let _ = unsafe { libc::kill(process_id as libc::pid_t, libc::SIGKILL) };
                forced_kill = true;
                exit_deadline = Instant::now() + Duration::from_secs(2);
            } else {
                let _ = child.kill();
                panic!(
                    "{shell} wrapper did not exit after the requested signal: {}",
                    display(&output)
                );
            }
        }
        if output_closed {
            if Instant::now() >= exit_deadline + Duration::from_secs(2) {
                let _ = child.kill();
                panic!("{shell} closed its PTY but did not exit");
            }
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) if chunk.is_empty() => output_closed = true,
            Ok(chunk) => {
                output.extend_from_slice(&chunk);
                if ctrl_c_sent
                    && !exit_after_interrupt_sent
                    && count(&output, b"READY> ") > prompt_count_at_interrupt
                {
                    writer
                        .as_mut()
                        .expect("the test pseudo-terminal writer should still be available")
                        .write_all(b"exit 0\n")
                        .expect("write input to the session PTY");
                    drop(writer.take());
                    exit_after_interrupt_sent = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout)
                if Instant::now() < exit_deadline + Duration::from_secs(2) => {}
            Err(_) => {
                let _ = child.kill();
                panic!(
                    "{shell} did not exit after the shell commands: {}",
                    display(&output)
                );
            }
        }
    }
    let _ = reader_thread.join();
    let status = child_status
        .expect("the session child should exit before its PTY output closes")
        .exit_code();
    let status = i32::try_from(status).expect("the PTY child exit code fits in i32");
    let restored_lflag = terminal_lflag(terminal_fd);
    let raw_during = during_lflag & (libc::ECHO | libc::ICANON | libc::ISIG) == 0;
    let restored = restored_lflag & (libc::ECHO | libc::ICANON | libc::ISIG)
        == original_lflag & (libc::ECHO | libc::ICANON | libc::ISIG);
    drop(writer);
    SessionResult {
        output,
        status,
        raw_during,
        restored,
    }
}

#[cfg(unix)]
fn terminal_lflag(fd: i32) -> libc::tcflag_t {
    let mut settings: MaybeUninit<libc::termios> = MaybeUninit::uninit();
    let result = {
        // SAFETY: tcgetattr initializes termios on success; fd remains open for this helper.
        unsafe { libc::tcgetattr(fd, settings.as_mut_ptr()) }
    };
    assert_eq!(result, 0, "tcgetattr should inspect the open test PTY");
    // SAFETY: tcgetattr returned success.
    unsafe { settings.assume_init() }.c_lflag
}

#[cfg(unix)]
fn assert_contains(output: &[u8], expected: &[u8]) {
    assert!(
        output
            .windows(expected.len())
            .any(|window| window == expected),
        "expected {:?} in PTY output: {}",
        String::from_utf8_lossy(expected),
        display(output)
    );
}

#[cfg(unix)]
#[must_use]
fn count(output: &[u8], needle: &[u8]) -> usize {
    output
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[cfg(unix)]
fn assert_no_private_markers(output: &[u8]) {
    assert!(
        !output
            .windows(b"\x1bP+BIONIC_READER;".len())
            .any(|window| window == b"\x1bP+BIONIC_READER;"),
        "private session marker leaked to terminal output: {}",
        display(output)
    );
}

#[cfg(unix)]
#[must_use]
fn display(output: &[u8]) -> String {
    String::from_utf8_lossy(output).into_owned()
}

#[cfg(unix)]
#[must_use]
fn strip_sgr(output: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(output.len());
    let mut index = 0;
    while index < output.len() {
        if output[index..].starts_with(b"\x1b[") {
            let start = index + 2;
            if let Some(relative_end) = output[start..]
                .iter()
                .position(|byte| (0x40..=0x7e).contains(byte))
            {
                let end = start + relative_end;
                if output[end] == b'm' {
                    index = end + 1;
                    continue;
                }
            }
        }
        result.push(output[index]);
        index += 1;
    }
    result
}
