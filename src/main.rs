use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bionic_reader::settings::{Overrides, Settings, Style};
use bionic_reader::{session, transform::Transformer};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    version,
    about = "Bionic reading for text streams and interactive shells"
)]
struct Cli {
    /// Transform this text instead of reading stdin.
    #[arg(short = 't', long)]
    text: Option<String>,
    /// Fraction of each selected word to emphasize (0 through 1).
    #[arg(long, global = true)]
    fraction: Option<f64>,
    /// Emphasize the first eligible word and then every Nth word.
    #[arg(long, global = true)]
    every: Option<usize>,
    /// Minimum number of letters a word must have to receive emphasis.
    #[arg(long, global = true)]
    min_word_length: Option<usize>,
    /// When to apply emphasis: automatically for terminals, always, or never.
    #[arg(long, global = true, value_enum)]
    style: Option<Style>,
    /// Read an explicit TOML configuration file.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Add a command name to the interactive passthrough exclusions.
    #[arg(long, global = true)]
    exclude: Vec<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive reading session.
    Session {
        /// Bash, Zsh, or fish executable (defaults to SHELL).
        #[arg(long)]
        shell: Option<String>,
    },
    /// Print an opt-in initialization snippet for your shell configuration.
    Init {
        #[arg(value_parser = ["bash", "zsh", "fish"])]
        shell: String,
    },
    /// Print completion definitions for your shell.
    Completions {
        #[arg(value_parser = ["bash", "zsh", "fish"])]
        shell: String,
    },
    /// Enable emphasis in the current reading session.
    On,
    /// Disable emphasis in the current reading session.
    Off,
    /// Toggle emphasis in the current reading session.
    Toggle,
    /// Reload settings at the next shell command boundary.
    Reload,
    /// Run a command without emphasis, restoring the prior state afterward.
    Bypass {
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        command: Vec<OsString>,
    },
}

enum Failure {
    Usage(String),
    Runtime(String),
    Io(io::Error),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(Failure::Usage(message)) => {
            eprintln!("bionic-reader: {message}");
            2
        }
        Err(Failure::Runtime(message)) => {
            eprintln!("bionic-reader: {message}");
            1
        }
        Err(Failure::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe => 141,
        Err(Failure::Io(error)) => {
            eprintln!("bionic-reader: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32, Failure> {
    let cli = Cli::parse();
    if cli.text.is_some() && cli.command.is_some() {
        return Err(Failure::Usage(
            "--text cannot be combined with a subcommand".into(),
        ));
    }
    match &cli.command {
        Some(Commands::Completions { shell }) => {
            use clap::CommandFactory;
            let shell = match shell.as_str() {
                "bash" => clap_complete::Shell::Bash,
                "zsh" => clap_complete::Shell::Zsh,
                _ => clap_complete::Shell::Fish,
            };
            // The generator treats a writer error as a panic. Buffer its finite
            // output so a closed downstream pipe follows the normal I/O path.
            let mut generated = Vec::new();
            let binary_name = if shell == clap_complete::Shell::Bash {
                // clap_complete 4.6.11 formats Bash subcommand cases from the
                // bin name differently from its command-state assignments.
                // A normalized root name keeps those generated identifiers in
                // sync; restore the executable name in Bash's registration.
                "bionic__reader"
            } else {
                "bionic-reader"
            };
            clap_complete::generate(shell, &mut Cli::command(), binary_name, &mut generated);
            if shell == clap_complete::Shell::Bash {
                let generated = String::from_utf8(generated)
                    .expect("clap_complete generates UTF-8 shell completions")
                    .replace("default bionic__reader", "default bionic-reader")
                    .into_bytes();
                io::stdout().lock().write_all(&generated)?;
                return Ok(0);
            }
            io::stdout().lock().write_all(&generated)?;
            return Ok(0);
        }
        Some(Commands::Init { shell }) => {
            let code = bionic_reader::shell::init(shell).map_err(Failure::Usage)?;
            io::stdout().lock().write_all(code.as_bytes())?;
            return Ok(0);
        }
        Some(Commands::On | Commands::Off | Commands::Toggle | Commands::Reload) => {
            let command = match cli.command {
                Some(Commands::On) => "on",
                Some(Commands::Off) => "off",
                Some(Commands::Toggle) => "toggle",
                _ => "reload",
            };
            session::control(command).map_err(Failure::Runtime)?;
            return Ok(0);
        }
        Some(Commands::Bypass { command }) => {
            return session::bypass(command).map_err(Failure::Runtime);
        }
        _ => {}
    }

    let overrides = Overrides {
        fraction: cli.fraction,
        every: cli.every,
        min_word_length: cli.min_word_length,
        style: cli.style,
        exclude_commands: (!cli.exclude.is_empty()).then_some(cli.exclude),
    };
    let settings = Settings::load(cli.config.as_deref(), &overrides).map_err(Failure::Usage)?;
    if let Some(Commands::Session { shell }) = cli.command {
        return session::run(shell.as_deref(), settings, cli.config, overrides)
            .map_err(Failure::Runtime);
    }
    if cli.text.is_none() && io::stdin().is_terminal() {
        use clap::CommandFactory;
        Cli::command().print_help()?;
        println!();
        return Ok(0);
    }
    let enabled = match settings.style {
        Style::Auto => io::stdout().is_terminal(),
        Style::Always => true,
        Style::Never => false,
    };
    let mut renderer = Transformer::new(settings, enabled);
    let mut output = io::stdout().lock();
    if let Some(text) = cli.text {
        output.write_all(&renderer.push(text.as_bytes()))?;
        output.write_all(&renderer.finish())?;
    } else {
        stream_filter(&mut renderer, &mut output)?;
    }
    output.flush()?;
    Ok(0)
}

fn stream_filter(renderer: &mut Transformer, output: &mut impl Write) -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut buffer = [0_u8; 8192];
    let deadline = Duration::from_millis(50);
    let mut pending_since: Option<Instant> = None;
    loop {
        let wait = pending_since.map_or(-1, |started| {
            deadline
                .saturating_sub(started.elapsed())
                .as_millis()
                .max(1) as i32
        });
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: descriptor is a valid, initialized pollfd for the duration of this call.
        let result = unsafe { libc::poll(&mut descriptor, 1, wait) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if result > 0 {
            match input.read(&mut buffer) {
                Ok(0) => {
                    output.write_all(&renderer.finish())?;
                    return Ok(());
                }
                Ok(count) => {
                    let bytes = renderer.push(&buffer[..count]);
                    if !renderer.has_pending() {
                        pending_since = None;
                    } else if pending_since.is_none() || !bytes.is_empty() {
                        pending_since = Some(Instant::now());
                    }
                    if !bytes.is_empty() {
                        output.write_all(&bytes)?;
                        output.flush()?;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        if pending_since.is_some_and(|started| started.elapsed() >= deadline) {
            output.write_all(&renderer.flush_pending())?;
            output.flush()?;
            pending_since = renderer.has_pending().then(Instant::now);
        }
    }
}
