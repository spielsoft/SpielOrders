//! The interface header: generated from the Rust source, committed, compiled as C, and
//! exercised by a C program against a disposable Package.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::Result;

const CRATE: &str = "crates/spielorders";
const HEADER: &str = "crates/spielorders/include/spielorders.h";
const WALKTHROUGH: &str = "crates/spielorders/tests/c/package_walkthrough.c";

fn generated(root: &Path) -> Result<Vec<u8>> {
    let crate_dir = root.join(CRATE);
    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))
        .map_err(|error| format!("cbindgen.toml: {error}"))?;
    let mut header = Vec::new();
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .map_err(|error| format!("generating the interface header: {error}"))?
        .write(&mut header);
    Ok(header)
}

/// Fails unless the committed header is exactly what the Rust source generates.
pub(crate) fn check_header(root: &Path) -> Result<()> {
    let committed = fs::read(root.join(HEADER))?;
    if committed != generated(root)? {
        return Err(format!(
            "{HEADER} is not what the interface source generates; run `cargo run -p spielorders-checks -- header write`"
        )
        .into());
    }
    Ok(())
}

pub(crate) fn write_header(root: &Path) -> Result<()> {
    fs::write(root.join(HEADER), generated(root)?)?;
    Ok(())
}

fn run(command: &mut Command) -> Result<std::process::Output> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "{command:?} failed\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output)
}

/// Builds the static library, compiles the walkthrough against the header with every
/// warning an error, and runs it against a new disposable Package.
pub(crate) fn run_walkthrough(root: &Path) -> Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let built = run(Command::new(&cargo).current_dir(root).args([
        "rustc",
        "--package",
        "spielorders",
        "--lib",
        "--locked",
        "--quiet",
        "--",
        "--print",
        "native-static-libs",
    ]))?;
    let printed = String::from_utf8_lossy(&built.stderr);
    let native: Vec<String> = printed
        .lines()
        .find_map(|line| {
            line.split_once("native-static-libs: ")
                .map(|(_, libs)| libs)
        })
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let scratch =
        std::env::temp_dir().join(format!("spielorders-walkthrough-{}", std::process::id()));
    fs::create_dir_all(&scratch)?;
    let result = (|| {
        let program = scratch.join("walkthrough");
        run(Command::new("clang")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-I"])
            .arg(root.join(CRATE).join("include"))
            .arg(root.join(WALKTHROUGH))
            .arg(target.join("debug/libspielorders.a"))
            .args(&native)
            .arg("-o")
            .arg(&program))?;
        let package = scratch.join("Package");
        fs::create_dir(&package)?;
        let walked = run(Command::new(&program).arg(&package))?;
        print!("{}", String::from_utf8_lossy(&walked.stdout));
        Ok(())
    })();
    fs::remove_dir_all(&scratch)?;
    result
}
