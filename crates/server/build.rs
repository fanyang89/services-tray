//! Build script: keep the embedded web UI (frontend/dist) up to date.
//!
//! - Freshness is content-based: every frontend input file (sources, configs,
//!   lockfile) is hashed; the digest is stored in dist/.web-build-stamp.
//! - When stale, runs `pnpm install --prefer-offline` + `pnpm build`.
//! - `SM_SKIP_WEB_BUILD=1` skips the build entirely (a placeholder
//!   index.html is created if dist is missing so rust-embed still compiles).
//! - If pnpm is unavailable but a previous dist exists, it is reused with a
//!   warning so release tarballs build without a Node toolchain.
//! - A lock file serializes concurrent cargo invocations.

use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const SKIP_ENV: &str = "SM_SKIP_WEB_BUILD";
const LOCK_TIMEOUT: Duration = Duration::from_secs(90);

fn main() {
    let manifest_dir = PathBuf::from(env_var("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("expected the crates/server layout")
        .to_path_buf();
    let frontend = repo_root.join("frontend");
    let dist = frontend.join("dist");
    let stamp = dist.join(".web-build-stamp");
    let lock_path = frontend.join(".web-build.lock");

    // Re-run when frontend inputs change. Never watch dist/ itself (loop).
    for path in watched(&frontend) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rerun-if-env-changed={SKIP_ENV}");

    if skip_requested() {
        ensure_placeholder(&dist);
        println!("cargo:warning={SKIP_ENV} is set: skipping web UI build");
        return;
    }

    let digest = hash_tree(&frontend);
    if stamp_current(&stamp, digest) {
        return; // dist is up to date
    }

    let _lock = LockGuard::acquire(&lock_path);
    // Another cargo may have finished the build while we waited for the lock.
    if stamp_current(&stamp, digest) {
        return;
    }

    let Some(pnpm) = find_pnpm() else {
        if dist.join("index.html").exists() {
            println!(
                "cargo:warning=pnpm not found: embedding the existing (possibly stale) frontend/dist"
            );
            return;
        }
        panic!(
            "pnpm not found and frontend/dist is missing: install Node.js and pnpm, \
             or set {SKIP_ENV}=1 to skip building the web UI"
        );
    };

    println!("cargo:warning=rebuilding web UI (frontend inputs changed)");
    run(&pnpm, &frontend, &["install", "--prefer-offline"]);
    run(&pnpm, &frontend, &["build"]);

    if !dist.join("index.html").exists() {
        panic!(
            "`pnpm build` finished but {} was not produced",
            dist.join("index.html").display()
        );
    }
    fs::write(&stamp, format!("{digest:016x}\n"))
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", stamp.display()));
}

fn env_var(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("missing env var {key}"))
}

fn skip_requested() -> bool {
    matches!(env::var(SKIP_ENV).as_deref(), Ok("1" | "true" | "yes"))
}

/// Files/directories whose changes re-trigger this build script.
fn watched(frontend: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for name in [
        "package.json",
        "pnpm-lock.yaml",
        "vite.config.ts",
        "tsconfig.json",
        "index.html",
    ] {
        let path = frontend.join(name);
        if path.exists() {
            out.push(path);
        }
    }
    for dir in ["src", "public"] {
        let path = frontend.join(dir);
        if path.is_dir() {
            out.push(path);
        }
    }
    // Watch the built entry file too (not the whole dist dir):
    // - if dist is deleted, cargo re-runs this script so the UI is rebuilt
    //   (or a placeholder is created when the build is skipped);
    // - a missing watched path also marks the script dirty, which is exactly
    //   the "dist absent" signal we need.
    // Watching just index.html (never the assets) keeps this loop-free: our
    // own build rewrites it once, then the fingerprint stays stable.
    out.push(frontend.join("dist").join("index.html"));
    out
}

/// Hash every input file's path and content. Content-based (not mtime-based)
/// so branch switches and re-checkouts with equal mtimes are still detected,
/// while plain `touch` does not trigger a rebuild.
fn hash_tree(frontend: &Path) -> u64 {
    let mut files = BTreeMap::new();
    collect_files(frontend, frontend, &mut files);
    let mut hasher = DefaultHasher::new();
    for (rel, data) in &files {
        rel.hash(&mut hasher);
        hasher.write(&(data.len() as u64).to_le_bytes());
        hasher.write(data);
    }
    hasher.finish()
}

fn collect_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "node_modules"
            || name == "dist"
            || name == ".git"
            || name == ".web-build.lock"
            || name.starts_with(".DS_Store")
        {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if path.is_file() {
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            if let Ok(data) = fs::read(&path) {
                out.insert(rel.to_string_lossy().into_owned(), data);
            }
        }
    }
}

fn stamp_current(stamp: &Path, digest: u64) -> bool {
    fs::read_to_string(stamp)
        .map(|s| s.trim() == format!("{digest:016x}"))
        .unwrap_or(false)
}

fn find_pnpm() -> Option<String> {
    let name = if cfg!(windows) { "pnpm.cmd" } else { "pnpm" };
    match Command::new(name).arg("--version").output() {
        Ok(output) if output.status.success() => Some(name.to_string()),
        _ => None,
    }
}

fn run(program: &str, cwd: &Path, args: &[&str]) {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `{program} {}`: {e}", args.join(" ")));
    if !output.status.success() {
        println!(
            "cargo:warning=--- `{program} {}` failed ---",
            args.join(" ")
        );
        println!("cargo:warning={}", String::from_utf8_lossy(&output.stdout));
        println!("cargo:warning={}", String::from_utf8_lossy(&output.stderr));
        panic!(
            "`{program} {}` exited with {}",
            args.join(" "),
            output.status
        );
    }
}

/// Keep dist/ compilable when the build is skipped.
fn ensure_placeholder(dist: &Path) {
    if !dist.join("index.html").exists() {
        fs::create_dir_all(dist).expect("create dist dir");
        fs::write(dist.join("index.html"), "<!-- web UI build skipped -->\n")
            .expect("write placeholder index.html");
    }
}

/// Cross-process lock (mkdir-style) so parallel cargo invocations (build +
/// clippy) don't race on frontend/dist.
struct LockGuard(PathBuf);

impl LockGuard {
    fn acquire(path: &Path) -> Option<Self> {
        let deadline = Instant::now() + LOCK_TIMEOUT;
        loop {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    return Some(Self(path.to_path_buf()));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline {
                        println!(
                            "cargo:warning=web build lock held for over {LOCK_TIMEOUT:?}; proceeding without it"
                        );
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                // E.g. read-only filesystem: proceed best-effort.
                Err(_) => return None,
            }
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
