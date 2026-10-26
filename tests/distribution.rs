//! Smoke tests for archive assets and the npm launcher.

#![cfg(unix)]

use std::fs;
use std::fs::Permissions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[must_use]
fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

#[must_use]
fn node_native_package() -> Option<String> {
    let probe = r"
      const arch = process.arch === 'x64' ? 'x64' : process.arch === 'arm64' ? 'arm64' : null;
      if (process.platform === 'darwin' && arch) {
        console.log(`@arturoaviles/bionic-reader-darwin-${arch}`);
      } else if (process.platform === 'linux' && arch && process.report && process.report.getReport) {
        const report = process.report.getReport();
        const libc = report.header.glibcVersionRuntime ? 'gnu' : 'musl';
        console.log(`@arturoaviles/bionic-reader-linux-${arch}-${libc}`);
      }
    ";
    let output = Command::new("node").args(["-e", probe]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let package = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!package.is_empty()).then_some(package)
}

#[must_use]
fn create_layout(root: &Path, native_package: &str, executable: &Path) -> PathBuf {
    let base = root.join("node_modules/@arturoaviles/bionic-reader");
    let native = root.join("node_modules").join(native_package).join("bin");
    fs::create_dir_all(base.join("bin")).expect("create npm launcher directory");
    fs::create_dir_all(&native).expect("create native package directory");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/npm/bin/bionic-reader.js"),
        base.join("bin/bionic-reader.js"),
    )
    .expect("copy npm launcher into test package layout");
    fs::copy(executable, native.join("bionic-reader"))
        .expect("copy native executable into test package layout");
    base.join("bin/bionic-reader.js")
}

fn assert_asset_tree_is_archived(source: &Path, root: &Path, top: &str, listing: &str) {
    let entries = fs::read_dir(source).expect("read distribution asset directory");
    for entry in entries {
        let entry = entry.expect("read distribution asset entry");
        let path = entry.path();
        let file_type = entry.file_type().expect("read distribution asset type");
        if file_type.is_dir() {
            assert_asset_tree_is_archived(&path, root, top, listing);
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("distribution asset should be under the repository root");
            let expected = format!("{top}/share/bionic-reader/{}", relative.display());
            assert!(
                listing.lines().any(|line| line == expected),
                "missing {expected}"
            );
        }
    }
}

#[test]
fn archive_candidates_are_reproducible_and_include_distribution_assets() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipping archive smoke test because python3 is unavailable");
        return;
    }
    let temporary = tempfile::tempdir().expect("create temporary archive directory");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary = env!("CARGO_BIN_EXE_bionic-reader");
    let outputs = [
        temporary.path().join("first.tar.gz"),
        temporary.path().join("second.tar.gz"),
    ];

    for output in &outputs {
        let result = Command::new("python3")
            .arg(root.join("scripts/create_archive.py"))
            .args([
                "--binary",
                binary,
                "--version",
                env!("CARGO_PKG_VERSION"),
                "--target",
                "native",
                "--epoch",
                "1_700_000_000",
            ])
            .arg("--output")
            .arg(output)
            .arg("--root")
            .arg(root)
            .output()
            .expect("create archive candidate");
        assert!(
            result.status.success(),
            "archive creation failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(
        fs::read(&outputs[0]).expect("read first archive candidate"),
        fs::read(&outputs[1]).expect("read second archive candidate"),
        "archive candidates should be byte-for-byte reproducible"
    );

    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(&outputs[0])
        .output()
        .expect("list archive contents");
    assert!(
        listing.status.success(),
        "tar failed to list the archive with status {:?}: {}",
        listing.status,
        String::from_utf8_lossy(&listing.stderr)
    );
    let listing = String::from_utf8(listing.stdout).expect("tar listing should be valid UTF-8");
    assert!(
        listing.contains("/bin/bionic-reader"),
        "archive should include the CLI"
    );
    assert!(
        listing.contains("/share/bionic-reader/config.example.toml"),
        "archive should include the example configuration"
    );
    let top = format!("bionic-reader-{}-native", env!("CARGO_PKG_VERSION"));
    for asset_directory in ["integrations", "completions"] {
        assert_asset_tree_is_archived(&root.join(asset_directory), root, &top, &listing);
    }
    assert!(
        listing.contains("/share/bionic-reader/completions/bionic-reader.bash"),
        "archive should include Bash completions"
    );
    assert!(
        listing.contains("/share/bionic-reader/completions/_bionic-reader"),
        "archive should include Zsh completions"
    );
    assert!(
        listing.contains("/share/bionic-reader/completions/bionic-reader.fish"),
        "archive should include Fish completions"
    );
}

#[test]
fn npm_launcher_runs_the_native_binary_and_preserves_status_and_spawn_errors() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    if !node_available() {
        eprintln!("skipping npm launcher smoke test because node is unavailable");
        return;
    }
    let Some(native_package) = node_native_package() else {
        eprintln!("skipping npm launcher smoke test on an unsupported Node platform");
        return;
    };
    let temporary = tempfile::tempdir().expect("create temporary npm package directory");
    let launcher = create_layout(
        temporary.path(),
        &native_package,
        Path::new(env!("CARGO_BIN_EXE_bionic-reader")),
    );

    let result = Command::new("node")
        .arg(&launcher)
        .args(["-t", "Bionic Reader", "--style", "never"])
        .output()
        .expect("run npm launcher with explicit text");
    assert_eq!(
        result.status.code(),
        Some(0),
        "npm launcher for explicit text exited with status {:?}",
        result.status
    );
    assert_eq!(result.stdout, b"Bionic Reader");

    let mut command = Command::new("node");
    command
        .arg(&launcher)
        .args(["--style", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn npm launcher for stdin test");
    child
        .stdin
        .take()
        .expect("npm launcher should have piped stdin")
        .write_all(b"stdin bytes")
        .expect("write stdin bytes to npm launcher");
    let result = child
        .wait_with_output()
        .expect("collect npm launcher stdin result");
    assert_eq!(
        result.status.code(),
        Some(0),
        "npm launcher for stdin input exited with status {:?}",
        result.status
    );
    assert_eq!(result.stdout, b"stdin bytes");

    let missing_config = temporary.path().join("missing.toml");
    let result = Command::new("node")
        .arg(&launcher)
        .arg("--config")
        .arg(missing_config)
        .output()
        .expect("run npm launcher with missing config");
    assert_eq!(
        result.status.code(),
        Some(2),
        "npm launcher should report a missing config file"
    );

    let native_binary = temporary
        .path()
        .join("node_modules")
        .join(native_package)
        .join("bin/bionic-reader");
    fs::set_permissions(&native_binary, Permissions::from_mode(0o644))
        .expect("remove executable permission from native binary");
    let result = Command::new("node")
        .arg(&launcher)
        .arg("--version")
        .output()
        .expect("run npm launcher with non-executable native binary");
    assert_eq!(
        result.status.code(),
        Some(1),
        "npm launcher with a non-executable native binary returned status {:?}",
        result.status
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("could not start native executable"),
        "npm launcher should explain the native executable spawn failure"
    );
}
