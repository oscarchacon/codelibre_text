#!/usr/bin/env bash
# The versions of the releases, as the branches release/<version> and the tags name them:
#   X.Y.Z                      0.2.0
#   X.Y.Z-<pre>                0.2.0-alpha, 0.2.0-alpha.1, 0.2.0-rc-2
#   X.Y.Z.<more>[-<pre>]       0.1.3.1-alpha, 0.1.3.1.hotfix-beta.2 (more parts after X.Y.Z)
#
#   version.sh check <version>   fails, with an error for GitHub, when it is not one
#   version.sh cargo <version>   the version Cargo.toml takes (SemVer, three numbers only): the parts
#                                after X.Y.Z become build metadata, which SemVer allows anything in
#                                (0.1.3.1-alpha is 0.1.3-alpha+1, 0.1.3.1 is 0.1.3+1)
set -euo pipefail

command="${1:?usage: version.sh check|cargo <version>}"
version="${2:?usage: version.sh check|cargo <version>}"

number='(0|[1-9][0-9]*)'
# A pre-release identifier: a number without leading zeros, or letters, digits and hyphens.
identifier='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
pattern="^$number\.$number\.$number((\.[0-9A-Za-z]+)*)(-$identifier(\.$identifier)*)?\$"

if ! [[ "$version" =~ $pattern ]]; then
  # On stdout, where GitHub reads its commands; the workflows check before they ask for cargo.
  echo "::error::'$version' is not a version: X.Y.Z, then optionally .W... and -alpha, -beta.1, -rc... (release/0.1.3-alpha, release/0.1.3.1-alpha)"
  exit 1
fi

case "$command" in
  check) ;;
  cargo)
    core="${version%%-*}"
    pre=""
    [[ "$version" == *-* ]] && pre="-${version#*-}"
    IFS=. read -r major minor patch more <<< "$core"
    cargo="$major.$minor.$patch$pre"
    [ -n "${more:-}" ] && cargo+="+$more"
    echo "$cargo"
    ;;
  *)
    echo "usage: version.sh check|cargo <version>" >&2
    exit 2
    ;;
esac
