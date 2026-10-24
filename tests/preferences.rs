//! Profile precedence, length rules, terminal preferences, and deterministic filtering.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use bionic_reader::settings::{LengthRule, Overrides, Profile, Settings};
use portable_pty::{CommandBuilder, PtySize};

#[test]
fn profiles_resolve_before_explicit_values_at_each_configuration_layer() {
    let root = tempfile::tempdir().expect("create settings directory");
    let path = root.path().join("reader.toml");
    std::fs::write(&path, "profile = 'subtle'\nfraction = 0.4\nevery = 3\n")
        .expect("write profile settings");
    let from_file = Settings::load(Some(&path), &Overrides::default()).expect("load file profile");
    assert_eq!(from_file.profile, Some(Profile::Subtle));
    assert_eq!(from_file.fraction.to_bits(), 0.4_f64.to_bits());
    assert_eq!(from_file.every, 3);
    assert_eq!(from_file.min_word_length, 4);
    let settings = Settings::load(
        Some(&path),
        &Overrides {
            profile: Some(Profile::Strong),
            fraction: Some(0.6),
            ..Overrides::default()
        },
    )
    .expect("load profile and explicit overrides");
    assert_eq!(settings.profile, Some(Profile::Strong));
    assert_eq!(settings.fraction.to_bits(), 0.6_f64.to_bits());
    assert_eq!(settings.every, 1);
    assert_eq!(settings.min_word_length, 2);
}

#[test]
fn length_rules_are_inclusive_and_validate_their_order_and_fractions() {
    let settings = Settings {
        fraction: 0.7,
        length_rules: vec![
            LengthRule {
                max_length: 4,
                fraction: 0.25,
            },
            LengthRule {
                max_length: 8,
                fraction: 0.5,
            },
        ],
        ..Settings::default()
    };
    settings.validate().expect("valid increasing length rules");
    for (length, expected) in [(4, 0.25_f64), (5, 0.5), (8, 0.5), (9, 0.7)] {
        assert_eq!(
            settings.fraction_for_length(length).to_bits(),
            expected.to_bits()
        );
    }
    for rules in [
        vec![LengthRule {
            max_length: 0,
            fraction: 0.5,
        }],
        vec![LengthRule {
            max_length: 4,
            fraction: f64::NAN,
        }],
        vec![LengthRule {
            max_length: 4,
            fraction: 1.1,
        }],
        vec![
            LengthRule {
                max_length: 4,
                fraction: 0.5,
            },
            LengthRule {
                max_length: 3,
                fraction: 0.5,
            },
        ],
    ] {
        assert!(
            Settings {
                length_rules: rules,
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
    }
}

fn terminal_output(style: &str, no_color: &str, term: &str) -> Vec<u8> {
    let home = tempfile::tempdir().expect("create isolated terminal preferences home");
    let pair = portable_pty::native_pty_system()
        .openpty(PtySize::default())
        .expect("open preferences pseudo-terminal");
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_bionic-reader"));
    command.args(["--style", style, "-t", "reading"]);
    command.env("HOME", home.path());
    command.env("XDG_CONFIG_HOME", home.path());
    command.env("NO_COLOR", no_color);
    command.env("TERM", term);
    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("start terminal preference probe");
    drop(pair.slave);
    let mut output = Vec::new();
    pair.master
        .try_clone_reader()
        .expect("clone preferences reader")
        .read_to_end(&mut output)
        .expect("read preferences output");
    assert!(
        child
            .wait()
            .expect("wait for preferences process")
            .success()
    );
    output
}

#[test]
fn auto_respects_terminal_preferences_and_always_is_an_explicit_override() {
    assert_eq!(terminal_output("auto", "1", "xterm-256color"), b"reading");
    assert_eq!(terminal_output("auto", "", "dumb"), b"reading");
    assert_eq!(
        terminal_output("auto", "", "xterm-256color"),
        b"\x1b[1mread\x1b[22ming"
    );
    assert_eq!(
        terminal_output("always", "1", "dumb"),
        b"\x1b[1mread\x1b[22ming"
    );
}

#[test]
fn ordinary_filtering_is_independent_of_input_delays() {
    let home = tempfile::tempdir().expect("create deterministic filter home");
    let mut child = Command::new(env!("CARGO_BIN_EXE_bionic-reader"))
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .args(["--style", "always"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start deterministic filter");
    let mut input = child.stdin.take().expect("open filter stdin");
    input.write_all(b"read").expect("write partial word");
    input.flush().expect("flush partial word");
    thread::sleep(Duration::from_millis(150));
    input.write_all(b"ing").expect("complete delayed word");
    drop(input);
    let output = child
        .wait_with_output()
        .expect("collect delayed filter output");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\x1b[1mread\x1b[22ming");
}
