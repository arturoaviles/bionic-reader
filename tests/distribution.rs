#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

fn node_native_package() -> Option<String> {
    let probe = r#"
      const arch = process.arch === 'x64' ? 'x64' : process.arch === 'arm64' ? 'arm64' : null;
      if (process.platform === 'darwin' && arch) {
        console.log(`@arturoaviles/bionic-reader-darwin-${arch}`);
      } else if (process.platform === 'linux' && arch && process.report && process.report.getReport) {
        const report = process.report.getReport();
        const libc = report.header.glibcVersionRuntime ? 'gnu' : 'musl';
        console.log(`@arturoaviles/bionic-reader-linux-${arch}-${libc}`);
      }
    "#;
    let output = Command::new("node").args(["-e", probe]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let package = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!package.is_empty()).then_some(package)
}

fn create_layout(root: &Path, native_package: &str, executable: &Path) -> PathBuf {
    let base = root.join("node_modules/@arturoaviles/bionic-reader");
    let native = root.join("node_modules").join(native_package).join("bin");
    fs::create_dir_all(base.join("bin")).unwrap();
    fs::create_dir_all(&native).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/npm/bin/bionic-reader.js"),
        base.join("bin/bionic-reader.js"),
    )
    .unwrap();
    fs::copy(executable, native.join("bionic-reader")).unwrap();
    base.join("bin/bionic-reader.js")
}

fn assert_asset_tree_is_archived(source: &Path, root: &Path, top: &str, listing: &str) {
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            assert_asset_tree_is_archived(&path, root, top, listing);
        } else if entry.file_type().unwrap().is_file() {
            let relative = path.strip_prefix(root).unwrap();
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
    let temporary = tempfile::tempdir().unwrap();
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
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(
        fs::read(&outputs[0]).unwrap(),
        fs::read(&outputs[1]).unwrap()
    );

    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(&outputs[0])
        .output()
        .unwrap();
    assert!(listing.status.success());
    let listing = String::from_utf8(listing.stdout).unwrap();
    assert!(listing.contains("/bin/bionic-reader"));
    assert!(listing.contains("/share/bionic-reader/config.example.toml"));
    let top = format!("bionic-reader-{}-native", env!("CARGO_PKG_VERSION"));
    for asset_directory in ["integrations", "completions"] {
        assert_asset_tree_is_archived(&root.join(asset_directory), root, &top, &listing);
    }
    assert!(listing.contains("/share/bionic-reader/completions/bionic-reader.bash"));
    assert!(listing.contains("/share/bionic-reader/completions/_bionic-reader"));
    assert!(listing.contains("/share/bionic-reader/completions/bionic-reader.fish"));
}

#[test]
fn npm_launcher_runs_the_native_binary_and_preserves_status_and_spawn_errors() {
    if !node_available() {
        eprintln!("skipping npm launcher smoke test because node is unavailable");
        return;
    }
    let Some(native_package) = node_native_package() else {
        eprintln!("skipping npm launcher smoke test on an unsupported Node platform");
        return;
    };
    let temporary = tempfile::tempdir().unwrap();
    let launcher = create_layout(
        temporary.path(),
        &native_package,
        Path::new(env!("CARGO_BIN_EXE_bionic-reader")),
    );

    let result = Command::new("node")
        .arg(&launcher)
        .args(["-t", "Bionic Reader", "--style", "never"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(result.stdout, b"Bionic Reader");

    let mut command = Command::new("node");
    command
        .arg(&launcher)
        .args(["--style", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"stdin bytes")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(result.stdout, b"stdin bytes");

    let missing_config = temporary.path().join("missing.toml");
    let result = Command::new("node")
        .arg(&launcher)
        .arg("--config")
        .arg(missing_config)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));

    let native_binary = temporary
        .path()
        .join("node_modules")
        .join(native_package)
        .join("bin/bionic-reader");
    fs::set_permissions(&native_binary, fs::Permissions::from_mode(0o644)).unwrap();
    let result = Command::new("node")
        .arg(&launcher)
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("could not start native executable"));
}
