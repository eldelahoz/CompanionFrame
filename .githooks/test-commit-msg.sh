#!/bin/sh
# Self-check for .githooks/commit-msg. Usage: sh .githooks/test-commit-msg.sh
dir=$(cd "$(dirname "$0")" && pwd)
hook="$dir/commit-msg"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fails=0
n=0

# pad_to N: a single-line ASCII message of exactly N characters.
pad_to() {
    printf 'feat: %s' "$(head -c $(($1 - 6)) /dev/zero | tr '\0' a)"
}

# check EXPECTED_EXIT LABEL MESSAGE_FILE
check() {
    n=$((n + 1))
    sh "$hook" "$3" 2>/dev/null
    rc=$?
    if [ "$rc" -eq "$1" ]; then
        echo "ok   $2"
    else
        echo "FAIL $2 (expected exit $1, got $rc)"
        fails=$((fails + 1))
    fi
}

msg() { printf '%s' "$2" > "$tmp/$1"; echo "$tmp/$1"; }

check 0 "scoped feat" "$(msg a 'feat(overlay): add click-through window')"
check 0 "fix without scope" "$(msg b 'fix: ignore key auto-repeat')"
check 0 "breaking change with !" "$(msg c 'refactor(wfm)!: split order validators')"
check 0 "uppercase and period not enforced" "$(msg d 'fix(hotkey): Ignore Key Auto-Repeat.')"
check 0 "exactly 140 chars" "$(pad_to 140 > "$tmp/e"; echo "$tmp/e")"
check 0 "merge message over 140 passes unchanged" "$(msg f "Merge branch 'x' into main $(pad_to 150)")"
check 0 "revert passes unchanged" "$(msg g 'Revert "feat: something that was wrong"')"
check 0 "fixup passes unchanged" "$(msg h 'fixup! feat: something')"
check 0 "comment lines ignored" "$(msg i "$(printf 'docs: explain hook\n# Please enter the commit message\n\n\n')")"
check 1 "141 chars rejected" "$(pad_to 141 > "$tmp/j"; echo "$tmp/j")"
check 1 "bad type rejected" "$(msg k 'wip: stuff')"
check 1 "empty scope rejected" "$(msg l 'feat(): x')"
check 1 "missing space after colon rejected" "$(msg m 'feat:x')"
check 1 "total length over 140 across body rejected" "$(msg n "$(printf 'feat: short title\n\n%s' "$(pad_to 140)")")"
check 1 "empty message rejected" "$(msg o '')"

echo
echo "$((n - fails))/$n passed"
[ "$fails" -eq 0 ]
