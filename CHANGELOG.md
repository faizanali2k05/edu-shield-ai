# Changelog

## 0.1.0

Initial release.

- `CodeOwners::parse` — never fails, collects per-line errors
- `of()` distinguishes owned / explicitly-unowned / unmatched
- Correct CODEOWNERS pattern semantics, not gitignore semantics
- `shadowed()` lint for rules that can never take effect
- Every example from GitHub's docs pinned as a test
- `no_std`, zero dependencies
