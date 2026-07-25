//! Every example from GitHub's "About code owners" documentation, pinned as a
//! test. If GitHub changes the spec, these fail loudly.
//!
//! Source: docs.github.com/articles/about-code-owners

use codeowner::{CodeOwners, ErrorKind, Owner, PatternError};

/// The full example file from the docs, verbatim.
const DOC_EXAMPLE: &str = r"
# This is a comment.
*       @global-owner1 @global-owner2
*.js    @js-owner #This is an inline comment.
*.go docs@example.com
*.txt @octo-org/octocats
/build/logs/ @doctocat
docs/* docs@example.com
apps/ @octocat
/docs/ @doctocat
/scripts/ @doctocat @octocat
**/logs @octocat
";

fn owners_of(file: &str, path: &str) -> Option<alloc_vec::Names> {
    let co = CodeOwners::parse(file);
    co.of(path)
        .map(|os| os.iter().map(alloc::string::ToString::to_string).collect())
}

// Tiny aliases so the assertions below read cleanly.
extern crate alloc;
mod alloc_vec {
    pub type Names = alloc::vec::Vec<alloc::string::String>;
}

#[test]
fn star_is_the_default_owner_for_everything() {
    assert_eq!(
        owners_of(DOC_EXAMPLE, "some/deeply/nested/file.rb").unwrap(),
        ["@global-owner1", "@global-owner2"]
    );
}

#[test]
fn inline_comments_are_stripped() {
    let co = CodeOwners::parse("*.js @js-owner #This is an inline comment.\n");
    assert_eq!(co.errors(), &[]);
    assert_eq!(co.of("app.js"), Some(&[Owner::user("js-owner")][..]));
}

#[test]
fn email_owners_are_supported() {
    let co = CodeOwners::parse("*.go docs@example.com\n");
    assert_eq!(
        co.of("main.go"),
        Some(&[Owner::email("docs@example.com")][..])
    );
}

#[test]
fn team_owners_are_supported() {
    let co = CodeOwners::parse("*.txt @octo-org/octocats\n");
    assert_eq!(
        co.of("notes.txt"),
        Some(&[Owner::team("octo-org", "octocats")][..])
    );
}

/// "@doctocat owns any files in the /build/logs directory at the root of the
/// repository and any of its subdirectories."
#[test]
fn anchored_directory_covers_subdirectories() {
    let co = CodeOwners::parse("/build/logs/ @doctocat\n");
    assert!(co.of("build/logs/error.log").is_some());
    assert!(co.of("build/logs/deep/nested/error.log").is_some());
    // Anchored: only at the root.
    assert!(co.of("src/build/logs/error.log").is_none());
}

/// "The `docs/*` pattern will match files like `docs/getting-started.md` but
/// not further nested files like `docs/build-app/troubleshooting.md`."
///
/// This is the headline divergence from gitignore semantics.
#[test]
fn single_star_does_not_cross_directories() {
    let co = CodeOwners::parse("docs/* docs@example.com\n");
    assert!(co.of("docs/getting-started.md").is_some());
    assert!(co.of("docs/build-app/troubleshooting.md").is_none());
}

/// "@octocat owns any file in an apps directory anywhere in your repository."
#[test]
fn unanchored_directory_matches_at_any_depth() {
    let co = CodeOwners::parse("apps/ @octocat\n");
    assert!(co.of("apps/index.js").is_some());
    assert!(co.of("src/apps/index.js").is_some());
    assert!(co.of("a/b/c/apps/deep/index.js").is_some());
    // It is a directory pattern, so a *file* called `apps` does not match.
    assert!(co.of("apps").is_none());
}

/// "@doctocat owns any file in the `/docs` directory in the root of your
/// repository and any of its subdirectories."
#[test]
fn leading_slash_anchors_to_root() {
    let co = CodeOwners::parse("/docs/ @doctocat\n");
    assert!(co.of("docs/index.md").is_some());
    assert!(co.of("docs/a/b/index.md").is_some());
    assert!(co.of("src/docs/index.md").is_none());
}

#[test]
fn multiple_owners_on_one_line() {
    let co = CodeOwners::parse("/scripts/ @doctocat @octocat\n");
    assert_eq!(
        co.of("scripts/deploy.sh"),
        Some(&[Owner::user("doctocat"), Owner::user("octocat")][..])
    );
}

/// "@octocat owns any file in a `/logs` directory such as `/build/logs`,
/// `/scripts/logs`, and `/deeply/nested/logs`."
#[test]
fn double_star_matches_any_depth() {
    let co = CodeOwners::parse("**/logs @octocat\n");
    assert!(co.of("build/logs/error.log").is_some());
    assert!(co.of("scripts/logs/error.log").is_some());
    assert!(co.of("deeply/nested/logs/error.log").is_some());
    assert!(co.of("logs/error.log").is_some());
    assert!(co.of("build/other/error.log").is_none());
}

/// "@octocat owns any file in the `/apps` directory ... except for the
/// `/apps/github` subdirectory, as its owners are left empty."
///
/// The distinction between "matched, no owners" and "no rule matched" is the
/// single most important thing this crate gets right.
#[test]
fn empty_owners_clears_ownership() {
    let co = CodeOwners::parse("/apps/ @octocat\n/apps/github\n");

    assert_eq!(
        co.of("apps/main/index.js"),
        Some(&[Owner::user("octocat")][..])
    );

    let cleared = co.of("apps/github/index.js").expect("rule should match");
    assert!(cleared.is_empty(), "ownership must be explicitly cleared");
    assert!(co.rule_for("apps/github/index.js").unwrap().is_unowned());

    assert_eq!(co.of("README.md"), None, "no rule at all is a third state");
}

/// The variant where the subdirectory has its own owner instead.
#[test]
fn later_rule_overrides_earlier() {
    let co = CodeOwners::parse("/apps/ @octocat\n/apps/github @doctocat\n");
    assert_eq!(
        co.of("apps/main/index.js"),
        Some(&[Owner::user("octocat")][..])
    );
    assert_eq!(
        co.of("apps/github/index.js"),
        Some(&[Owner::user("doctocat")][..])
    );
}

#[test]
fn last_matching_rule_wins_not_the_most_specific() {
    // Deliberately "wrong" ordering: the catch-all is last, so it wins.
    let co = CodeOwners::parse("/src/parser/ @alice\n* @everyone\n");
    assert_eq!(
        co.of("src/parser/lexer.rs"),
        Some(&[Owner::user("everyone")][..])
    );
}

// ---------- documented gitignore features that must NOT work ----------

#[test]
fn negation_is_rejected() {
    let co = CodeOwners::parse("!*.js @alice\n");
    assert!(co.rules().is_empty());
    assert_eq!(
        co.errors()[0].kind,
        ErrorKind::BadPattern(PatternError::Negation)
    );
}

#[test]
fn character_ranges_are_rejected() {
    let co = CodeOwners::parse("*.[jt]s @alice\n");
    assert!(co.rules().is_empty());
    assert_eq!(
        co.errors()[0].kind,
        ErrorKind::BadPattern(PatternError::CharacterRange)
    );
}

/// GitHub: "Escaping a pattern starting with `#` using `\` so it is treated as
/// a pattern and not a comment doesn't work."
///
/// So the `#` still opens a comment. What GitHub does with the leftover `\` is
/// not documented; see the README's open questions. The documented, testable
/// fact is that no rule for `#config` is produced.
#[test]
fn backslash_does_not_escape_a_leading_hash() {
    let co = CodeOwners::parse(r"\#config @alice");
    assert!(
        co.rules().iter().all(|r| r.pattern.as_str() != "#config"),
        "the escape must not resurrect `#config` as a pattern"
    );
    assert_eq!(co.of("config"), None);
    // The owner is swallowed by the comment too, so nothing is assigned.
    assert!(co.rules().iter().all(|r| r.owners.is_empty()));
}

#[test]
fn invalid_lines_are_skipped_not_fatal() {
    let co = CodeOwners::parse("*.rs @alice\n!bad @bob\n*.md @carol\n");
    assert_eq!(co.rules().len(), 2);
    assert_eq!(co.errors().len(), 1);
    assert_eq!(co.errors()[0].line, 2);
    assert_eq!(co.of("main.rs"), Some(&[Owner::user("alice")][..]));
    assert_eq!(co.of("README.md"), Some(&[Owner::user("carol")][..]));
}

#[test]
fn paths_are_case_sensitive() {
    let co = CodeOwners::parse("/Docs/ @alice\n");
    assert!(co.of("Docs/index.md").is_some());
    assert!(co.of("docs/index.md").is_none());
}
