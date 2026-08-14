use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

pub fn git_root() -> Result<PathBuf> {
    let output = git(&["rev-parse", "--show-toplevel"])?;
    if !output.status.success() {
        return Err(Error::msg(
            "the current directory is not inside a Git worktree",
        ));
    }
    stdout_path(&output)
}

pub fn validate_branch_name(branch: &str) -> Result<()> {
    if branch.is_empty() {
        return Err(Error::msg("a branch name is required"));
    }
    let output = git(&["check-ref-format", "--branch", branch])?;
    if !output.status.success() {
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
        .any(|worktree| worktree.path == path))
}

pub fn current_branch(repo: &Path) -> Result<String> {
    let output = git_in(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if !output.status.success() {
        return Err(Error::msg("the current worktree has a detached HEAD"));
    }
    stdout_string(&output)
}

pub fn has_commits(repo: &Path) -> Result<bool> {
    let output = git_in(repo, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?;
    Ok(output.status.success())
}

pub fn branch_exists(repo: &Path, branch: &str) -> Result<bool> {
    let output = git_in(
        repo,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )?;
    Ok(output.status.success())
}

pub fn add_worktree(
    repo: &Path,
    destination: &Path,
    branch: &str,
    create_branch: bool,
) -> Result<()> {
    let dest = path_arg(destination)?;
    let mut args = vec!["worktree", "add", "--quiet"];
    if create_branch {
        args.extend(["-b", branch, dest, "HEAD"]);
    } else {
        args.extend([dest, branch]);
    }
    let output = git_in_visible(repo, &args)?;
    if !output.status.success() {
        if create_branch {
            return Err(Error::msg(format!(
                "failed to create worktree and branch {branch}"
            )));
        }
        return Err(Error::msg(format!(
            "failed to create worktree for existing branch {branch}"
        )));
    }
    Ok(())
}

pub fn remove_worktree(repo: &Path, destination: &Path, force: bool) -> Result<()> {
    let dest = path_arg(destination)?;
    let mut args = vec!["worktree", "remove"];
    if force {
        args.extend(["--force", "--force"]);
    }
    args.push(dest);
    let output = git_in_visible(repo, &args)?;
    if !output.status.success() {
        return Err(Error::msg(format!(
            "failed to remove worktree: {}",
            destination.display()
        )));
    }
    Ok(())
}

pub fn prune_worktrees(repo: &Path) {
    let _ = git_in(repo, &["worktree", "prune"]);
}

pub fn has_uncommitted_changes(path: &Path) -> Result<bool> {
    let output = git_in(path, &["status", "--porcelain", "--untracked-files=normal"])?;
    if !output.status.success() {
        return Err(Error::msg(format!(
            "failed to inspect worktree: {}",
            path.display()
        )));
    }
    Ok(!stdout_string(&output)?.is_empty())
}

pub fn has_unpushed_commits(path: &Path) -> Result<bool> {
    let upstream = git_in(
        path,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )?;
    if upstream.status.success() {
        let upstream_name = stdout_string(&upstream)?;
        let range = format!("{upstream_name}..HEAD");
        return Ok(rev_list_count(path, &[&range])? > 0);
    }

    let remotes = git_in(
        path,
        &["for-each-ref", "--format=%(refname)", "refs/remotes"],
    )?;
    if !remotes.status.success() {
        return Err(Error::msg(format!(
            "failed to inspect remotes in {}",
            path.display()
        )));
    }
    if !stdout_string(&remotes)?.is_empty() {
        return Ok(rev_list_count(path, &["HEAD", "--not", "--remotes"])? > 0);
    }

    let mut exclusions = Vec::new();
    for base in ["main", "staging"] {
        if branch_exists(path, base)? {
            exclusions.push(format!("refs/heads/{base}"));
        }
    }
    if exclusions.is_empty() {
        return Ok(true);
    }

    let mut args = vec!["HEAD".to_string(), "--not".to_string()];
    args.extend(exclusions);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(rev_list_count(path, &arg_refs)? > 0)
}

pub fn parse_worktree_list(text: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current_path = None;
    let mut current_branch = None;

    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            flush_worktree(&mut worktrees, &mut current_path, &mut current_branch);
            current_path = Some(PathBuf::from(path));
            continue;
        }
        if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(branch.to_string());
            continue;
        }
        if line.is_empty() {
            flush_worktree(&mut worktrees, &mut current_path, &mut current_branch);
        }
    }
    flush_worktree(&mut worktrees, &mut current_path, &mut current_branch);
    worktrees
}

fn list_worktrees() -> Result<Vec<Worktree>> {
    let output = git(&["worktree", "list", "--porcelain"])?;
    if !output.status.success() {
        return Err(Error::msg("failed to list git worktrees"));
    }
    Ok(parse_worktree_list(&stdout_string(&output)?))
}

fn flush_worktree(
    worktrees: &mut Vec<Worktree>,
    current_path: &mut Option<PathBuf>,
    current_branch: &mut Option<String>,
) {
    if let Some(path) = current_path.take() {
        worktrees.push(Worktree {
            path,
            branch: current_branch.take(),
        });
    }
}

fn rev_list_count(path: &Path, extra: &[&str]) -> Result<u64> {
    let mut args = vec!["rev-list", "--count"];
    args.extend(extra);
    let output = git_in(path, &args)?;
    if !output.status.success() {
        return Err(Error::msg(format!(
            "failed to inspect commits in {}",
            path.display()
        )));
    }
    stdout_string(&output)?
        .parse()
        .map_err(|_| Error::msg("git rev-list returned a non-numeric count"))
}

fn git(args: &[&str]) -> Result<Output> {
    run_git(None, args, Stdio::piped())
}

fn git_in(dir: &Path, args: &[&str]) -> Result<Output> {
    run_git(Some(dir), args, Stdio::piped())
}

fn git_in_visible(dir: &Path, args: &[&str]) -> Result<Output> {
    run_git(Some(dir), args, Stdio::inherit())
}

fn run_git(dir: Option<&Path>, args: &[&str], stderr: Stdio) -> Result<Output> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    command
        .args(args)
        .stderr(stderr)
        .output()
        .map_err(|err| Error::msg(format!("failed to run git: {err}")))
}

fn stdout_string(output: &Output) -> Result<String> {
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| Error::msg("git produced non-UTF-8 output"))?;
    Ok(text.trim_end_matches(['\n', '\r']).to_string())
}

fn stdout_path(output: &Output) -> Result<PathBuf> {
    Ok(PathBuf::from(stdout_string(output)?))
}

fn path_arg(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| Error::msg(format!("path is not valid UTF-8: {}", path.display())))
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

    #[test]
    fn parse_worktree_list_reads_paths_and_branches() {
        let text = "\
worktree /dev/my-app
HEAD abcdef
branch refs/heads/main

worktree /dev/my-app.worktrees/feature
HEAD 123456
branch refs/heads/feature
";
        let worktrees = parse_worktree_list(text);
        assert_eq!(
            worktrees,
            vec![
                Worktree {
                    path: PathBuf::from("/dev/my-app"),
                    branch: Some("refs/heads/main".to_string()),
                },
                Worktree {
                    path: PathBuf::from("/dev/my-app.worktrees/feature"),
                    branch: Some("refs/heads/feature".to_string()),
                },
            ]
        );
    }
}
