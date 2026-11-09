#!/usr/bin/env bash
# The section of CHANGELOG.md for one version (e.g. 1.0.0): from its heading to the next one.
set -euo pipefail
version=${1:?version, e.g. 1.0.0}
awk -v v="$version" '
    /^## / { if (found) exit; found = index($0, "## " v) == 1 }
    found { print }
' "$(dirname "$0")/../CHANGELOG.md"
