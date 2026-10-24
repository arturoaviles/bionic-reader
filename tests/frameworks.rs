//! Opt-in smoke tests against the framework managers' real loaders.
//!
//! Set `BIONIC_READER_FRAMEWORKS_DIR` to a directory containing shallow
//! checkouts named `bash-it`, `oh-my-bash`, `oh-my-zsh`, `prezto`, `zinit`,
//! `fisher`, and `oh-my-fish`, then run this test target with `--ignored`.

#![cfg(unix)]

use std::env;
use std::fs::{self, Permissions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize};

const EMPHASIZED_PROBE: &[u8] = b"\x1b[1madapte\x1b[22mrproof";

#[test]
#[ignore = "requires the pinned upstream Bash-it checkout in BIONIC_READER_FRAMEWORKS_DIR"]
fn bash_it_loads_adapter_through_its_custom_plugin_loader() {
    let frameworks = frameworks_dir();
    let framework = frameworks.join("bash-it");
    assert_framework_checkout(&framework);
    let home = tempfile::tempdir().expect("create temporary home for Bash-it test");
    let custom_plugins = home.path().join(".bash_it_custom/plugins");
    write_adapter(
        "integrations/adapters/bash-it/bionic-reader/bionic-reader.plugin.bash",
        &custom_plugins.join("bionic-reader.plugin.bash"),
    );
    write_startup(
        &home.path().join(".bashrc"),
        &format!(
            "BASH_IT={}; BASH_IT_CUSTOM={}; BASH_IT_THEME=\nsource \"$BASH_IT/bash_it.sh\"\n",
            shell_quote(&framework),
            shell_quote(&home.path().join(".bash_it_custom")),
        ),
    );

    assert_framework_session("bash", home.path(), None);
}

#[test]
#[ignore = "requires the pinned upstream Oh My Bash checkout in BIONIC_READER_FRAMEWORKS_DIR"]
fn oh_my_bash_loads_adapter_from_custom_plugins() {
    let frameworks = frameworks_dir();
    let framework = frameworks.join("oh-my-bash");
    assert_framework_checkout(&framework);
    let home = tempfile::tempdir().expect("create temporary home for Oh My Bash test");
    let custom = home.path().join(".oh-my-bash-custom/plugins/bionic-reader");
    write_adapter(
        "integrations/adapters/oh-my-bash/bionic-reader/bionic-reader.plugin.sh",
        &custom.join("bionic-reader.plugin.sh"),
    );
    write_startup(
        &home.path().join(".bashrc"),
        &format!(
            "OSH={}; OSH_CUSTOM={}; OSH_CACHE_DIR={}; DISABLE_AUTO_UPDATE=true\nplugins=(bionic-reader)\nsource \"$OSH/oh-my-bash.sh\"\n",
            shell_quote(&framework),
            shell_quote(&home.path().join(".oh-my-bash-custom")),
            shell_quote(&home.path().join(".cache/oh-my-bash")),
        ),
    );

    assert_framework_session("bash", home.path(), None);
}

#[test]
#[ignore = "requires the pinned upstream Oh My Zsh checkout in BIONIC_READER_FRAMEWORKS_DIR"]
fn oh_my_zsh_loads_adapter_from_custom_plugins() {
    let frameworks = frameworks_dir();
    let framework = frameworks.join("oh-my-zsh");
    assert_framework_checkout(&framework);
    let home = tempfile::tempdir().expect("create temporary home for Oh My Zsh test");
    let custom = home.path().join(".oh-my-zsh-custom/plugins/bionic-reader");
    write_adapter(
        "integrations/adapters/oh-my-zsh/bionic-reader/bionic-reader.plugin.zsh",
        &custom.join("bionic-reader.plugin.zsh"),
    );
    write_startup(
        &home.path().join(".zshrc"),
        &format!(
            "ZSH={}; ZSH_CUSTOM={}; ZSH_THEME=''\nplugins=(bionic-reader)\nsource \"$ZSH/oh-my-zsh.sh\"\n",
            shell_quote(&framework),
            shell_quote(&home.path().join(".oh-my-zsh-custom")),
        ),
    );

    assert_framework_session("zsh", home.path(), None);
}

#[test]
#[ignore = "requires the pinned upstream Prezto checkout in BIONIC_READER_FRAMEWORKS_DIR"]
fn prezto_loads_adapter_as_a_user_module() {
    let frameworks = frameworks_dir();
    let framework = frameworks.join("prezto");
    assert_framework_checkout(&framework);
    let home = tempfile::tempdir().expect("create temporary home for Prezto test");
    let module = home.path().join(".zprezto-contrib/bionic-reader");
    write_adapter(
        "integrations/adapters/prezto/bionic-reader/init.zsh",
        &module.join("init.zsh"),
    );
    write_startup(
        &home.path().join(".zpreztorc"),
        &format!(
            "zstyle ':prezto:load' pmodule-dirs {}\nzstyle ':prezto:load' pmodule 'bionic-reader'\n",
            shell_quote(&home.path().join(".zprezto-contrib")),
        ),
    );
    write_startup(
        &home.path().join(".zshrc"),
        &format!("source {}/init.zsh\n", shell_quote(&framework),),
    );

    assert_framework_session("zsh", home.path(), None);
}

#[test]
#[ignore = "requires the pinned upstream Zinit checkout in BIONIC_READER_FRAMEWORKS_DIR"]
fn zinit_loads_adapter_from_a_local_plugin_path() {
    let frameworks = frameworks_dir();
    let framework = frameworks.join("zinit");
    assert_framework_checkout(&framework);
    let home = tempfile::tempdir().expect("create temporary home for Zinit test");
    let plugin = home.path().join(".local/share/bionic-reader-zinit");
    write_adapter(
        "integrations/adapters/zinit/bionic-reader/init.zsh",
        &plugin.join("init.zsh"),
    );
    write_startup(
        &home.path().join(".zshrc"),
        &format!(
            "source {}/zinit.zsh\nzinit light {}\n",
            shell_quote(&framework),
            shell_quote(&plugin),
        ),
    );

    assert_framework_session("zsh", home.path(), None);
}

#[test]
#[ignore = "requires the pinned upstream Fisher checkout in BIONIC_READER_FRAMEWORKS_DIR"]
fn fisher_installs_adapter_conf_d_file_into_interactive_startup() {
    let frameworks = frameworks_dir();
    let framework = frameworks.join("fisher");
    assert_framework_checkout(&framework);
    let home = tempfile::tempdir().expect("create temporary home for Fisher test");
    let plugin = adapter_dir("fisher");
    let fish_config = home.path().join(".config/fish");
    fs::create_dir_all(&fish_config).expect("create Fisher Fish config directory");
    let fish = env::var_os("FISH_BIN").unwrap_or_else(|| "fish".into());
    let install = Command::new(fish)
        .arg("--command")
        .arg(format!(
            "source {}/functions/fisher.fish; fisher install {}",
            shell_quote(&framework),
            shell_quote(&plugin),
        ))
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("XDG_DATA_HOME", home.path().join(".local/share"))
        .stdin(Stdio::null())
        .output()
        .expect("Fisher test needs a Fish executable");
    assert!(
        install.status.success(),
        "Fisher could not install the local plugin: {}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(
        fish_config.join("conf.d/bionic-reader.fish").is_file(),
        "Fisher did not install the adapter into conf.d"
    );
    write_startup(&fish_config.join("config.fish"), "");

    assert_framework_session("fish", home.path(), fish_bin_dir());
}

#[test]
#[ignore = "requires the pinned upstream Oh My Fish checkout in BIONIC_READER_FRAMEWORKS_DIR"]
fn oh_my_fish_loads_adapter_as_a_package() {
    let frameworks = frameworks_dir();
    let framework = frameworks.join("oh-my-fish");
    assert_framework_checkout(&framework);
    let home = tempfile::tempdir().expect("create temporary home for Oh My Fish test");
    let package = home.path().join(".config/omf/pkg/bionic-reader");
    write_adapter(
        "integrations/adapters/oh-my-fish/bionic-reader/init.fish",
        &package.join("init.fish"),
    );
    let fish_config = home.path().join(".config/fish");
    fs::create_dir_all(&fish_config).expect("create Oh My Fish config directory");
    write_startup(
        &fish_config.join("config.fish"),
        &format!(
            "set -gx OMF_PATH {}\nset -gx OMF_CONFIG {}\nsource $OMF_PATH/init.fish\n",
            shell_quote(&framework),
            shell_quote(&home.path().join(".config/omf")),
        ),
    );

    assert_framework_session("fish", home.path(), fish_bin_dir());
}

#[must_use]
fn frameworks_dir() -> PathBuf {
    env::var_os("BIONIC_READER_FRAMEWORKS_DIR")
        .map(PathBuf::from)
        .expect("set BIONIC_READER_FRAMEWORKS_DIR to official framework checkouts")
}

fn assert_framework_checkout(path: &Path) {
    assert!(
        path.join(".git").exists(),
        "expected a git checkout at {}; see integrations/adapters/README.md",
        path.display()
    );
}

#[must_use]
fn adapter_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("integrations/adapters")
        .join(name)
        .join("bionic-reader")
}

fn write_adapter(relative_source: &str, destination: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_source);
    let contents = fs::read(source).expect("adapter asset should be readable");
    let contents =
        str::from_utf8(&contents).expect("adapter asset should contain UTF-8 shell code");
    write_startup(destination, contents);
}

fn write_startup(path: &Path, contents: &str) {
    let parent = path
        .parent()
        .expect("startup file path should have a parent directory");
    fs::create_dir_all(parent).expect("create shell startup directory");
    fs::write(path, contents).expect("write shell startup file");
}

#[must_use]
fn shell_quote(path: &Path) -> String {
    let path = path.to_string_lossy();
    format!("'{}'", path.replace('\'', "'\\''"))
}

#[must_use]
fn fish_bin_dir() -> Option<PathBuf> {
    let fish = env::var_os("FISH_BIN").map(PathBuf::from)?;
    fish.parent().map(Path::to_path_buf)
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep the PTY startup, command exchange, and exit checks together so their shared synchronization state stays visible."
)]
fn assert_framework_session(shell: &str, home: &Path, extra_path: Option<PathBuf>) {
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;

    let pair = portable_pty::native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("create a pseudo-terminal for the framework session test");
    // Start the user's shell itself: its framework adapter must activate the reading session.
    let mut command = CommandBuilder::new(shell);
    command.arg("-i");
    command.env("HOME", home);
    command.env("ZDOTDIR", home);
    command.env("XDG_CONFIG_HOME", home.join(".config"));
    command.env("XDG_DATA_HOME", home.join(".local/share"));
    command.env("XDG_CACHE_HOME", home.join(".cache"));
    command.env("TERM", "xterm-256color");
    command.env_remove("NO_COLOR");
    // This PTY harness has no terminal emulator to answer fish capability probes.
    command.env("fish_features", "no-query-term");
    command.env(
        "BIONIC_READER_INIT_LOG",
        home.join("bionic-reader-init-count"),
    );
    command.env_remove("BIONIC_READER_DISABLE");
    command.env_remove("BIONIC_READER_SESSION");
    command.env_remove("BIONIC_READER_CONTROL_SOCKET");
    command.env_remove("BIONIC_READER_MARKER_TOKEN");
    let mut paths = vec![
        home.join("bin"),
        Path::new(env!("CARGO_BIN_EXE_bionic-reader"))
            .parent()
            .expect("test executable should have a parent directory")
            .to_path_buf(),
    ];
    if let Some(path) = extra_path {
        paths.insert(0, path);
    }
    if let Some(path) = env::var_os("PATH") {
        paths.extend(env::split_paths(&path));
    }
    let wrapper_bin = home.join("bin");
    fs::create_dir_all(&wrapper_bin).expect("create framework test wrapper directory");
    let real_cli = shell_quote(Path::new(env!("CARGO_BIN_EXE_bionic-reader")));
    let wrapper = wrapper_bin.join("bionic-reader");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\ncase \"$1\" in init|session) printf '%s\\n' \"$1\" >> \"$BIONIC_READER_INIT_LOG\" ;; esac\nexec {real_cli} \"$@\"\n"
        ),
    )
    .expect("write framework test wrapper");
    fs::set_permissions(&wrapper, Permissions::from_mode(0o755))
        .expect("make framework test wrapper executable");
    command.env(
        "PATH",
        env::join_paths(paths).expect("construct PATH for framework shell"),
    );

    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("spawn shell in framework session test");
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .expect("clone pseudo-terminal reader");
    let mut writer = pair
        .master
        .take_writer()
        .expect("take pseudo-terminal writer");
    let (sender, receiver) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut output = Vec::new();
    let startup_deadline = Instant::now() + Duration::from_secs(20);
    let init_log = home.join("bionic-reader-init-count");
    let mut initialized_at: Option<Instant> = None;
    while initialized_at.is_none_or(|at| at.elapsed() < Duration::from_millis(500)) {
        if Instant::now() >= startup_deadline {
            let _ = child.kill();
            let init_calls = fs::read_to_string(&init_log).unwrap_or_default();
            panic!(
                "{shell} manager did not load the adapter (init calls: {}): {}",
                init_calls.lines().count(),
                display(&output)
            );
        }
        if initialized_at.is_none()
            && fs::read_to_string(&init_log)
                .is_ok_and(|calls| !calls.lines().next().unwrap_or_default().is_empty())
        {
            initialized_at = Some(Instant::now());
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => {
                // A real terminal answers fish's capability probes. This
                // headless PTY emulates the primary device attributes reply.
                if chunk.windows(4).any(|part| part == b"\x1b[0c") {
                    writer
                        .write_all(b"\x1b[?1;2c")
                        .expect("reply to primary device attributes probe");
                }
                if chunk.windows(4).any(|part| part == b"\x1b[6n") {
                    writer
                        .write_all(b"\x1b[1;1R")
                        .expect("reply to cursor position report");
                }
                output.extend_from_slice(&chunk);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!(
                    "{shell} exited during manager startup: {}",
                    display(&output)
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }

    writer
        .write_all(b"printf 'adapterproof\\n'\nprintf 'adapterproof\\n'\n")
        .expect("send framework verification commands");
    let mut exit_sent = false;
    let exit_deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if Instant::now() >= exit_deadline {
            let _ = child.kill();
            panic!("{shell} framework shell did not exit: {}", display(&output));
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => {
                if chunk.windows(4).any(|part| part == b"\x1b[0c") {
                    writer
                        .write_all(b"\x1b[?1;2c")
                        .expect("reply to primary device attributes probe");
                }
                if chunk.windows(4).any(|part| part == b"\x1b[6n") {
                    writer
                        .write_all(b"\x1b[1;1R")
                        .expect("reply to cursor position report");
                }
                output.extend_from_slice(&chunk);
                if !exit_sent && count(&output, EMPHASIZED_PROBE) >= 2 {
                    writer
                        .write_all(b"exit 0\n")
                        .expect("send exit command to framework shell");
                    exit_sent = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break child.wait().expect("wait for framework shell");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(status) = child.try_wait().expect("poll framework shell") {
                    break status;
                }
            }
        }
    };
    reader_thread
        .join()
        .expect("join framework pseudo-terminal reader");
    let init_count = fs::read_to_string(home.join("bionic-reader-init-count")).unwrap_or_default();
    assert_eq!(
        init_count.lines().filter(|line| *line == "init").count(),
        2,
        "{shell} must initialize once before activation and once inside its reading session: {}",
        display(&output)
    );
    assert_eq!(
        init_count.lines().filter(|line| *line == "session").count(),
        1,
        "{shell} must activate exactly one reading session: {}",
        display(&output)
    );
    assert!(
        status.success(),
        "{shell} session failed: {}",
        display(&output)
    );
    assert_eq!(
        count(&output, EMPHASIZED_PROBE),
        2,
        "{shell} manager should transform both adapter probes: {}",
        display(&output)
    );
}

#[must_use]
fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[must_use]
fn display(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace('\x1b', "<ESC>")
}
