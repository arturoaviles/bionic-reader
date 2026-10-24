//! Integration tests for generated shell completions and CLI help text.

use std::fs;
#[cfg(unix)]
use std::io::{Read, Write};
use std::process::Command;
#[cfg(unix)]
use std::sync::mpsc;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
#[cfg(unix)]
use tempfile::TempDir;

#[test]
fn generated_bash_completion_handles_subcommands_and_values() {
    let generated = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .args(["completions", "bash"])
        .output()
        .expect("run Bash completion generation");
    assert!(
        generated.status.success(),
        "Bash completion generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let directory = tempfile::tempdir().expect("create temporary completion directory");
    let completion = directory.path().join("bionic-reader.bash");
    fs::write(&completion, generated.stdout).expect("write generated Bash completion");
    let script = r#"
source "$1"
registration=$(complete -p bionic-reader) || exit 42
callback=${registration#*-F }
callback=${callback%% *}
[[ -n $callback ]] || exit 43
complete_for() {
    COMP_WORDS=("$@")
    COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
    "$callback" bionic-reader "${COMP_WORDS[COMP_CWORD]}" "${COMP_WORDS[COMP_CWORD-1]}"
    printf '%s\n' "${COMPREPLY[@]}"
}

complete_for bionic-reader session --s
printf '%s\n' ---
complete_for bionic-reader --style a
printf '%s\n' ---
complete_for bionic-reader completions f
"#;
    let completed = Command::new("bash")
        .arg("-c")
        .arg(script)
        .arg("bionic-reader-completion-test")
        .arg(completion)
        .output()
        .expect("run Bash completion script");
    assert!(
        completed.status.success(),
        "Bash completion script failed: {}",
        String::from_utf8_lossy(&completed.stderr)
    );
    assert_eq!(
        String::from_utf8(completed.stdout).expect("Bash completion output should be valid UTF-8"),
        "--shell\n--style\n--skip-code\n---\nauto\nalways\n---\nfish\n"
    );
}

#[cfg(unix)]
#[test]
fn generated_zsh_completion_executes_commands_options_and_values() {
    let generated = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .args(["completions", "zsh"])
        .output()
        .expect("run Zsh completion generation");
    assert!(
        generated.status.success(),
        "Zsh completion generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let home = tempfile::tempdir().expect("create isolated Zsh completion home");
    fs::write(home.path().join("_bionic-reader"), generated.stdout)
        .expect("install generated Zsh completion in fpath");
    fs::write(
        home.path().join(".zshrc"),
        concat!(
            "fpath=(\"$ZDOTDIR\" $fpath)\n",
            "autoload -Uz compinit\n",
            "compinit -i\n",
            "PROMPT='ZSH_COMPLETION_READY> '\n",
            "function br_completion_test {\n",
            "  local candidate\n",
            "  for candidate in 'bionic-reader session --sh' 'bionic-reader --style=au' 'bionic-reader pre'; do\n",
            "    BUFFER=\"$candidate\"\n",
            "    CURSOR=${#BUFFER}\n",
            "    zle complete-word\n",
            "    print -r -- \"$BUFFER\" >> \"$BR_COMPLETION_RESULT\"\n",
            "  done\n",
            "  BUFFER=''\n",
            "  zle -M 'ZSH_COMPLETION_DONE'\n",
            "  zle reset-prompt\n",
            "}\n",
            "zle -N br_completion_test\n",
            "bindkey '^X^X' br_completion_test\n",
        ),
    )
    .expect("write isolated Zsh completion startup file");

    let report = home.path().join("results");
    let mut terminal = ZshCompletionTerminal::start(home, &report);
    terminal.expect(b"ZSH_COMPLETION_READY> ");
    terminal.send(b"\x18\x18");
    terminal.expect(b"ZSH_COMPLETION_DONE");
    assert_eq!(
        fs::read_to_string(&report).expect("read Zsh completion results"),
        "bionic-reader session --shell=\nbionic-reader --style=auto \nbionic-reader preview \n"
    );

    terminal.send(b"exit\n");
    assert_eq!(
        terminal.wait(),
        0,
        "the isolated Zsh process should exit cleanly"
    );
}

#[cfg(unix)]
struct ZshCompletionTerminal {
    child: Box<dyn Child + Send + Sync>,
    _master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<Vec<u8>>,
    output: Vec<u8>,
    _home: TempDir,
}

#[cfg(unix)]
impl ZshCompletionTerminal {
    fn start(owner: TempDir, report: &std::path::Path) -> Self {
        let home = owner.path();
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pseudo-terminal for Zsh completion");
        let mut command = CommandBuilder::new("zsh");
        // Exercise our completion in the isolated startup file. Ubuntu's global
        // zshrc can run compinit (and prompt) before that file is read.
        command.args(["-d", "-i"]);
        command.env("HOME", home);
        command.env("ZDOTDIR", home);
        command.env("BR_COMPLETION_RESULT", report);
        command.env("TERM", "xterm-256color");
        let child = pair
            .slave
            .spawn_command(command)
            .expect("start interactive Zsh for completion");
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .expect("clone the Zsh completion PTY reader");
        let writer = pair
            .master
            .take_writer()
            .expect("take the Zsh completion PTY writer");
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
            _master: pair.master,
            writer,
            receiver,
            output: Vec::new(),
            _home: owner,
        }
    }

    fn send(&mut self, input: &[u8]) {
        self.writer
            .write_all(input)
            .expect("write to the Zsh completion PTY");
        self.writer.flush().expect("flush the Zsh completion PTY");
    }

    fn expect(&mut self, expected: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self
            .output
            .windows(expected.len())
            .any(|window| window == expected)
        {
            assert!(
                Instant::now() < deadline,
                "Zsh did not emit {:?}: {:?}",
                String::from_utf8_lossy(expected),
                String::from_utf8_lossy(&self.output)
            );
            match self.receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(chunk) if chunk.is_empty() => break,
                Ok(chunk) => self.output.extend(chunk),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        assert!(
            self.output
                .windows(expected.len())
                .any(|window| window == expected),
            "Zsh did not emit {:?}: {:?}",
            String::from_utf8_lossy(expected),
            String::from_utf8_lossy(&self.output)
        );
    }

    fn wait(&mut self) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .expect("poll the isolated Zsh process")
            {
                return status.exit_code();
            }
            assert!(Instant::now() < deadline, "isolated Zsh did not exit");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(unix)]
impl Drop for ZshCompletionTerminal {
    fn drop(&mut self) {
        let _ = self.writer.write_all(b"exit\n");
        let _ = self.writer.flush();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn help_describes_minimum_word_length_and_style() {
    let help = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .arg("--help")
        .env("COLUMNS", "140")
        .output()
        .expect("run CLI help");
    assert!(
        help.status.success(),
        "CLI help failed with status {:?}: {}",
        help.status,
        String::from_utf8_lossy(&help.stderr)
    );
    let help = String::from_utf8(help.stdout).expect("CLI help output should be valid UTF-8");
    assert!(help.contains("Minimum number of letters a word must have to receive emphasis"));
    assert!(help.contains("When to apply emphasis: automatically for terminals, always, or never"));
}
