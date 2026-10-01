//! A Rust take on `json-linter PATH... --fix --config-set indent=2`
//! (json-linter 1.3.0, https://github.com/atomicptr/json-linter): sorts object
//! keys naturally, rewrites files with 2-space indent, then lints them.

mod natural;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::{env, fs};

use serde_json::Value;

const COLOR_GREEN: &str = "\x1b[92m";
const COLOR_RED: &str = "\x1b[91m";
const COLOR_RESET: &str = "\x1b[00m";

/// One rule outcome: its name and the failure message, if it failed.
type RuleResult = (&'static str, Option<String>);

fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a.as_bytes().starts_with(b"-")) {
        eprintln!("usage: json-lint PATH [PATH ...]");
        eprintln!("Fixes and lints like: json-linter PATH... --fix --config-set indent=2");
        std::process::exit(2);
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let code = run(&args, &mut out);
    let _ = out.flush();
    std::process::exit(code);
}

fn run(args: &[std::ffi::OsString], out: &mut impl Write) -> i32 {
    let cwd = normalize(b"", env::current_dir().unwrap().as_os_str().as_bytes());
    let files: Vec<Vec<u8>> = args
        .iter()
        .flat_map(|a| gather_files(&cwd, a.as_bytes()))
        .collect();
    if files.is_empty() {
        let _ = writeln!(out, "No files found.");
        return 1;
    }

    let mut per_path: BTreeMap<Vec<u8>, Vec<RuleResult>> = BTreeMap::new();
    for file in &files {
        let results = per_path.entry(display_path(file, &cwd)).or_default();
        results.extend(fix_and_lint(file));
    }

    print_header(out, "json-linter", None);
    let (mut passed, mut total) = (0, 0);
    for (path, results) in &per_path {
        let _ = out.write_all(path);
        let _ = out.write_all(b" ");
        for (_, err) in results {
            let (color, marker) = if err.is_none() {
                (COLOR_GREEN, '.')
            } else {
                (COLOR_RED, 'F')
            };
            let _ = write!(out, "{color}{marker}{COLOR_RESET}");
        }
        let _ = writeln!(out);
        for (name, err) in results {
            if let Some(msg) = err {
                let msg = if msg.is_empty() {
                    String::new()
                } else {
                    format!(": {msg}")
                };
                let _ = writeln!(out, "\t{COLOR_RED}{name}{msg}{COLOR_RESET}");
            }
        }
        total += results.len();
        passed += results.iter().filter(|(_, e)| e.is_none()).count();
    }
    print_header(
        out,
        &format!("{passed} / {total} passed"),
        Some(COLOR_GREEN),
    );
    if passed == total { 0 } else { 1 }
}

/// Rewrites the file with naturally sorted keys and 2-space indent, then
/// returns the rule results. Unparseable files are reported and left alone.
fn fix_and_lint(path: &[u8]) -> Vec<RuleResult> {
    let value = match fix_file(path) {
        Ok(v) => v,
        Err(e) => return vec![("lint_file", Some(e))],
    };
    let sorted = natural::keys_are_sorted(&value);
    vec![
        ("rule_keys_are_sorted", (!sorted).then(String::new)),
        // No naming style is configured, so every key name is accepted.
        ("rule_naming_style_is_correct", None),
    ]
}

fn fix_file(path: &[u8]) -> Result<Value, String> {
    let p = as_path(path);
    let data = fs::read(p).map_err(|e| format!("IOError: {e}"))?;
    let mut value: Value =
        serde_json::from_slice(&data).map_err(|e| format!("JSONDecodeError: {e}"))?;
    natural::sort_keys(&mut value);
    let mut fixed = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    fixed.push(b'\n');
    fs::write(p, fixed).map_err(|e| format!("IOError: {e}"))?;
    Ok(value)
}

// ------------------------------------------------------------------- files --

/// Joins `path` onto `base` and normalizes it lexically, like `pathlib.Path`.
fn normalize(base: &[u8], path: &[u8]) -> Vec<u8> {
    let joined = if path.starts_with(b"/") {
        path.to_vec()
    } else {
        [base, b"/", path].concat()
    };
    let mut out = Vec::with_capacity(joined.len());
    for part in joined
        .split(|&c| c == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        out.push(b'/');
        out.extend_from_slice(part);
    }
    if out.is_empty() {
        out.push(b'/');
    }
    out
}

fn as_path(p: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(p))
}

/// The file itself, or a directory's `*.json` entries (non-recursive, sorted).
fn gather_files(cwd: &[u8], arg: &[u8]) -> Vec<Vec<u8>> {
    let path = normalize(cwd, arg);
    let p = as_path(&path);
    if p.is_file() {
        return vec![path];
    }
    let Ok(entries) = fs::read_dir(p) else {
        return vec![];
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file() && e.file_name().as_bytes().ends_with(b".json"))
        .map(|e| normalize(&path, e.file_name().as_bytes()))
        .collect();
    files.sort();
    files
}

/// The path relative to cwd when it is inside it, otherwise absolute.
fn display_path(path: &[u8], cwd: &[u8]) -> Vec<u8> {
    if cwd == b"/" {
        return path[1..].to_vec();
    }
    match path.strip_prefix(cwd) {
        Some(rest) if rest.starts_with(b"/") => rest[1..].to_vec(),
        _ => path.to_vec(),
    }
}

// ------------------------------------------------------------------ output --

/// The text centered in `=` signs, 80 per side when stdout is not a terminal.
fn print_header(out: &mut impl Write, text: &str, color: Option<&str>) {
    let width = match terminal_columns() {
        Some(cols) => (cols as f64 / 2.0 - (text.chars().count() as f64 + 2.0) / 2.0).floor(),
        None => 80.0,
    };
    let segment = "=".repeat(width.max(0.0) as usize);
    let _ = match color {
        Some(c) => writeln!(out, "{c}{segment} {text} {segment}{COLOR_RESET}"),
        None => writeln!(out, "{segment} {text} {segment}"),
    };
}

/// Width of the terminal attached to stdout, if any.
fn terminal_columns() -> Option<u16> {
    // SAFETY: TIOCGWINSZ only writes into the provided winsize struct.
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        (libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0).then_some(ws.ws_col)
    }
}
