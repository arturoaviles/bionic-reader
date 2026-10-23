//! Integration tests for generated shell completions and CLI help text.

use std::fs;
use std::process::Command;

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
        "--shell\n--style\n---\nauto\nalways\n---\nfish\n"
    );
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
