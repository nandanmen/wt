use std::path::{Path, PathBuf};

use git2::{
    BranchType, ErrorCode, Oid, Repository, StatusOptions, WorktreeAddOptions, WorktreePruneOptions,
};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Worktree {
    path: PathBuf,
    branch: Option<String>,
}

pub fn git_root() -> Result<PathBuf> {
    workdir(&open_cwd()?)
}

pub fn validate_branch_name(branch: &str) -> Result<()> {
    if branch.is_empty() {
        return Err(Error::msg("a branch name is required"));
    }
    let refname = format!("refs/heads/{branch}");
    if !git2::Reference::is_valid_name(&refname) {
        return Err(Error::msg(format!("invalid branch name: {branch}")));
    }
    Ok(())
}

pub fn worktree_name(branch: &str) -> String {
    branch.replace('/', "__")
}

pub fn worktrees_root(repo: &Path) -> Result<PathBuf> {
    let repo_name = repo.file_name().ok_or_else(|| {
        Error::msg(format!(
            "could not determine repository name from {}",
            repo.display()
        ))
    })?;
    let repo_parent = repo.parent().ok_or_else(|| {
        Error::msg(format!(
            "could not determine parent directory of {}",
            repo.display()
        ))
    })?;
    let mut root = repo_parent.to_path_buf();
    let mut dir_name = repo_name.to_os_string();
    dir_name.push(".worktrees");
    root.push(dir_name);
    Ok(root)
}

pub fn worktree_for_branch(branch: &str) -> Result<Option<PathBuf>> {
    let wanted = format!("refs/heads/{branch}");
    Ok(list_worktrees()?
        .into_iter()
        .find(|worktree| worktree.branch.as_deref() == Some(wanted.as_str()))
        .map(|worktree| worktree.path))
}

pub fn is_registered_worktree(path: &Path) -> Result<bool> {
    Ok(list_worktrees()?
        .iter()
        .any(|worktree| paths_match(&worktree.path, path)))
}

pub fn current_branch(repo: &Path) -> Result<String> {
    let repo = open_at(repo)?;
    let head = repo
        .find_reference("HEAD")
        .map_err(|err| map_git(err, "failed to read HEAD"))?;
    match head.symbolic_target() {
        Some(target) => Ok(target
            .strip_prefix("refs/heads/")
            .unwrap_or(target)
            .to_string()),
        None => Err(Error::msg("the current worktree has a detached HEAD")),
    }
}

pub fn has_commits(repo: &Path) -> Result<bool> {
    let repo = open_at(repo)?;
    if repo
        .is_empty()
        .map_err(|err| map_git(err, "failed to inspect repository"))?
    {
        return Ok(false);
    }
    Ok(repo.head().ok().and_then(|head| head.target()).is_some())
}

pub fn branch_exists(repo: &Path, branch: &str) -> Result<bool> {
    branch_exists_in(&open_at(repo)?, branch)
}

pub fn add_worktree(
    repo: &Path,
    destination: &Path,
    branch: &str,
    create_branch: bool,
) -> Result<()> {
    let repo = open_at(repo)?;
    let name = worktree_name(branch);

    let branch_ref = if create_branch {
        let commit = repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|err| map_git(err, "failed to read HEAD"))?;
        repo.branch(branch, &commit, false).map_err(|err| {
            map_git(
                err,
                format!("failed to create worktree and branch {branch}"),
            )
        })?
    } else {
        repo.find_branch(branch, BranchType::Local).map_err(|err| {
            map_git(
                err,
                format!("failed to create worktree for existing branch {branch}"),
            )
        })?
    };

    let mut opts = WorktreeAddOptions::new();
    opts.reference(Some(branch_ref.get()));
    repo.worktree(&name, destination, Some(&opts))
        .map_err(|err| {
            if create_branch {
                map_git(
                    err,
                    format!("failed to create worktree and branch {branch}"),
                )
            } else {
                map_git(
                    err,
                    format!("failed to create worktree for existing branch {branch}"),
                )
            }
        })?;
    Ok(())
}

pub fn remove_worktree(repo: &Path, destination: &Path, force: bool) -> Result<()> {
    let repo = open_at(repo)?;
    let worktree = find_worktree_by_path(&repo, destination)?;
    let mut opts = WorktreePruneOptions::new();
    opts.valid(true).working_tree(true);
    if force {
        opts.locked(true);
    }
    worktree.prune(Some(&mut opts)).map_err(|err| {
        map_git(
            err,
            format!("failed to remove worktree: {}", destination.display()),
        )
    })
}

pub fn prune_worktrees(repo: &Path) {
    let Ok(repo) = open_at(repo) else {
        return;
    };
    let Ok(names) = repo.worktrees() else {
        return;
    };
    for name in names.iter().flatten() {
        let Ok(worktree) = repo.find_worktree(name) else {
            continue;
        };
        if worktree.is_prunable(None).unwrap_or(false) {
            let _ = worktree.prune(None);
        }
    }
}

pub fn has_uncommitted_changes(path: &Path) -> Result<bool> {
    let repo = open_at(path)?;
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo.statuses(Some(&mut opts)).map_err(|err| {
        map_git(
            err,
            format!("failed to inspect worktree: {}", path.display()),
        )
    })?;
    Ok(!statuses.is_empty())
}

pub fn has_unpushed_commits(path: &Path) -> Result<bool> {
    let repo = open_at(path)?;
    if let Some(upstream) = upstream_oid(&repo)? {
        let head = head_oid(&repo)?;
        let (ahead, _) = repo.graph_ahead_behind(head, upstream).map_err(|err| {
            map_git(
                err,
                format!("failed to inspect commits in {}", path.display()),
            )
        })?;
        return Ok(ahead > 0);
    }

    let remote_oids = remote_tracking_oids(&repo)?;
    if !remote_oids.is_empty() {
        return commits_not_in(&repo, &remote_oids);
    }

    let mut exclusions = Vec::new();
    for base in ["main", "staging"] {
        if branch_exists_in(&repo, base)? {
            if let Ok(branch) = repo.find_branch(base, BranchType::Local) {
                if let Ok(commit) = branch.get().peel_to_commit() {
                    exclusions.push(commit.id());
                }
            }
        }
    }
    if exclusions.is_empty() {
        return Ok(true);
    }
    commits_not_in(&repo, &exclusions)
}

fn list_worktrees() -> Result<Vec<Worktree>> {
    let repo = open_cwd()?;
    let mut worktrees = Vec::new();

    if let Some(path) = main_worktree_path(&repo) {
        let branch = open_at(&path).ok().as_ref().and_then(checked_out_branch);
        worktrees.push(Worktree { path, branch });
    }

    let names = repo
        .worktrees()
        .map_err(|err| map_git(err, "failed to list git worktrees"))?;
    for name in names.iter().flatten() {
        let worktree = repo
            .find_worktree(name)
            .map_err(|err| map_git(err, "failed to list git worktrees"))?;
        let path = normalize_path(worktree.path());
        let branch = open_at(&path).ok().as_ref().and_then(checked_out_branch);
        worktrees.push(Worktree { path, branch });
    }
    Ok(worktrees)
}

fn find_worktree_by_path(repo: &Repository, path: &Path) -> Result<git2::Worktree> {
    let names = repo
        .worktrees()
        .map_err(|err| map_git(err, "failed to list git worktrees"))?;
    let names: Vec<String> = names.iter().flatten().map(ToString::to_string).collect();
    for name in names {
        let worktree = repo
            .find_worktree(&name)
            .map_err(|err| map_git(err, "failed to list git worktrees"))?;
        if paths_match(worktree.path(), path) {
            return Ok(worktree);
        }
    }
    Err(Error::msg(format!(
        "failed to remove worktree: {}",
        path.display()
    )))
}

fn open_cwd() -> Result<Repository> {
    Repository::discover(".")
        .map_err(|_| Error::msg("the current directory is not inside a Git worktree"))
}

fn open_at(path: &Path) -> Result<Repository> {
    Repository::open(path)
        .or_else(|_| Repository::discover(path))
        .map_err(|_| Error::msg("the current directory is not inside a Git worktree"))
}

fn workdir(repo: &Repository) -> Result<PathBuf> {
    let path = repo
        .workdir()
        .ok_or_else(|| Error::msg("the current directory is not inside a Git worktree"))?;
    Ok(normalize_path(path))
}

fn main_worktree_path(repo: &Repository) -> Option<PathBuf> {
    if repo.is_worktree() {
        repo.path()
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .map(normalize_path)
    } else {
        repo.workdir().map(normalize_path)
    }
}

fn checked_out_branch(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    if head.is_branch() {
        head.name().map(ToString::to_string)
    } else {
        None
    }
}

fn branch_exists_in(repo: &Repository, branch: &str) -> Result<bool> {
    match repo.find_branch(branch, BranchType::Local) {
        Ok(_) => Ok(true),
        Err(err) if err.code() == ErrorCode::NotFound => Ok(false),
        Err(err) => Err(map_git(err, "failed to look up branch")),
    }
}

fn upstream_oid(repo: &Repository) -> Result<Option<Oid>> {
    let Ok(head) = repo.head() else {
        return Ok(None);
    };
    let Some(name) = head.shorthand() else {
        return Ok(None);
    };
    let Ok(branch) = repo.find_branch(name, BranchType::Local) else {
        return Ok(None);
    };
    match branch.upstream() {
        Ok(upstream) => Ok(Some(
            upstream
                .get()
                .peel_to_commit()
                .map_err(|err| map_git(err, "failed to inspect upstream"))?
                .id(),
        )),
        Err(_) => Ok(None),
    }
}

fn head_oid(repo: &Repository) -> Result<Oid> {
    repo.head()
        .and_then(|head| head.peel_to_commit())
        .map(|commit| commit.id())
        .map_err(|err| map_git(err, "failed to read HEAD"))
}

fn remote_tracking_oids(repo: &Repository) -> Result<Vec<Oid>> {
    let refs = repo
        .references_glob("refs/remotes/*")
        .map_err(|err| map_git(err, "failed to inspect remotes"))?;
    let mut oids = Vec::new();
    for reference in refs {
        let reference = reference.map_err(|err| map_git(err, "failed to inspect remotes"))?;
        if let Ok(commit) = reference.peel_to_commit() {
            oids.push(commit.id());
        }
    }
    Ok(oids)
}

fn commits_not_in(repo: &Repository, hidden: &[Oid]) -> Result<bool> {
    let mut walk = repo
        .revwalk()
        .map_err(|err| map_git(err, "failed to inspect commits"))?;
    walk.push_head()
        .map_err(|err| map_git(err, "failed to inspect commits"))?;
    for oid in hidden {
        let _ = walk.hide(*oid);
    }
    match walk.next() {
        Some(Ok(_)) => Ok(true),
        Some(Err(err)) => Err(map_git(err, "failed to inspect commits")),
        None => Ok(false),
    }
}

fn paths_match(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    PathBuf::from(path.to_string_lossy().trim_end_matches('/'))
}

fn map_git(err: git2::Error, message: impl Into<String>) -> Error {
    let message = message.into();
    if err.message().is_empty() {
        Error::msg(message)
    } else {
        Error::msg(format!("{message}: {}", err.message()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_name_flattens_slashes() {
        assert_eq!(worktree_name("feature/login"), "feature__login");
        assert_eq!(worktree_name("plain"), "plain");
    }

    #[test]
    fn worktrees_root_is_sibling_directory() {
        let repo = Path::new("/dev/my-app");
        assert_eq!(
            worktrees_root(repo).unwrap(),
            PathBuf::from("/dev/my-app.worktrees")
        );
    }
}
