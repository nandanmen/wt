# `wt`

`wt` is a CLI for managing worktrees. It has three commands:

## `wt create`

Creates a `git` worktree for the current working directory.

Worktrees are stored adjacent to the current working directory in a `<directory>.worktrees` folder, e.g. given:

```
dev/
    my-app/
```

And:

```sh
cd dev/my-app
wt create feature-branch
```

The final folder structure will be:

```
dev/
    my-app/
    my-app.worktrees/
        feature-branch/
```

### Arguments

```
wt create <branch-name>
```

- `branch-name` is both the name of the worktree and the name of the branch in that worktree.

On completion, the command will print the absolute path to the worktree.

### Checks

- If <branch-name> already exists, say that it exists and return the absolute path to that worktree.
- Worktrees should be created off the current branch. Exit if the current working directory is not on the `main` or `staging` branch, unless the user passes the `--allow-dev-branches` flag.

## `wt cleanup`

```
wt cleanup <branch-name>
```

Deletes the given worktree.

- Fails if the branch has uncommitted or unpushed changes
- Supports a `--force` flag that skips validation and deletes the worktree anyway.
- Succeeds (exit 0) if the branch doesn't exist.

## `wt get`

```
wt get <branch-name>
```

Prints the absolute path to the worktree for the given branch.

- Returns the absolute path if the worktree exists.
- If `branch-name` is `main` or `staging`, returns the main project's directory.
- Prints `worktree on branch <branch-name> not found` to stderr and exits with code 1 if no worktree is found.
