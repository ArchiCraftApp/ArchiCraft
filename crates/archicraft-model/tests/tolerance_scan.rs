//! NFR-ACC-003: the model tolerance (0.01 mm, SRS Appendix D.1) is the only
//! tolerance constant of the model, geometry and snapping code.
//!
//! The scan reads the non-test source under [`SCANNED_ROOTS`] and fails on
//!
//! * a `const` or `static` whose name contains TOLERANCE, MERGE, CLOSE or
//!   EPSILON, or has a word starting with EPS (`EPS`, `HEIGHT_EPS2`, but not
//!   `STEPS`), outside `tolerance.rs`;
//! * an exponent-form float literal with a negative exponent (`1e-9`,
//!   `2.5E-12`) outside `tolerance.rs`;
//! * a second literal-initialised constant in `tolerance.rs`: every constant
//!   there other than `MODEL_TOLERANCE` is an expression of the others.
//!
//! The only exemption is the named validation ranges of `validate.rs`
//! (EDT-019 and ELM-017 thresholds, not coincidence tolerances). Comments,
//! string and character literals and `#[cfg(test)]` items are ignored.

use std::fs;
use std::path::{Path, PathBuf};

/// The scanned source trees, relative to `crates/`. The geometry crate and
/// the snapping module join once their own epsilons are retired.
const SCANNED_ROOTS: [&str; 1] = ["archicraft-model/src"];

/// The one file that defines tolerances.
const TOLERANCE_RS: &str = "archicraft-model/src/tolerance.rs";

/// The file whose validation ranges may use exponent literals.
const VALIDATE_RS: &str = "archicraft-model/src/validate.rs";

/// The validation ranges of `validate.rs`, besides every `MAX_*` constant.
const VALIDATION_RANGES: [&str; 4] = ["MIN_EDGE_LENGTH", "MIN_CORNER_ANGLE_DEG", "MIN_THICKNESS", "MIN_HEIGHT"];

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate lies in crates/")
        .to_path_buf()
}

/// The `.rs` files under `dir`, sorted.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut paths: Vec<PathBuf> = entries.map(|e| e.expect("a directory entry").path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The source with comments, string and character literals and
/// `#[cfg(test)]` items replaced by spaces. Line breaks are kept, so line
/// numbers stay valid.
fn code(source: &str) -> String {
    let chars = without_comments_and_literals(&source.chars().collect::<Vec<_>>());
    without_test_items(chars).into_iter().collect()
}

fn blank(chars: &mut [char]) {
    for c in chars.iter_mut().filter(|c| **c != '\n') {
        *c = ' ';
    }
}

fn without_comments_and_literals(source: &[char]) -> Vec<char> {
    let mut out = source.to_vec();
    let mut i = 0;
    while i < source.len() {
        let end = literal_or_comment_end(source, i);
        match end {
            Some(end) => {
                blank(&mut out[i..end]);
                i = end;
            }
            None => i += 1,
        }
    }
    out
}

/// The end of the comment or literal that starts at `i`, if one does.
fn literal_or_comment_end(s: &[char], i: usize) -> Option<usize> {
    let at = |k: usize| s.get(k).copied();
    let after_ident = i > 0 && is_ident(s[i - 1]);
    match (s[i], at(i + 1)) {
        ('/', Some('/')) => Some(s[i..].iter().position(|&c| c == '\n').map_or(s.len(), |n| i + n)),
        ('/', Some('*')) => {
            let (mut depth, mut k) = (0, i);
            while k < s.len() {
                if s[k] == '/' && at(k + 1) == Some('*') {
                    depth += 1;
                    k += 2;
                } else if s[k] == '*' && at(k + 1) == Some('/') {
                    depth -= 1;
                    k += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    k += 1;
                }
            }
            Some(k.min(s.len()))
        }
        // Raw strings: r"…", r#"…"#, br"…".
        ('r', Some('"' | '#')) if !after_ident || (s[i - 1] == 'b' && (i < 2 || !is_ident(s[i - 2]))) => {
            let hashes = s[i + 1..].iter().take_while(|&&c| c == '#').count();
            if at(i + 1 + hashes) != Some('"') {
                return None;
            }
            let mut k = i + 2 + hashes;
            while k < s.len() {
                if s[k] == '"' && (1..=hashes).all(|h| at(k + h) == Some('#')) {
                    return Some(k + 1 + hashes);
                }
                k += 1;
            }
            Some(s.len())
        }
        ('"', _) => {
            let mut k = i + 1;
            while k < s.len() && s[k] != '"' {
                k += if s[k] == '\\' { 2 } else { 1 };
            }
            Some((k + 1).min(s.len()))
        }
        // A character literal; a lifetime has no closing quote.
        ('\'', Some('\\')) => (i + 3..s.len()).find(|&k| s[k] == '\'').map(|k| k + 1),
        ('\'', Some(_)) if at(i + 2) == Some('\'') => Some(i + 3),
        _ => None,
    }
}

/// Blanks every item annotated with `#[cfg(test)]`: up to its `;`, or to
/// the end of its block.
fn without_test_items(mut chars: Vec<char>) -> Vec<char> {
    let marker: Vec<char> = "#[cfg(test)]".chars().collect();
    let mut i = 0;
    while i + marker.len() <= chars.len() {
        if chars[i..i + marker.len()] != marker[..] {
            i += 1;
            continue;
        }
        let mut k = i + marker.len();
        let mut depth = 0;
        while k < chars.len() {
            match chars[k] {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' => depth -= 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ';' if depth == 0 => break,
                _ => {}
            }
            k += 1;
        }
        let end = (k + 1).min(chars.len());
        blank(&mut chars[i..end]);
        i = end;
    }
    chars
}

/// The identifiers and number tokens of `code` with their byte offsets.
fn words(code: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in code.char_indices() {
        match (is_ident(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, &code[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &code[s..]));
    }
    out
}

/// A `const` or `static` item with an initialiser.
#[derive(Debug)]
struct Constant {
    line: usize,
    name: String,
    /// Byte range of the initialiser.
    value: std::ops::Range<usize>,
}

fn line_of(code: &str, offset: usize) -> usize {
    code[..offset].matches('\n').count() + 1
}

fn constants(code: &str) -> Vec<Constant> {
    let words = words(code);
    let mut out = Vec::new();
    for (k, &(_, word)) in words.iter().enumerate() {
        if word != "const" && word != "static" {
            continue;
        }
        let n = if words.get(k + 1).is_some_and(|w| w.1 == "mut") {
            k + 2
        } else {
            k + 1
        };
        let Some(&(at, name)) = words.get(n) else {
            continue;
        };
        let after_name = at + name.len();
        let rest = &code[after_name..];
        // `const fn`, `const _` and `*const T` are not constants.
        if name == "fn" || name == "_" || !rest.trim_start().starts_with(':') || rest.trim_start().starts_with("::") {
            continue;
        }
        let Some(eq) = initialiser_start(rest) else {
            continue;
        };
        let start = after_name + eq + 1;
        out.push(Constant {
            line: line_of(code, at),
            name: name.to_owned(),
            value: start..start + initialiser_end(&code[start..]),
        });
    }
    out
}

/// The offset of the `=` that ends a constant's type, or `None` if the type
/// ends first: a generic `const N: usize` parameter (closed by `>`, `,` or a
/// brace) or a declaration without an initialiser.
fn initialiser_start(after_name: &str) -> Option<usize> {
    let mut depth = 0;
    let mut previous = ' ';
    for (i, c) in after_name.char_indices() {
        match c {
            '=' if depth == 0 => return Some(i),
            '(' | '[' | '<' => depth += 1,
            '>' if previous == '-' => {}
            ')' | ']' | '>' if depth == 0 => return None,
            ')' | ']' | '>' => depth -= 1,
            ',' | ';' if depth == 0 => return None,
            '{' | '}' => return None,
            _ => {}
        }
        previous = c;
    }
    None
}

/// The length of an initialiser: up to the first `;` outside brackets.
fn initialiser_end(value: &str) -> usize {
    let mut depth = 0;
    for (i, c) in value.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ';' if depth <= 0 => return i,
            _ => {}
        }
    }
    value.len()
}

fn is_tolerance_name(name: &str) -> bool {
    let upper = name.to_uppercase();
    ["TOLERANCE", "MERGE", "CLOSE", "EPSILON"]
        .iter()
        .any(|w| upper.contains(w))
        || upper.split('_').any(|word| word.starts_with("EPS"))
}

/// Exponent-form float literals with a negative exponent
/// (`\d(\.\d+)?[eE]-\d+`), with their byte offsets.
fn exponent_literals(code: &str) -> Vec<(usize, &str)> {
    let bytes = code.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    let mut out = Vec::new();
    for i in 1..bytes.len() {
        if !(matches!(bytes[i], b'e' | b'E') && digit(i - 1) && bytes.get(i + 1) == Some(&b'-') && digit(i + 2)) {
            continue;
        }
        let mut start = i;
        while start > 0 && (bytes[start - 1].is_ascii_digit() || matches!(bytes[start - 1], b'.' | b'_')) {
            start -= 1;
        }
        // The literal starts a token: `x1e-5` is a subtraction.
        if start > 0 && (bytes[start - 1].is_ascii_alphabetic() || bytes[start - 1] == b'_') {
            continue;
        }
        let mut end = i + 2;
        while digit(end) || bytes.get(end) == Some(&b'_') {
            end += 1;
        }
        out.push((start, &code[start..end]));
    }
    out
}

/// The rule violations of one file; `path` ends in its crate directory.
fn violations(path: &Path, source: &str) -> Vec<String> {
    let code = code(source);
    let display = path.display();
    let constants = constants(&code);
    if path.ends_with(TOLERANCE_RS) {
        let names: Vec<&str> = constants.iter().map(|c| c.name.as_str()).collect();
        let mut out: Vec<String> = constants
            .iter()
            .filter(|c| c.name != "MODEL_TOLERANCE")
            .filter(|c| !words(&code[c.value.clone()]).iter().any(|(_, w)| names.contains(w)))
            .map(|c| {
                format!(
                    "{display}:{}: {} is not an expression of MODEL_TOLERANCE",
                    c.line, c.name
                )
            })
            .collect();
        if !names.contains(&"MODEL_TOLERANCE") {
            out.push(format!("{display}: MODEL_TOLERANCE is missing"));
        }
        return out;
    }
    let is_range = |c: &&Constant| {
        path.ends_with(VALIDATE_RS) && (VALIDATION_RANGES.contains(&c.name.as_str()) || c.name.starts_with("MAX_"))
    };
    let ranges: Vec<&Constant> = constants.iter().filter(is_range).collect();
    let named = constants.iter().filter(|c| is_tolerance_name(&c.name)).map(|c| {
        format!(
            "{display}:{}: tolerance constant {} outside tolerance.rs",
            c.line, c.name
        )
    });
    let literals = exponent_literals(&code)
        .into_iter()
        .filter(|(at, _)| !ranges.iter().any(|c| c.value.contains(at)))
        .map(|(at, literal)| {
            format!(
                "{display}:{}: exponent literal {literal} outside tolerance.rs",
                line_of(&code, at)
            )
        });
    named.chain(literals).collect()
}

#[test]
fn only_model_tolerance_is_a_tolerance_constant() {
    let crates = crates_dir();
    let mut files = Vec::new();
    for root in SCANNED_ROOTS {
        rust_files(&crates.join(root), &mut files);
    }
    for required in [TOLERANCE_RS, VALIDATE_RS] {
        assert!(files.iter().any(|f| f.ends_with(required)), "{required} is not scanned");
    }
    let problems: Vec<String> = files
        .iter()
        .flat_map(|path| {
            let source = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let relative = path.strip_prefix(&crates).unwrap_or(path);
            violations(relative, &source)
        })
        .collect();
    assert!(
        problems.is_empty(),
        "use archicraft_model::tolerance instead (NFR-ACC-003):\n{}",
        problems.join("\n")
    );
}

// The scanner itself.

fn check(path: &str, source: &str) -> Vec<String> {
    violations(Path::new(path), source)
}

#[test]
fn scanner_finds_tolerance_constants_and_exponent_literals() {
    let other = "archicraft-geometry/src/wall.rs";
    assert_eq!(check(other, "pub const JOIN_TOLERANCE: f64 = 0.0001;").len(), 1);
    assert_eq!(check(other, "pub(crate) const EPS: f64 = 0.5;").len(), 1);
    assert_eq!(check(other, "const HEIGHT_EPS2: f64 = 0.5;").len(), 1);
    assert_eq!(check(other, "static HEIGHT_EPSILON: f64 = 0.5;").len(), 1);
    assert_eq!(check(other, "static mut MERGE: f64 = 0.5;").len(), 1);
    assert_eq!(check(other, "const CLOSE_STEPS: [f64; 2] = [0.5, 0.25];").len(), 1);
    assert_eq!(check(other, "fn f(a: f64) -> bool { a < 1e-9 }").len(), 1);
    assert_eq!(check(other, "let k = (p.x * 2.5E-12) as i64;").len(), 1);
    assert_eq!(check(other, "let k = 1_000.0e-1_2;").len(), 1);
    assert_eq!(check(other, "const EPS: f64 = 1e-9;").len(), 2);
    let found = check(other, "\n\nconst MERGE_STEP: f64 = 1e-6;");
    assert_eq!(
        found,
        [
            "archicraft-geometry/src/wall.rs:3: tolerance constant MERGE_STEP outside tolerance.rs",
            "archicraft-geometry/src/wall.rs:3: exponent literal 1e-6 outside tolerance.rs",
        ]
    );
    // A tolerance.rs elsewhere is not the tolerance module.
    assert_eq!(
        check("archicraft-geometry/src/tolerance.rs", "const EPS: f64 = 1e-9;").len(),
        2
    );
}

#[test]
fn scanner_ignores_tests_comments_literals_and_other_names() {
    let clean = r##"
        //! Points closer than 1e-5 m coincide.
        /* nested /* 1e-9 */ still a comment 1e-9 */
        const STEPS: usize = 4;
        const MAX_DEPTH: f64 = 1e5;
        const fn half(x: f64) -> f64 { x / 2.0 }
        fn f<'a>(s: &'a str) -> char { let _ = "1e-9 \" 1e-9"; let _ = r#"1e-9 "quoted""#; '"' }
        fn g() -> (f64, u8, char) { let x1e = 3.0; (x1e-5.0, b'e', '\'') }
        fn h(p: *const u8) -> usize { let _ = br"1e-9"; std::mem::size_of::<[u8; 3]>() }
        struct Grid<const N: usize>;
        #[cfg(test)]
        mod tests {
            const EPS: f64 = 1e-12;
            fn h() { assert!(0.0 < 1e-9); }
        }
        #[cfg(test)]
        const CLOSE_TOLERANCE: f64 = 1e-6;
        #[cfg(test)]
        fn sized() -> [f64; 2] { [1e-9, 2e-9] }
        fn after_tests() {}
    "##;
    assert_eq!(check("archicraft-model/src/x.rs", clean), Vec::<String>::new());
    // Code after a test module is still scanned.
    let after = "#[cfg(test)]\nmod tests { fn f() {} }\nconst EPS: f64 = 0.1;";
    assert_eq!(check("archicraft-model/src/x.rs", after).len(), 1);
}

#[test]
fn only_the_validation_ranges_of_validate_rs_are_exempt() {
    let ranges = "pub const MIN_EDGE_LENGTH: f64 = 1e-4;\npub const MAX_SPAN: f64 = 2e-3;\n";
    assert!(check(VALIDATE_RS, ranges).is_empty());
    assert_eq!(check(VALIDATE_RS, "pub const MIN_SPAN: f64 = 1e-4;").len(), 1);
    assert_eq!(check(VALIDATE_RS, "pub const MODEL_TOLERANCE: f64 = 1e-5;").len(), 2);
    let same_line = "pub const MIN_HEIGHT: f64 = 1e-3; fn f(a: f64) -> bool { a < 1e-9 }";
    assert_eq!(check(VALIDATE_RS, same_line).len(), 1);
    assert_eq!(check("archicraft-model/src/other.rs", ranges).len(), 2);
}

#[test]
fn tolerance_rs_has_one_literal_constant() {
    let good = "pub const MODEL_TOLERANCE: f64 = 1e-5;\n\
                pub const JOIN_TOLERANCE: f64 = MODEL_TOLERANCE;\n\
                pub const ANGLE_TOLERANCE: f64 = MODEL_TOLERANCE * 0.5;\n\
                pub const STEPS: [f64; 2] = [ANGLE_TOLERANCE; 2];\n\
                pub fn near_zero(v: f64, s: f64) -> bool { v.abs() <= 64.0 * f64::EPSILON * s }\n\
                const fn around(i: i64) -> [i64; 3] { [i, i - 1, i + 1] }\n\
                struct Grid<const N: usize>;\n\
                #[cfg(test)]\nmod tests { const MICROMETRE: f64 = 1e-6; }\n";
    assert_eq!(check(TOLERANCE_RS, good), Vec::<String>::new());
    let second = format!("{good}pub const MERGE_STEP: f64 = 1e-6;\n");
    assert_eq!(
        check(TOLERANCE_RS, &second),
        ["archicraft-model/src/tolerance.rs:10: MERGE_STEP is not an expression of MODEL_TOLERANCE"]
    );
    assert_eq!(check(TOLERANCE_RS, "pub const JOIN_TOLERANCE: f64 = 0.0001;").len(), 2);
}
