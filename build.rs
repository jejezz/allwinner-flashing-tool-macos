//! Makes `version:` in gui/pubspec.yaml the single source of the version.
//!
//! - The binary's `--version` is compiled from pubspec directly (`AW_VERSION`),
//!   so it is right on the very build that follows a pubspec bump.
//! - Cargo.toml's own `version` field can't be computed by Cargo, so when it
//!   has drifted this script rewrites it in place. Cargo notices the edited
//!   manifest and refreshes Cargo.lock on the next build.
//!
//! Without the GUI checkout (pubspec missing) it falls back to Cargo.toml.

use std::{env, fs, path::Path};

fn main() {
    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let pubspec = Path::new(&root).join("gui").join("pubspec.yaml");
    println!("cargo:rerun-if-changed={}", pubspec.display());
    println!("cargo:rerun-if-changed=build.rs");

    let cargo_version = env::var("CARGO_PKG_VERSION").unwrap();
    let Some(full) = fs::read_to_string(&pubspec)
        .ok()
        .and_then(|text| pubspec_version(&text))
    else {
        println!("cargo:rustc-env=AW_VERSION={cargo_version}");
        return;
    };
    println!("cargo:rustc-env=AW_VERSION={full}");

    // Cargo.toml carries only the semver part; pubspec's "+11" is Flutter's
    // build number, which the About dialog and --version still show.
    let base = full.split('+').next().unwrap();
    if base != cargo_version {
        sync_manifest(&Path::new(&root).join("Cargo.toml"), &cargo_version, base);
    }
}

/// The value of the top-level `version:` key, e.g. `0.1.1+11`.
fn pubspec_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("version:"))
        .map(|v| v.split('#').next().unwrap().trim().trim_matches(['"', '\'']).to_string())
        .filter(|v| !v.is_empty())
}

/// Replaces `version = "<old>"` inside `[package]`, leaving the rest of the
/// file (comments, dependency versions) untouched.
fn sync_manifest(manifest: &Path, old: &str, new: &str) {
    let Ok(text) = fs::read_to_string(manifest) else { return };
    let mut in_package = false;
    let mut changed = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_package = trimmed == "[package]";
            } else if in_package && !changed && trimmed == format!("version = \"{old}\"") {
                changed = true;
                return format!("version = \"{new}\"");
            }
            line.to_string()
        })
        .collect();
    if !changed {
        return;
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = lines.join(eol);
    if text.ends_with('\n') {
        out.push_str(eol);
    }
    if fs::write(manifest, out).is_ok() {
        println!("cargo:warning=Cargo.toml version {old} -> {new} (synced from gui/pubspec.yaml)");
    }
}
