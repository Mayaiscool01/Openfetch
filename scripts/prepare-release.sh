#!/bin/sh
# Fill in your details before publishing.
# Usage: scripts/prepare-release.sh <github-username> "<name for the license>"
set -eu
[ $# -eq 2 ] || { echo "usage: $0 <github-username> \"<name for the license>\"" >&2; exit 1; }
cd "$(dirname "$0")/.."
user=$1
name=$2
sed -i "s|YOURNAME|$user|g" Cargo.toml README.md packaging/PKGBUILD packaging/openfetch.spec
sed -i "s|<your repo>|https://github.com/$user/openfetch|" README.md
sed -i "s|OpenFetch contributors|$name|" LICENSE
sed -i "s|you <you@example.com>|$name|; s|You <you@example.com>|$name|" packaging/PKGBUILD packaging/openfetch.spec
echo "Done. Review the changes with: git diff"
