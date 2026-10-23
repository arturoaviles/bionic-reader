use std::process::Command;

#[test]
fn generated_bash_completion_handles_subcommands_and_values() {
    let generated = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .args(["completions", "bash"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let directory = tempfile::tempdir().unwrap();
    let completion = directory.path().join("bionic-reader.bash");
    std::fs::write(&completion, generated.stdout).unwrap();
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
        .unwrap();
    assert!(
        completed.status.success(),
        "{}",
        String::from_utf8_lossy(&completed.stderr)
    );
    assert_eq!(
        String::from_utf8(completed.stdout).unwrap(),
        "--shell\n--style\n---\nauto\nalways\n---\nfish\n"
    );
}

#[test]
fn help_describes_minimum_word_length_and_style() {
    let help = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .arg("--help")
        .env("COLUMNS", "140")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("Minimum number of letters a word must have to receive emphasis"));
    assert!(help.contains("When to apply emphasis: automatically for terminals, always, or never"));
}
