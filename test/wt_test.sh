#!/bin/sh

set -eu

WT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)/wt
TMP=${TMPDIR:-/tmp}/wt-tests-$$
passed=0
failed=0

cleanup() {
    rm -rf "$TMP"
}
trap cleanup EXIT HUP INT TERM

new_repo() {
    name=$1
    repo=$TMP/$name
    mkdir -p "$repo"
    git -C "$repo" init -q -b main
    git -C "$repo" config user.name 'WT Tests'
    git -C "$repo" config user.email 'wt-tests@example.invalid'
    printf 'initial\n' >"$repo/README.md"
    git -C "$repo" add README.md
    git -C "$repo" commit -qm 'Initial commit'
    printf '%s\n' "$repo"
}

new_unborn_repo() {
    name=$1
    repo=$TMP/$name
    mkdir -p "$repo"
    git -C "$repo" init -q -b main
    printf '%s\n' "$repo"
}

assert_contains() {
    case $1 in
        *"$2"*) return 0 ;;
        *) printf 'expected output to contain: %s\nactual: %s\n' "$2" "$1"; return 1 ;;
    esac
}

assert_eq() {
    [ "$1" = "$2" ] || {
        printf 'expected: %s\nactual:   %s\n' "$2" "$1"
        return 1
    }
}

run_test() {
    name=$1
    shift
    if ("$@"); then
        passed=$((passed + 1))
        printf 'ok - %s\n' "$name"
    else
        failed=$((failed + 1))
        printf 'not ok - %s\n' "$name"
    fi
}

test_create() {
    repo=$(new_repo create)
    output=$(cd "$repo" && "$WT" create feature)
    assert_eq "$output" "$repo.worktrees/feature"
    [ -d "$output" ]
    assert_eq "$(git -C "$output" branch --show-current)" feature
}

test_create_is_idempotent() {
    repo=$(new_repo idempotent)
    first=$(cd "$repo" && "$WT" create feature 2>/dev/null)
    second=$(cd "$repo" && "$WT" create feature 2>"$TMP/error")
    assert_eq "$second" "$first"
    assert_contains "$(cat "$TMP/error")" 'already exists'
}

test_create_existing_local_branch() {
    repo=$(new_repo existing-branch)
    git -C "$repo" branch feature
    path=$(cd "$repo" && "$WT" create feature)
    assert_eq "$path" "$repo.worktrees/feature"
    assert_eq "$(git -C "$path" branch --show-current)" feature
}

test_create_explains_unborn_branch() {
    repo=$(new_unborn_repo unborn)
    if (cd "$repo" && "$WT" create feature >"$TMP/output" 2>"$TMP/error"); then
        return 1
    fi
    assert_contains "$(cat "$TMP/error")" 'no commits yet'
}

test_create_rejects_dev_branch() {
    repo=$(new_repo dev-branch)
    git -C "$repo" switch -qc develop
    if (cd "$repo" && "$WT" create feature >"$TMP/output" 2>"$TMP/error"); then
        return 1
    fi
    assert_contains "$(cat "$TMP/error")" 'main or staging'
    output=$(cd "$repo" && "$WT" create --allow-dev-branches feature)
    [ -d "$output" ]
}

test_cleanup_clean_worktree() {
    repo=$(new_repo clean)
    path=$(cd "$repo" && "$WT" create feature)
    (cd "$repo" && "$WT" cleanup feature)
    [ ! -e "$path" ]
}

test_cleanup_rejects_uncommitted_changes() {
    repo=$(new_repo dirty)
    path=$(cd "$repo" && "$WT" create feature)
    printf 'dirty\n' >"$path/new-file"
    if (cd "$repo" && "$WT" cleanup feature >"$TMP/output" 2>"$TMP/error"); then
        return 1
    fi
    assert_contains "$(cat "$TMP/error")" 'uncommitted changes'
    [ -d "$path" ]
}

test_cleanup_rejects_unpushed_commit() {
    repo=$(new_repo unpushed)
    path=$(cd "$repo" && "$WT" create feature)
    printf 'change\n' >>"$path/README.md"
    git -C "$path" commit -qam 'Local change'
    if (cd "$repo" && "$WT" cleanup feature >"$TMP/output" 2>"$TMP/error"); then
        return 1
    fi
    assert_contains "$(cat "$TMP/error")" 'unpushed commits'
    [ -d "$path" ]
}

test_force_cleanup() {
    repo=$(new_repo force)
    path=$(cd "$repo" && "$WT" create feature)
    printf 'dirty\n' >"$path/new-file"
    (cd "$repo" && "$WT" cleanup --force feature)
    [ ! -e "$path" ]
}

test_cleanup_pushed_commit() {
    repo=$(new_repo pushed)
    remote=$TMP/pushed-remote.git
    git init -q --bare "$remote"
    git -C "$repo" remote add origin "$remote"
    git -C "$repo" push -qu origin main

    path=$(cd "$repo" && "$WT" create feature)
    printf 'change\n' >>"$path/README.md"
    git -C "$path" commit -qam 'Pushed change'
    git -C "$path" push -qu origin feature

    (cd "$repo" && "$WT" cleanup feature)
    [ ! -e "$path" ]
}

test_cleanup_missing_is_success() {
    repo=$(new_repo missing)
    (cd "$repo" && "$WT" cleanup absent)
}

test_nested_branch_name() {
    repo=$(new_repo nested)
    path=$(cd "$repo" && "$WT" create feature/login)
    assert_eq "$path" "$repo.worktrees/feature/login"
    (cd "$repo" && "$WT" cleanup feature/login)
    [ ! -d "$repo.worktrees/feature" ]
}

mkdir -p "$TMP"
TMP=$(CDPATH= cd -- "$TMP" && pwd -P)
run_test 'create makes a sibling worktree' test_create
run_test 'create is idempotent' test_create_is_idempotent
run_test 'create checks out an existing local branch' test_create_existing_local_branch
run_test 'create explains repositories with no commits' test_create_explains_unborn_branch
run_test 'create enforces the base branch' test_create_rejects_dev_branch
run_test 'cleanup removes a clean worktree' test_cleanup_clean_worktree
run_test 'cleanup rejects uncommitted changes' test_cleanup_rejects_uncommitted_changes
run_test 'cleanup rejects unpushed commits' test_cleanup_rejects_unpushed_commit
run_test 'force cleanup skips validation' test_force_cleanup
run_test 'cleanup permits pushed commits' test_cleanup_pushed_commit
run_test 'cleanup succeeds for a missing worktree' test_cleanup_missing_is_success
run_test 'nested branch names are supported' test_nested_branch_name

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
