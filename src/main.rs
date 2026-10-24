//! Command-line dispatch and deadline-based stdin filtering for bionic reading.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use bionic_reader::Error;
use bionic_reader::settings::{
    ControlPolicy, Emphasis, InputFormat, LengthRule, OutputFormat, Overrides, Profile, Settings,
    Style,
};
use bionic_reader::transformer::Transformer;
use bionic_reader::{document, reader};
use bionic_reader::{session, shell};
use clap::{Parser, Subcommand};
use clap_complete::Shell;

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
    /// Named fixation preference; explicit numeric flags override its values.
    #[arg(long, global = true, value_enum)]
    profile: Option<Profile>,
    /// Per-length fraction in MAX:FRACTION form; repeat in increasing MAX order.
    #[arg(long = "length-rule", global = true, value_parser = parse_length_rule)]
    length_rules: Vec<LengthRule>,
    /// Emphasis attribute: bold, underline, or reverse video.
    #[arg(long, global = true, value_enum)]
    emphasis: Option<Emphasis>,
    /// Interpret ordinary text, Markdown source, or trusted terminal output.
    #[arg(long, global = true, value_enum)]
    input_format: Option<InputFormat>,
    /// Emit ANSI, unstyled plain text, Markdown emphasis, or escaped HTML.
    #[arg(long, global = true, value_enum)]
    format: Option<OutputFormat>,
    /// Remove active document controls, or preserve trusted terminal controls.
    #[arg(long, global = true, value_enum)]
    controls: Option<ControlPolicy>,
    /// Protect Markdown code from emphasis (default true); use --skip-code=false to include it.
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true")]
    skip_code: Option<bool>,
    /// Flush stalled partial words unchanged after 50 ms; disables content-only determinism.
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true")]
    low_latency: Option<bool>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Read a document with scrolling, search, and adjustable fixation settings.
    Read {
        /// File to read; omit it or use - to consume piped stdin.
        file: Option<PathBuf>,
    },
    /// Preview fixation settings on a sample passage or supplied text.
    Preview {
        /// Passage for the interactive preview.
        #[arg(long)]
        text: Option<String>,
        /// Print the preview instead of opening the interactive reader.
        #[arg(long)]
        print: bool,
    },
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
    Runtime(Error),
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
        Err(Failure::Runtime(error)) if error.io_kind() == Some(io::ErrorKind::BrokenPipe) => 141,
        Err(Failure::Runtime(error)) => {
            eprintln!("bionic-reader: {error}");
            1
        }
        Err(Failure::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe => 141,
        Err(Failure::Io(error)) => {
            eprintln!("bionic-reader: {error}");
            1
        }
    };
    process::exit(code);
}

fn run() -> Result<i32, Failure> {
    use std::io::IsTerminal;

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
                "bash" => Shell::Bash,
                "zsh" => Shell::Zsh,
                _ => Shell::Fish,
            };
            // The generator treats a writer error as a panic. Buffer its finite
            // output so a closed downstream pipe follows the normal I/O path.
            let mut generated = Vec::new();
            let binary_name = if shell == Shell::Bash {
                // clap_complete 4.6.11 formats Bash subcommand cases from the
                // bin name differently from its command-state assignments.
                // A normalized root name keeps those generated identifiers in
                // sync; restore the executable name in Bash's registration.
                "bionic__reader"
            } else {
                "bionic-reader"
            };
            clap_complete::generate(shell, &mut Cli::command(), binary_name, &mut generated);
            if shell == Shell::Bash {
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
            let code = shell::init(shell).map_err(|error| Failure::Usage(error.to_string()))?;
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
        profile: cli.profile,
        length_rules: (!cli.length_rules.is_empty()).then_some(cli.length_rules),
        emphasis: cli.emphasis,
        input_format: cli.input_format,
        format: cli.format,
        controls: cli.controls,
        skip_code: cli.skip_code,
        low_latency: cli.low_latency,
    };
    let settings = Settings::load(cli.config.as_deref(), &overrides)
        .map_err(|error| Failure::Usage(error.to_string()))?;
    match cli.command {
        Some(Commands::Session { shell }) => {
            if settings.format != OutputFormat::Ansi {
                return Err(Failure::Usage(
                    "reading sessions require --format ansi".into(),
                ));
            }
            return session::run(shell.as_deref(), settings, cli.config, overrides)
                .map_err(Failure::Runtime);
        }
        Some(Commands::Read { file }) => return read_document(file, settings),
        Some(Commands::Preview { text, print }) => return preview(text, print, settings),
        _ => {}
    }
    if cli.text.is_none() && io::stdin().is_terminal() {
        use clap::CommandFactory;
        Cli::command().print_help()?;
        println!();
        return Ok(0);
    }
    filter(cli.text, &settings)
}

fn read_document(file: Option<PathBuf>, settings: Settings) -> Result<i32, Failure> {
    use std::io::IsTerminal;
    let mut input: Box<dyn Read> = match file {
        Some(path) if path.as_os_str() != "-" => Box::new(std::fs::File::open(path)?),
        _ => {
            if io::stdin().is_terminal() {
                return Err(Failure::Usage(
                    "supply a file or pipe document text into read".into(),
                ));
            }
            Box::new(io::stdin())
        }
    };
    reader::run(&mut input, settings).map_err(Failure::Runtime)?;
    Ok(0)
}

fn preview(text: Option<String>, print: bool, settings: Settings) -> Result<i32, Failure> {
    use std::io::IsTerminal;
    let passage = text.unwrap_or_else(||
        "Reading complex documentation becomes easier with customizable visual emphasis.\nCafé, niño, 日本語, 你好世界, مرحبا, Привет.\n".into());
    if print {
        let enabled = if settings.format == OutputFormat::Ansi {
            settings.style.enabled(io::stdout().is_terminal())
        } else {
            settings.style != Style::Never
        };
        io::stdout().lock().write_all(
            &document::render(passage.as_bytes(), &settings, enabled).map_err(Failure::Runtime)?,
        )?;
    } else {
        reader::run(&mut passage.as_bytes(), settings).map_err(Failure::Runtime)?;
    }
    Ok(0)
}

fn filter(text: Option<String>, settings: &Settings) -> Result<i32, Failure> {
    use std::io::IsTerminal;
    let enabled = if settings.format == OutputFormat::Ansi {
        settings.style.enabled(io::stdout().is_terminal())
    } else {
        settings.style != Style::Never
    };
    let mut output = io::stdout().lock();
    if let Some(text) = text {
        output.write_all(
            &document::render(text.as_bytes(), settings, enabled).map_err(Failure::Runtime)?,
        )?;
    } else if settings.low_latency
        && settings.format == OutputFormat::Ansi
        && settings.input_format != InputFormat::Markdown
    {
        let mut renderer = Transformer::new(settings, enabled);
        let policy = if settings.input_format == InputFormat::Terminal {
            ControlPolicy::Preserve
        } else {
            settings.controls
        };
        stream_filter(&mut renderer, &mut output, policy)?;
    } else {
        document::render_stream(&mut io::stdin().lock(), &mut output, settings, enabled)
            .map_err(Failure::Runtime)?;
    }
    output.flush()?;
    Ok(0)
}

fn stream_filter(
    renderer: &mut Transformer,
    output: &mut impl Write,
    policy: ControlPolicy,
) -> io::Result<()> {
    let mut controls = document::ControlFilter::new(policy);
    let mut input = io::stdin().lock();
    let mut buffer = [0_u8; 8192];
    let mut flush_timer = PendingFlushTimer::default();
    loop {
        let wait = flush_timer.poll_timeout(Instant::now());
        let ready = match stdin_ready(wait) {
            Ok(ready) => ready,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if ready {
            match input.read(&mut buffer) {
                Ok(0) => {
                    output.write_all(&renderer.push(&controls.finish()))?;
                    output.write_all(&renderer.finish())?;
                    return Ok(());
                }
                Ok(count) => {
                    let bytes = renderer.push(&controls.push(&buffer[..count]));
                    flush_timer.observe_input(
                        renderer.has_pending(),
                        !bytes.is_empty(),
                        Instant::now(),
                    );
                    if !bytes.is_empty() {
                        output.write_all(&bytes)?;
                        output.flush()?;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        let now = Instant::now();
        if flush_timer.is_due(now) {
            output.write_all(&renderer.flush_pending())?;
            output.flush()?;
            flush_timer.after_flush(renderer.has_pending(), Instant::now());
        }
    }
}

fn parse_length_rule(value: &str) -> Result<LengthRule, String> {
    let (maximum, fraction) = value
        .split_once(':')
        .ok_or_else(|| "expected MAX:FRACTION, for example 4:0.25".to_owned())?;
    Ok(LengthRule {
        max_length: maximum
            .parse()
            .map_err(|error| format!("MAX must be a positive integer: {error}"))?,
        fraction: fraction
            .parse()
            .map_err(|error| format!("FRACTION must be a number from zero through one: {error}"))?,
    })
}

fn stdin_ready(timeout_ms: i32) -> io::Result<bool> {
    let mut descriptor = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    // Rust's standard input API has no timed-read operation. POSIX poll lets
    // partial words flush on their deadline without waiting for more input.
    // SAFETY: descriptor is a valid, initialized pollfd for the duration of this call.
    let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result > 0)
    }
}

const PARTIAL_WORD_DEADLINE: Duration = Duration::from_millis(50);

#[derive(Default)]
struct PendingFlushTimer {
    started: Option<Instant>,
}

impl PendingFlushTimer {
    fn observe_input(&mut self, has_pending: bool, emitted_output: bool, now: Instant) {
        if !has_pending {
            self.started = None;
        } else if self.started.is_none() || emitted_output {
            self.started = Some(now);
        }
    }

    #[must_use]
    fn poll_timeout(&self, now: Instant) -> i32 {
        self.started.map_or(-1, |started| {
            let millis = PARTIAL_WORD_DEADLINE
                .saturating_sub(now.saturating_duration_since(started))
                .as_millis()
                .max(1);
            i32::try_from(millis).unwrap_or(i32::MAX)
        })
    }

    #[must_use]
    fn is_due(&self, now: Instant) -> bool {
        self.started
            .is_some_and(|started| now.saturating_duration_since(started) >= PARTIAL_WORD_DEADLINE)
    }

    fn after_flush(&mut self, has_pending: bool, now: Instant) {
        self.started = has_pending.then_some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::{PARTIAL_WORD_DEADLINE, PendingFlushTimer};
    use std::time::{Duration, Instant};

    #[test]
    fn a_new_partial_word_after_an_idle_period_gets_a_fresh_deadline() {
        let start = Instant::now();
        let mut timer = PendingFlushTimer::default();
        let before_deadline = PARTIAL_WORD_DEADLINE.saturating_sub(Duration::from_millis(1));

        timer.observe_input(true, false, start);
        let first_flush = start + PARTIAL_WORD_DEADLINE;
        assert!(timer.is_due(first_flush));
        timer.after_flush(false, first_flush);

        let idle_end = first_flush + Duration::from_millis(95);
        timer.observe_input(false, true, idle_end);
        assert_eq!(timer.poll_timeout(idle_end), -1);

        timer.observe_input(true, false, idle_end);
        assert_eq!(
            timer.poll_timeout(idle_end),
            i32::try_from(PARTIAL_WORD_DEADLINE.as_millis())
                .expect("the partial-word deadline fits in a poll timeout")
        );
        assert!(!timer.is_due(idle_end + before_deadline));
        assert!(timer.is_due(idle_end + PARTIAL_WORD_DEADLINE));
    }

    #[test]
    fn a_partial_chunk_without_output_preserves_its_deadline() {
        let start = Instant::now();
        let mut timer = PendingFlushTimer::default();
        let before_deadline = PARTIAL_WORD_DEADLINE.saturating_sub(Duration::from_millis(1));

        timer.observe_input(true, false, start);
        let next_chunk = start + Duration::from_millis(10);
        timer.observe_input(true, false, next_chunk);

        assert_eq!(
            timer.poll_timeout(next_chunk),
            i32::try_from((PARTIAL_WORD_DEADLINE - Duration::from_millis(10)).as_millis())
                .expect("the remaining partial-word deadline fits in a poll timeout")
        );
        assert!(!timer.is_due(start + before_deadline));
        assert!(timer.is_due(start + PARTIAL_WORD_DEADLINE));
    }

    #[test]
    fn emitted_output_starts_a_fresh_deadline_for_a_trailing_partial_word() {
        let start = Instant::now();
        let mut timer = PendingFlushTimer::default();
        let before_deadline = PARTIAL_WORD_DEADLINE.saturating_sub(Duration::from_millis(1));

        timer.observe_input(true, false, start);
        let output_emitted = start + Duration::from_millis(10);
        timer.observe_input(true, true, output_emitted);

        assert_eq!(
            timer.poll_timeout(output_emitted),
            i32::try_from(PARTIAL_WORD_DEADLINE.as_millis())
                .expect("the partial-word deadline fits in a poll timeout")
        );
        assert!(!timer.is_due(output_emitted + before_deadline));
        assert!(timer.is_due(output_emitted + PARTIAL_WORD_DEADLINE));
    }

    #[test]
    fn completed_input_clears_the_pending_deadline() {
        let start = Instant::now();
        let mut timer = PendingFlushTimer::default();

        timer.observe_input(true, false, start);
        timer.observe_input(false, true, start + Duration::from_millis(10));

        assert_eq!(timer.poll_timeout(start + Duration::from_secs(1)), -1);
        assert!(!timer.is_due(start + Duration::from_secs(1)));
    }
}
