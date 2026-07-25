use codeowner::{CodeOwners, ErrorKind, Owner, Pattern, PatternError, MAX_FILE_SIZE, SEARCH_PATHS};

// ---------- owner parsing ----------

#[test]
fn owner_shapes() {
    assert_eq!(Owner::parse("@alice"), Some(Owner::user("alice")));
    assert_eq!(Owner::parse("@org/team"), Some(Owner::team("org", "team")));
    assert_eq!(
        Owner::parse("a.b+tag@example.co.uk"),
        Some(Owner::email("a.b+tag@example.co.uk"))
    );
}

#[test]
fn owner_rejects_junk() {
    for junk in [
        "@", "@/team", "@org/", "@a/b/c", "plain", "", "@org//t", "a@b",
    ] {
        assert_eq!(Owner::parse(junk), None, "should reject {junk:?}");
    }
}

#[test]
fn owner_round_trips_through_display() {
    for token in ["@alice", "@org/team", "docs@example.com"] {
        let owner = Owner::parse(token).unwrap();
        assert_eq!(alloc_string(&owner), token);
    }
}

fn alloc_string(o: &Owner) -> String {
    o.to_string()
}

// ---------- glob semantics ----------

#[test]
fn star_never_crosses_a_slash() {
    let p = Pattern::new("src/*.rs").unwrap();
    assert!(p.matches("src/main.rs"));
    assert!(!p.matches("src/parser/main.rs"));
}

#[test]
fn question_mark_matches_one_character() {
    let p = Pattern::new("v?.txt").unwrap();
    assert!(p.matches("v1.txt"));
    assert!(!p.matches("v10.txt"));
    assert!(!p.matches("v.txt"));
}

#[test]
fn pathological_globs_do_not_blow_up() {
    let p = Pattern::new("*a*a*a*a*a*a*b").unwrap();
    assert!(!p.matches(&"a".repeat(64)));
}

#[test]
fn double_star_can_match_zero_segments() {
    let p = Pattern::new("/src/**/mod.rs").unwrap();
    assert!(p.matches("src/mod.rs"));
    assert!(p.matches("src/a/mod.rs"));
    assert!(p.matches("src/a/b/c/mod.rs"));
    assert!(!p.matches("other/mod.rs"));
}

#[test]
fn trailing_double_star_covers_everything_below() {
    let p = Pattern::new("/docs/**").unwrap();
    assert!(p.matches("docs/a.md"));
    assert!(p.matches("docs/a/b/c.md"));
}

#[test]
fn unanchored_bare_name_matches_at_any_depth() {
    let p = Pattern::new("Makefile").unwrap();
    assert!(p.matches("Makefile"));
    assert!(p.matches("src/Makefile"));
    assert!(p.matches("a/b/Makefile"));
    assert!(!p.matches("Makefile.in"));
}

#[test]
fn pattern_keeps_its_original_text() {
    assert_eq!(Pattern::new("/docs/").unwrap().as_str(), "/docs/");
}

#[test]
fn pattern_errors() {
    assert_eq!(Pattern::new(""), Err(PatternError::Empty));
    assert_eq!(Pattern::new("/"), Err(PatternError::Empty));
    assert_eq!(Pattern::new("!x"), Err(PatternError::Negation));
    assert_eq!(Pattern::new("a[0-9]"), Err(PatternError::CharacterRange));
}

// ---------- file-level API ----------

#[test]
fn leading_slash_on_query_path_is_ignored() {
    let co = CodeOwners::parse("/src/ @alice\n");
    assert_eq!(co.of("/src/main.rs"), co.of("src/main.rs"));
    assert!(co.of("/src/main.rs").is_some());
}

#[test]
fn all_matching_returns_file_order() {
    let co = CodeOwners::parse("* @a\n*.rs @b\nsrc/ @c\n");
    let lines: Vec<usize> = co
        .all_matching("src/main.rs")
        .iter()
        .map(|r| r.line)
        .collect();
    assert_eq!(lines, [1, 2, 3]);
    // but only the last one decides
    assert_eq!(co.rule_for("src/main.rs").unwrap().line, 3);
}

#[test]
fn shadowed_rules_are_detected() {
    let co = CodeOwners::parse("/src/ @alice\n* @everyone\n/src/ @bob\n");
    let shadowed: Vec<usize> = co.shadowed().iter().map(|r| r.line).collect();
    assert_eq!(shadowed, [1], "line 1 is fully overridden by line 3");
}

#[test]
fn no_shadowing_in_a_clean_file() {
    let co = CodeOwners::parse("* @everyone\n/src/ @alice\n/docs/ @bob\n");
    assert!(co.shadowed().is_empty());
}

#[test]
fn errors_carry_line_numbers_and_text() {
    let co = CodeOwners::parse("# comment\n\n*.rs @alice\n*.md @bad!owner\n");
    assert_eq!(co.errors().len(), 1);
    let e = &co.errors()[0];
    assert_eq!(e.line, 4);
    assert_eq!(e.text, "*.md @bad!owner");
    assert!(matches!(e.kind, ErrorKind::BadOwner(_)));
    assert_eq!(e.to_string(), "line 4: `@bad!owner` is not a valid owner");
}

#[test]
fn blank_and_comment_only_files_are_empty_not_errors() {
    let co = CodeOwners::parse("\n\n# just a comment\n   \n");
    assert!(co.is_empty());
    assert!(co.errors().is_empty());
    assert_eq!(co.of("anything.rs"), None);
}

#[test]
fn crlf_line_endings_are_handled() {
    let co = CodeOwners::parse("*.rs @alice\r\n*.md @bob\r\n");
    assert_eq!(co.rules().len(), 2);
    assert_eq!(co.of("main.rs"), Some(&[Owner::user("alice")][..]));
}

#[test]
fn empty_input_is_valid() {
    let co = CodeOwners::parse("");
    assert!(co.is_empty());
    assert!(co.errors().is_empty());
}

#[test]
fn constants_match_the_spec() {
    assert_eq!(SEARCH_PATHS[0], ".github/CODEOWNERS");
    assert_eq!(SEARCH_PATHS.len(), 3);
    assert_eq!(MAX_FILE_SIZE, 3 * 1024 * 1024);
}

#[test]
fn a_realistic_monorepo_file() {
    let co = CodeOwners::parse(
        "\
# Default
*                          @org/platform

# Services
/services/payments/        @org/payments
/services/auth/            @org/security

# Anything security-sensitive, anywhere
**/*.pem                   @org/security
/.github/                  @org/platform-admins

# Vendored code has no owner
/vendor/
",
    );

    assert!(co.errors().is_empty());
    assert_eq!(
        co.of("services/payments/api/handler.rs"),
        Some(&[Owner::team("org", "payments")][..])
    );
    assert_eq!(
        co.of("services/payments/certs/key.pem"),
        Some(&[Owner::team("org", "security")][..]),
        "later rule must win over the more specific earlier one"
    );
    assert_eq!(
        co.of("README.md"),
        Some(&[Owner::team("org", "platform")][..])
    );
    assert_eq!(
        co.of("vendor/lib/thing.c").map(<[_]>::len),
        Some(0),
        "vendored code is matched but deliberately unowned"
    );
}
