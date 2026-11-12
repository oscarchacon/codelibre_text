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

## 3. Scopes of up to 16 atoms

`Scope` (`src/parsing/scope.rs`) packed at most 8 atoms (dot-separated parts) in two `u64`, `a` and `b`; a longer
scope was refused with `TooManyAtoms` (the error meant for a full table of atom names), and the whole syntax with it.
Sublime's own ActionScript has 9-part scopes
(`keyword.operator.assignment.augmented.bitwise.shift.left.signed.actionscript`). `Scope` is now `atoms: [u64; 4]`
(`MAX_ATOMS` = 16, 32 bytes): `pack_as_u16s`, `atom_at`, `missing_atoms`, `len`, `is_prefix_of` (one mask per word)
and `ScopeRepository::to_string` work on it, and a scope longer than 16 atoms is `TooLong`. Scopes are serialized as
strings, so syntect's dumped syntaxes and themes load unchanged. `ATOM_LEN_BITS` (3) is kept: a selector element of
more than 7 atoms scores a little higher than its neighbours' slots allow, which only decides between rules that are
otherwise tied.

Tests: `global_repo_works` and `prefixes_work` in the same file (run them from a copy with an empty `[workspace]`
added: the `testdata` the other modules include is not vendored), and
`a_syntax_with_scopes_of_more_than_eight_parts_loads_and_highlights` in `crates/frameworks/tests/syntax_engine.rs`.
