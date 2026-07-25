//! CODEOWNERS path patterns.
//!
//! These look like gitignore patterns and are *documented* as following "most
//! of the same rules", but the differences are where every buggy
//! implementation lives. This module implements the documented CODEOWNERS
//! behaviour, not gitignore behaviour. See the crate-level docs for the list
//! of deliberate divergences.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Why a pattern could not be compiled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatternError {
    /// The pattern was empty.
    Empty,
    /// `!` negation. Explicitly unsupported by GitHub in CODEOWNERS files.
    Negation,
    /// `[` or `]` character range. Explicitly unsupported by GitHub.
    CharacterRange,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Empty => "empty pattern",
            Self::Negation => "`!` negation is not supported in CODEOWNERS",
            Self::CharacterRange => "`[ ]` character ranges are not supported in CODEOWNERS",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// `**` — matches zero or more whole path segments.
    AnyDepth,
    /// No wildcards.
    Literal(String),
    /// Contains `*` or `?`, but never crosses a `/`.
    Glob(String),
}

impl Segment {
    fn new(s: &str) -> Self {
        if s == "**" {
            Self::AnyDepth
        } else if s.contains('*') || s.contains('?') {
            Self::Glob(s.to_owned())
        } else {
            Self::Literal(s.to_owned())
        }
    }

    fn matches(&self, text: &str) -> bool {
        match self {
            Self::AnyDepth => true,
            Self::Literal(l) => l == text,
            Self::Glob(g) => glob_match(g, text),
        }
    }
}

/// A compiled CODEOWNERS path pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    raw: String,
    segments: Vec<Segment>,
    /// Pattern is tied to the repository root rather than matching at any depth.
    anchored: bool,
    /// Pattern ended with `/`, so it only matches *inside* a directory.
    dir_only: bool,
    /// Final segment has no wildcard, so the pattern may also name a directory
    /// and match everything beneath it.
    trailing_literal: bool,
}

impl Pattern {
    /// Compile a raw pattern string.
    ///
    /// # Errors
    ///
    /// Returns [`PatternError`] for empty patterns and for the gitignore
    /// features GitHub documents as non-functional in CODEOWNERS.
    pub fn new(raw: &str) -> Result<Self, PatternError> {
        if raw.is_empty() {
            return Err(PatternError::Empty);
        }
        if raw.starts_with('!') {
            return Err(PatternError::Negation);
        }
        if raw.contains('[') || raw.contains(']') {
            return Err(PatternError::CharacterRange);
        }

        // A pattern is anchored if it contains a `/` anywhere except as the
        // final character. `docs/` is not anchored; `/docs` and `docs/x` are.
        let body = raw.strip_suffix('/').unwrap_or(raw);
        let anchored = body.contains('/');
        let dir_only = raw.ends_with('/');

        let segments: Vec<Segment> = body
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(Segment::new)
            .collect();

        if segments.is_empty() {
            return Err(PatternError::Empty);
        }

        let trailing_literal = matches!(segments.last(), Some(Segment::Literal(_)));

        Ok(Self {
            raw: raw.to_owned(),
            segments,
            anchored,
            dir_only,
            trailing_literal,
        })
    }

    /// The original pattern text, exactly as written in the file.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Test a repository-relative file path against this pattern.
    ///
    /// A leading `/` on `path` is ignored. Matching is case sensitive, because
    /// GitHub evaluates CODEOWNERS on a case sensitive filesystem.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return false;
        }
        if self.anchored {
            self.matches_at(0, &parts, 0)
        } else {
            (0..parts.len()).any(|start| self.matches_at(0, &parts, start))
        }
    }

    fn matches_at(&self, pi: usize, parts: &[&str], si: usize) -> bool {
        if pi == self.segments.len() {
            return self.terminal_ok(si, parts.len());
        }
        match &self.segments[pi] {
            // `**` consumes any number of segments, including none.
            Segment::AnyDepth => (si..=parts.len()).any(|k| self.matches_at(pi + 1, parts, k)),
            seg => {
                si < parts.len() && seg.matches(parts[si]) && self.matches_at(pi + 1, parts, si + 1)
            }
        }
    }

    /// Decide whether a successful segment match should count, given how much
    /// of the path is left over.
    ///
    /// This is the subtle part. `si == len` means the pattern consumed the
    /// whole path. `si < len` means the pattern named a *directory* and the
    /// path lies inside it — which counts only when the pattern is either
    /// explicitly a directory (`docs/`) or ends in a literal (`**/logs`).
    /// It must NOT count when the final segment is a wildcard, because GitHub
    /// documents `docs/*` as not matching `docs/build-app/troubleshooting.md`.
    fn terminal_ok(&self, si: usize, len: usize) -> bool {
        if self.dir_only {
            si < len
        } else if si == len {
            true
        } else {
            self.trailing_literal
        }
    }
}

/// Match a single path segment against a glob containing `*` and `?`.
///
/// Neither wildcard ever crosses a `/`, because this only ever sees one
/// segment. Iterative with backtracking, so no recursion blowup on inputs
/// like `*a*a*a*a*`.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();

    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut backtrack) = (None, 0usize);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            backtrack = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            backtrack += 1;
            ti = backtrack;
        } else {
            return false;
        }
    }

    p[pi..].iter().all(|&c| c == '*')
}
