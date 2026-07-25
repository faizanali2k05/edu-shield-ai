//! Parse GitHub `CODEOWNERS` files and answer the only question that matters:
//! **who owns this file?**
//!
//! ```
//! use codeowner::{CodeOwners, Owner};
//!
//! let owners = CodeOwners::parse("\
//! *                @org/everyone
//! /src/parser/     @alice
//! *.md             docs@example.com
//! ");
//!
//! assert_eq!(owners.of("src/parser/lexer.rs"), Some(&[Owner::user("alice")][..]));
//! assert_eq!(owners.of("README.md"),           Some(&[Owner::email("docs@example.com")][..]));
//! assert_eq!(owners.of("build.sh"),            Some(&[Owner::team("org", "everyone")][..]));
//! ```
//!
//! # Three things implementations usually get wrong
//!
//! **1. Unowned is not the same as unmatched.** A rule with no owners
//! deliberately *clears* ownership. GitHub documents this. Collapsing the two
//! cases into one silently reassigns files to the wrong team.
//!
//! ```
//! use codeowner::CodeOwners;
//!
//! let owners = CodeOwners::parse("/apps/ @octocat\n/apps/github\n");
//!
//! assert!(owners.of("apps/main/index.js").is_some()); // owned by @octocat
//! assert_eq!(owners.of("apps/github/index.js").map(<[_]>::len), Some(0)); // matched, no owner
//! assert_eq!(owners.of("README.md"), None);           // no rule matched at all
//! ```
//!
//! **2. CODEOWNERS is not gitignore.** GitHub documents three gitignore
//! features as non-functional here: `!` negation, `[ ]` character ranges, and
//! `\` escaping of a leading `#`. Lines using them are invalid and skipped —
//! this crate reports them rather than silently mis-parsing.
//!
//! **3. `docs/*` does not match nested files.** Under gitignore rules it would,
//! by matching the intermediate directory. GitHub says it does not.
//!
//! ```
//! use codeowner::CodeOwners;
//!
//! let owners = CodeOwners::parse("docs/* docs@example.com\n");
//! assert!(owners.of("docs/getting-started.md").is_some());
//! assert!(owners.of("docs/build-app/troubleshooting.md").is_none());
//! ```
//!
//! # Errors are data, not failures
//!
//! GitHub skips invalid lines rather than rejecting the file, so
//! [`CodeOwners::parse`] never fails. Bad lines land in
//! [`errors`](CodeOwners::errors) with line numbers, which is what you want if
//! you are writing a linter.
//!
//! ```
//! use codeowner::CodeOwners;
//!
//! let owners = CodeOwners::parse("*.rs @alice\n![!]bad @bob\n");
//! assert_eq!(owners.rules().len(), 1);
//! assert_eq!(owners.errors().len(), 1);
//! assert_eq!(owners.errors()[0].line, 2);
//! ```
//!
//! # Scope
//!
//! GitHub CODEOWNERS syntax, zero dependencies, `no_std` (needs `alloc`).
//! GitLab's section headers (`[Backend][2]`) are not supported yet.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

mod pattern;

pub use pattern::{Pattern, PatternError};

/// The paths GitHub searches, in priority order.
///
/// The first file that exists wins; the others are ignored entirely.
pub const SEARCH_PATHS: [&str; 3] = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

/// GitHub refuses to load a CODEOWNERS file above this size.
pub const MAX_FILE_SIZE: usize = 3 * 1024 * 1024;

/// A single code owner.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Owner {
    /// `@username`
    User(String),
    /// `@org/team-name`
    Team {
        /// The organization, without the leading `@`.
        org: String,
        /// The team slug.
        team: String,
    },
    /// A bare email address.
    Email(String),
}

impl Owner {
    /// Construct a [`Owner::User`].
    #[must_use]
    pub fn user(name: &str) -> Self {
        Self::User(name.to_owned())
    }

    /// Construct a [`Owner::Team`].
    #[must_use]
    pub fn team(org: &str, team: &str) -> Self {
        Self::Team {
            org: org.to_owned(),
            team: team.to_owned(),
        }
    }

    /// Construct a [`Owner::Email`].
    #[must_use]
    pub fn email(addr: &str) -> Self {
        Self::Email(addr.to_owned())
    }

    /// Parse one owner token.
    ///
    /// ```
    /// use codeowner::Owner;
    ///
    /// assert_eq!(Owner::parse("@alice"),     Some(Owner::user("alice")));
    /// assert_eq!(Owner::parse("@org/team"),  Some(Owner::team("org", "team")));
    /// assert_eq!(Owner::parse("a@b.com"),    Some(Owner::email("a@b.com")));
    /// assert_eq!(Owner::parse("nonsense"),   None);
    /// ```
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        if let Some(rest) = token.strip_prefix('@') {
            if rest.is_empty() {
                return None;
            }
            return match rest.split_once('/') {
                Some((org, team)) => {
                    if is_login(org) && is_slug(team) {
                        Some(Self::team(org, team))
                    } else {
                        None
                    }
                }
                None if is_login(rest) => Some(Self::User(rest.to_owned())),
                None => None,
            };
        }
        // Deliberately permissive: GitHub resolves the address against account
        // emails, so rejecting exotic-but-valid addresses would be worse than
        // accepting a few junk ones.
        let (local, domain) = token.split_once('@')?;
        if local.is_empty() || !domain.contains('.') || domain.starts_with('.') {
            return None;
        }
        Some(Self::Email(token.to_owned()))
    }
}

/// A GitHub account name: alphanumeric and hyphens, no leading or trailing
/// hyphen, at most 39 characters.
fn is_login(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 39
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// A team slug. Slightly looser than a login: underscores and dots occur in
/// real team slugs.
fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User(u) => write!(f, "@{u}"),
            Self::Team { org, team } => write!(f, "@{org}/{team}"),
            Self::Email(e) => f.write_str(e),
        }
    }
}

/// One `pattern owners...` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The compiled path pattern.
    pub pattern: Pattern,
    /// Owners, in the order written. Empty means ownership is explicitly cleared.
    pub owners: Vec<Owner>,
    /// 1-based line number in the source file.
    pub line: usize,
}

impl Rule {
    /// True if this rule deliberately leaves matching paths unowned.
    #[must_use]
    pub fn is_unowned(&self) -> bool {
        self.owners.is_empty()
    }
}

/// What was wrong with a line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The path pattern could not be compiled.
    BadPattern(PatternError),
    /// A token after the pattern was not a recognisable owner.
    BadOwner(String),
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadPattern(e) => write!(f, "{e}"),
            Self::BadOwner(t) => write!(f, "`{t}` is not a valid owner"),
        }
    }
}

/// A skipped line, with enough context to point at it in an editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number.
    pub line: usize,
    /// What went wrong.
    pub kind: ErrorKind,
    /// The offending line, trimmed.
    pub text: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.kind)
    }
}

/// A parsed CODEOWNERS file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeOwners {
    rules: Vec<Rule>,
    errors: Vec<ParseError>,
}

impl CodeOwners {
    /// Parse a CODEOWNERS file. Never fails; invalid lines are collected in
    /// [`errors`](Self::errors), matching GitHub's own behaviour.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut rules = Vec::new();
        let mut errors = Vec::new();

        for (idx, raw_line) in text.lines().enumerate() {
            let line = idx + 1;

            // No escaping: `#` always starts a comment, anywhere on the line.
            let content = raw_line.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }

            let mut tokens = content.split_whitespace();
            let Some(pattern_str) = tokens.next() else {
                continue;
            };

            let pattern = match Pattern::new(pattern_str) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(ParseError {
                        line,
                        kind: ErrorKind::BadPattern(e),
                        text: content.to_owned(),
                    });
                    continue;
                }
            };

            let mut owners = Vec::new();
            let mut bad = None;
            for token in tokens {
                match Owner::parse(token) {
                    Some(o) => owners.push(o),
                    None => {
                        bad = Some(token.to_owned());
                        break;
                    }
                }
            }

            if let Some(token) = bad {
                errors.push(ParseError {
                    line,
                    kind: ErrorKind::BadOwner(token),
                    text: content.to_owned(),
                });
                continue;
            }

            rules.push(Rule {
                pattern,
                owners,
                line,
            });
        }

        Self { rules, errors }
    }

    /// Owners of `path`, or `None` if no rule matched.
    ///
    /// `Some(&[])` means a rule matched and explicitly left the path unowned.
    /// That distinction is the whole point; see the crate docs.
    #[must_use]
    pub fn of(&self, path: &str) -> Option<&[Owner]> {
        self.rule_for(path).map(|r| r.owners.as_slice())
    }

    /// The rule that decides `path` — the **last** matching rule in the file.
    #[must_use]
    pub fn rule_for(&self, path: &str) -> Option<&Rule> {
        self.rules.iter().rev().find(|r| r.pattern.matches(path))
    }

    /// Every rule that matches `path`, in file order.
    ///
    /// Only the last one takes effect, but a linter wants to show the rest.
    #[must_use]
    pub fn all_matching(&self, path: &str) -> Vec<&Rule> {
        self.rules
            .iter()
            .filter(|r| r.pattern.matches(path))
            .collect()
    }

    /// Rules that can never take effect, because a later rule always wins for
    /// everything they match.
    ///
    /// A cheap, useful lint: it catches the classic mistake of putting a
    /// specific rule above the catch-all instead of below it.
    #[must_use]
    pub fn shadowed(&self) -> Vec<&Rule> {
        self.rules
            .iter()
            .enumerate()
            .filter(|(i, rule)| {
                self.rules[i + 1..]
                    .iter()
                    .any(|later| later.pattern.as_str() == rule.pattern.as_str())
            })
            .map(|(_, rule)| rule)
            .collect()
    }

    /// All rules, in file order.
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Lines that were skipped.
    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// True if no rule parsed successfully.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
