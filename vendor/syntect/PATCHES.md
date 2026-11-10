# Patches to syntect

This is syntect 5.3.0 (MIT, see `LICENSE.txt`) with the changes listed here. The application depends on it through
`[patch.crates-io]` in the workspace `Cargo.toml`. When syntect is upgraded, copy the new release here and apply these
again (search the sources for `codelibre_text patch`).

## 1. Empty backreferences under a quantifier

`substitute_backrefs_in_regex` (`src/parsing/syntax_definition.rs`) pastes the text a capture matched into a pattern
that refers to it (`\1`, `\2`…). When the capture is empty, or the group did not take part in the match, `(?:\2)*`
becomes `(?:)*`, which the regex engine rejects ("Target of repeat operator is invalid"). syntect then panics
("regex string should be pre-tested") while highlighting, which closes the editor. MarkdownEditing's
`fenced_code_block_end` (`(?:(?:\2)*|(?:\3)*)`) does this for some fences.

The fix substitutes an empty or missing capture as `(?:|)`: the same empty match, and repeatable.

Test: `a_backreference_to_an_empty_capture_can_be_repeated` in `crates/frameworks/tests/syntax_engine.rs`.

## 2. A lint of newer compilers

`Parser::new` in `src/parsing/yaml_load.rs` hid its elided lifetime (`-> Parser`), which current rustc warns about on
every build. It now says `Parser<'_>`. Nothing else changed.
