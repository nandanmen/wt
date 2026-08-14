# wt

`wt` is a CLI for creating and cleaning up Git worktrees in a predictable
sibling directory.

```sh
wt create feature-branch
# /absolute/path/to/repo.worktrees/feature-branch

wt get feature-branch
# /absolute/path/to/repo.worktrees/feature-branch

wt switch feature-branch
# /absolute/path/to/repo.worktrees/feature-branch

cd $(wt switch feature-branch)

wt cleanup feature-branch
```

Worktrees can normally only be created while the current worktree is on
`main` or `staging`. Use `--allow-dev-branches` to deliberately create one from
another branch:

```sh
wt create --allow-dev-branches experiment
```

The repository must have at least one commit before Git can create a worktree.

Cleanup refuses to remove a worktree with uncommitted or unpushed changes. Use
`--force` to skip both checks:

```sh
wt cleanup --force experiment
```

## Install

Requires [Rust](https://www.rust-lang.org/tools/install) and Git.

```sh
cargo install --path .
```

Or:

```sh
make install PREFIX="$HOME/.local"
```

## Test

```sh
make test
```
