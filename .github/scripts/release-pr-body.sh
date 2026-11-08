#!/usr/bin/env bash
# Writes the description of a pull request into release/<version>: what it releases, whether it
# goes to main, the pull requests it brings and every commit, grouped by type, with its details.
#   release-pr-body.sh <version> <base> <head> [current-body-file]
#   (release-pr-body.sh 0.1.3-alpha origin/release/0.1.3-alpha origin/develop body.md)
# Prints the whole new description. What is generated sits between the markers
# <!-- auto:start --> and <!-- auto:end -->: the text written by hand outside them (the current
# description, when given) is kept, so the description can be edited and regenerated.
# Needs GH_REPO (owner/name) and GH_TOKEN, and the history of both refs (fetch-depth: 0).
set -euo pipefail

version="${1:?usage: release-pr-body.sh <version> <base> <head> [current-body-file]}"
base="${2:?missing base}"
head="${3:?missing head}"
current_file="${4:-}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
start_marker="<!-- auto:start -->"
end_marker="<!-- auto:end -->"
# GitHub refuses descriptions longer than 65536 characters.
max_length=60000

case "$version" in
  *-rc*) kind="release candidate" ;;
  *-beta*) kind="beta" ;;
  *-alpha*) kind="alpha" ;;
  *-*) kind="pre-release" ;;
  *) kind="final" ;;
esac

decision="$(bash "$here/goes-to-main.sh" "$version")"
to_main="$(sed -n 's/^to_main=//p' <<< "$decision")"
reason="$(sed -n 's/^reason=//p' <<< "$decision")"
if [ "$to_main" = true ]; then
  destination="**main**, once every installer is built ($reason)"
else
  destination="only \`release/$version\` ($reason)"
fi

# Conventional commit types, in the order they are listed.
types=(feat fix perf refactor docs test ci build chore style)
declare -A titles=(
  [feat]="Features" [fix]="Fixes" [perf]="Performance" [refactor]="Refactoring"
  [docs]="Documentation" [test]="Tests" [ci]="CI" [build]="Build" [chore]="Chores" [style]="Style"
  [other]="Other changes"
)
declare -A sections=()
commit_count=0

# One record per commit (no merges): hash, subject and body, split by unit / record separators.
while IFS=$'\x1f' read -r -d $'\x1e' hash subject body; do
  hash="${hash#$'\n'}"
  [ -n "$hash" ] || continue
  commit_count=$((commit_count + 1))
  type=other
  text="$subject"
  if [[ "$subject" =~ ^([a-z]+)(\([^\)]*\))?(!)?:[[:space:]]*(.*)$ ]] && [ -n "${titles[${BASH_REMATCH[1]}]:-}" ] \
     && [ "${BASH_REMATCH[1]}" != other ]; then
    type="${BASH_REMATCH[1]}"
    scope="${BASH_REMATCH[2]}"
    breaking="${BASH_REMATCH[3]}"
    text="${BASH_REMATCH[4]}"
    [ -n "$scope" ] && text="**${scope:1:-1}:** $text"
    [ -n "$breaking" ] && text="**BREAKING** $text"
  fi
  # Trailers (Co-Authored-By...) say nothing about the change.
  body="$(sed -E '/^[A-Za-z-]+-[Bb]y: /d' <<< "$body" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')"
  entry="- $text (${hash:0:7})"
  if [ -n "$(tr -d '[:space:]' <<< "$body")" ]; then
    entry+=$'\n  <details><summary>Details</summary>\n\n'
    entry+="$(sed 's/^/  /' <<< "$body")"
    entry+=$'\n\n  </details>'
  fi
  sections[$type]+="$entry"$'\n'
done < <(git log --no-merges --reverse --format='%H%x1f%s%x1f%b%x1e' "$base..$head")

# Pull requests merged into the head branch: "Merge pull request #12 ..." or a squash "... (#12)".
pull_requests=""
while read -r number; do
  [ -n "$number" ] || continue
  info="$(gh pr view "$number" --json title,url,author,body \
    --jq '"\(.title)\u001f\(.url)\u001f\(.author.login)\u001f\(.body)"' 2> /dev/null)" || continue
  IFS=$'\x1f' read -r -d '' title url author pr_body <<< "$info" || true
  pr_body="${pr_body%$'\n'}"
  pull_requests+="- [#$number]($url) $title (by \`$author\`)"$'\n'
  if [ -n "$(tr -d '[:space:]' <<< "$pr_body")" ]; then
    pull_requests+=$'  <details><summary>Description</summary>\n\n'
    pull_requests+="$(sed 's/^/  /' <<< "$pr_body")"
    pull_requests+=$'\n\n  </details>\n'
  fi
done < <(git log --format=%s "$base..$head" \
  | sed -nE 's/^Merge pull request #([0-9]+).*/\1/p; s/.*\(#([0-9]+)\)$/\1/p' | awk '!seen[$0]++')

generated="$start_marker
<!-- Generated from the commits: regenerated on every push. Write your own notes outside these markers. -->
## Release $version

| | |
|---|---|
| Version | \`$version\` ($kind) |
| Goes to | $destination |
| Commits | $commit_count |

To take a pre-release to main, push a tag on develop (or on \`release/$version\`), at any time:
\`git tag to-main/$version develop && git push origin to-main/$version\`

Merging this pull request runs the tests on every platform and builds the installers of \`v$version\`.
"
if [ -n "$pull_requests" ]; then
  generated+=$'\n## Pull requests\n\n'"$pull_requests"
fi
generated+=$'\n## Changes\n'
for type in "${types[@]}" other; do
  if [ -n "${sections[$type]:-}" ]; then
    generated+=$'\n### '"${titles[$type]}"$'\n\n'"${sections[$type]}"
  fi
done
[ "$commit_count" = 0 ] && generated+=$'\nNo commits yet.\n'
if [ "${#generated}" -gt "$max_length" ]; then
  generated="${generated:0:$max_length}"$'\n\n_Cut: the full list is in the Commits tab._\n'
fi
generated+="$end_marker"

current=""
[ -n "$current_file" ] && [ -f "$current_file" ] && current="$(cat "$current_file")"
if [[ "$current" == *"$start_marker"*"$end_marker"* ]]; then
  before="${current%%"$start_marker"*}"
  after="${current#*"$end_marker"}"
  printf '%s%s%s\n' "$before" "$generated" "$after"
elif [ -n "$(tr -d '[:space:]' <<< "$current")" ]; then
  printf '%s\n\n%s\n' "$current" "$generated"
else
  printf '## Notes\n\n<!-- Write here what the release is about: it is kept when the rest is regenerated. -->\n\n%s\n' "$generated"
fi
