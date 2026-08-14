use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::git;

pub fn create(branch: &str, allow_dev_branches: bool) -> Result<()> {
    git::validate_branch_name(branch)?;

    let repo = git::git_root()?;
    let root = git::worktrees_root(&repo)?;
    let destination = root.join(git::worktree_name(branch));

    if let Some(existing) = git::worktree_for_branch(branch)? {
        eprintln!("Worktree for branch {branch} already exists.");
        println!("{}", existing.display());
        return Ok(());
    }

    let current_branch = git::current_branch(&repo)?;
    if !allow_dev_branches && current_branch != "main" && current_branch != "staging" {
        return Err(Error::msg(format!(
            "worktrees can only be created from main or staging (currently on {current_branch}); pass --allow-dev-branches to override"
        )));
    }

    if !git::has_commits(&repo)? {
        return Err(Error::msg(
            "repository has no commits yet; create an initial commit before creating a worktree",
        ));
    }

    if destination.exists() {
        return Err(Error::msg(format!(
            "destination already exists but is not a registered worktree: {}",
            destination.display()
        )));
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| Error::msg("could not create worktree directory"))?;
    }

    let create_branch = !git::branch_exists(&repo, branch)?;
    git::add_worktree(&repo, &destination, branch, create_branch)?;
    println!("{}", destination.display());
    Ok(())
}

pub fn cleanup(branch: &str, force: bool) -> Result<()> {
    git::validate_branch_name(branch)?;

    let repo = git::git_root()?;
    let root = git::worktrees_root(&repo)?;
    let destination = root.join(git::worktree_name(branch));

    if !destination.is_dir() {
        git::prune_worktrees(&repo);
        return Ok(());
    }

    if !git::is_registered_worktree(&destination)? {
        return Ok(());
    }

    if !force {
        if git::has_uncommitted_changes(&destination)? {
            return Err(Error::msg(format!(
                "worktree has uncommitted changes: {}",
                destination.display()
            )));
        }
        if git::has_unpushed_commits(&destination)? {
            return Err(Error::msg(format!("branch has unpushed commits: {branch}")));
        }
    }

    git::remove_worktree(&repo, &destination, force)?;
    remove_empty_parents(&destination, &root);
    Ok(())
}

pub fn get(branch: &str) -> Result<()> {
    git::validate_branch_name(branch)?;

    let repo = git::git_root()?;
    if branch == "main" || branch == "staging" {
        println!("{}", repo.display());
        return Ok(());
    }

    match git::worktree_for_branch(branch)? {
        Some(existing) => {
            println!("{}", existing.display());
            Ok(())
        }
        None => Err(Error::raw(format!("worktree on branch {branch} not found"))),
    }
}

fn remove_empty_parents(destination: &Path, root: &Path) {
    let mut parent = destination.parent();
    while let Some(path) = parent {
        if path == root {
            break;
        }
        if fs::remove_dir(path).is_err() {
            break;
        }
        parent = path.parent();
    }
}
