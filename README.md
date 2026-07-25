# codeowner

[![crates.io](https://img.shields.io/crates/v/codeowner.svg)](https://crates.io/crates/codeowner)
[![docs.rs](https://docs.rs/codeowner/badge.svg)](https://docs.rs/codeowner)

Parse GitHub `CODEOWNERS` files and answer one question correctly: **who owns
this file?**

Zero dependencies. `no_std`. Every example in GitHub's official documentation
is pinned as a test.

```rust
use codeowner::{CodeOwners, Owner};

let owners = CodeOwners::parse("\
*                @org/platform
/services/auth/  @org/security
**/*.pem         @org/security
/vendor/
");

assert_eq!(owners.of("services/auth/login.rs"), Some(&[Owner::team("org", "security")][..]));
assert_eq!(owners.of("README.md"),              Some(&[Owner::team("org", "platform")][..]));
assert_eq!(owners.of("vendor/lib.c").map(<[_]>::len), Some(0)); // matched, deliberately unowned
```

## Why another one

`CODEOWNERS` patterns look like `.gitignore` patterns, and GitHub's own docs
say they follow "most of the same rules." The word doing the work is *most*.
Implementations that reach for a gitignore matcher get the following wrong.

### 1. Unowned is not the same as unmatched

A rule with no owners **clears** ownership. GitHub documents this explicitly.
It is a third state, and collapsing it into "no match" silently reassigns files
to whichever broader rule came before.

```rust
use codeowner::CodeOwners;

let owners = CodeOwners::parse("/apps/ @octocat\n/apps/github\n");

assert!(owners.of("apps/main/index.js").is_some());                  // owned
assert_eq!(owners.of("apps/github/index.js").map(<[_]>::len), Some(0)); // matched, no owner
assert_eq!(owners.of("README.md"), None);                            // never matched
```

`of()` returns `Option<&[Owner]>`, so all three states are distinguishable.

### 2. `docs/*` does not match nested files

Under gitignore rules it would, because `docs/*` matches the intermediate
directory `docs/build-app` and everything beneath it comes along. GitHub says
it does not.

```rust
# use codeowner::CodeOwners;
let owners = CodeOwners::parse("docs/* docs@example.com\n");
assert!(owners.of("docs/getting-started.md").is_some());
assert!(owners.of("docs/build-app/troubleshooting.md").is_none());
```

### 3. Three gitignore features are documented as broken here

- `!` negation
- `[ ]` character ranges
- `\` escaping a leading `#`

Lines using the first two are invalid. This crate reports them with line
numbers instead of mis-parsing them.

## Errors are data

GitHub skips invalid lines rather than rejecting the file, so `parse` never
fails. Bad lines go to `errors()`, which is what you want when writing a linter
or a CI check.

```rust
use codeowner::CodeOwners;

let owners = CodeOwners::parse("*.rs @alice\n*.md @bad!owner\n");
assert_eq!(owners.rules().len(), 1);
assert_eq!(owners.errors()[0].to_string(), "line 2: `@bad!owner` is not a valid owner");
```

## API

| Method | Purpose |
|---|---|
| `CodeOwners::parse(&str)` | Never fails. Collects errors. |
| `.of(path)` | `Option<&[Owner]>` — the three states above |
| `.rule_for(path)` | The winning rule, with its line number |
| `.all_matching(path)` | Every matching rule, in file order |
| `.shadowed()` | Rules that can never take effect |
| `.errors()` | Skipped lines with line numbers |
| `Pattern::new(&str)` | Use the matcher on its own |
| `Owner::parse(&str)` | `@user`, `@org/team`, or an email |

`shadowed()` catches the most common CODEOWNERS mistake: putting a specific
rule *above* the catch-all instead of below it. Last matching rule wins, so the
specific rule silently never fires.

## Install

```toml
[dependencies]
codeowner = "0.1"
```

## Open questions

Calls made here that are not settled by GitHub's documentation. Evidence beats
opinion — open an issue with a real-world file if you disagree.

1. **Username validation.** Logins are restricted to alphanumerics and hyphens
   (≤39 chars); team slugs additionally allow `_` and `.`. GitHub does not
   publish the exact grammar, so this may be too strict for some orgs.
2. **Email validation** is deliberately loose — local part non-empty, domain
   contains a dot. Tightening it would reject valid exotic addresses.
3. **A leftover `\` after comment stripping.** Since `\#foo` is a comment, the
   line reduces to `\`. This crate treats that as a literal pattern rather than
   an error. GitHub's behaviour here is undocumented.
4. **Non-UTF-8 paths** are out of scope; the API is `&str` throughout.

## Not in scope

- GitLab section headers (`[Backend][2]`) — planned for 0.2
- Filesystem walking, git integration, or GitHub API calls
- Resolving whether a user or team actually exists

## License

MIT OR Apache-2.0, at your option.
