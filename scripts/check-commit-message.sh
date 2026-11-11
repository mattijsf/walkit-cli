#!/usr/bin/env bash
# Checks that a commit message's first line follows Conventional Commits:
#   type(optional scope)!: description
# Usage: check-commit-message.sh <file with the message>
#        check-commit-message.sh --subject "<first line>"
set -euo pipefail

types='feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert'

if [ "${1:-}" = "--subject" ]; then
    subject=${2:-}
else
    subject=$(grep -v '^#' "$1" | head -n 1)
fi

# commits git writes itself
case "$subject" in
    "Merge "* | "Revert \""* | "fixup! "* | "squash! "*) exit 0 ;;
esac

if ! printf '%s\n' "$subject" | grep -Eq "^($types)(\([a-z0-9._/-]+\))?!?: [^ ].*[^.]$"; then
    {
        echo "Commit message does not follow Conventional Commits:"
        echo "  $subject"
        echo
        echo "Expected: <type>(<scope>): <description>"
        echo "  types: ${types//|/, }"
        echo "  example: fix(render): keep the window's own clip on unclip"
        echo "  no full stop at the end"
    } >&2
    exit 1
fi

if [ "${#subject}" -gt 100 ]; then
    echo "Commit subject is ${#subject} characters long; keep it to 100 or fewer." >&2
    exit 1
fi
