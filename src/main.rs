mod commands;
mod error;
mod git;

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use error::{Error, Result};

#[derive(Debug)]
enum Command {
    Create {
        allow_dev_branches: bool,
        branch_name: String,
    },
    Cleanup {
        force: bool,
        branch_name: String,
    },
    Get {
        branch_name: String,
    },
}

fn main() -> ExitCode {
    let mut args = env::args();
    let _program = args.next();
    let args: Vec<String> = args.collect();
    if args.is_empty() {
        let _ = usage_to_stderr();
        return ExitCode::from(1);
    }

    match parse_args(args) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(command)) => match run(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(err),
        },
        Err(err) => fail(err),
    }
}

fn fail(err: Error) -> ExitCode {
    err.print();
    err.exit_code()
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Create {
            allow_dev_branches,
            branch_name,
        } => commands::create(&branch_name, allow_dev_branches),
        Command::Cleanup { force, branch_name } => commands::cleanup(&branch_name, force),
        Command::Get { branch_name } => commands::get(&branch_name),
    }
}

fn parse_args(args: Vec<String>) -> Result<Option<Command>> {
    let mut args = args.into_iter();
    let command = args
        .next()
        .ok_or_else(|| Error::msg("a command is required"))?;

    match command.as_str() {
        "create" => Ok(
            parse_command_args(args.collect(), "create", Some("--allow-dev-branches"))?.map(
                |(allow_dev_branches, branch_name)| Command::Create {
                    allow_dev_branches,
                    branch_name,
                },
            ),
        ),
        "cleanup" => Ok(
            parse_command_args(args.collect(), "cleanup", Some("--force"))?
                .map(|(force, branch_name)| Command::Cleanup { force, branch_name }),
        ),
        "get" => Ok(parse_command_args(args.collect(), "get", None)?
            .map(|(_, branch_name)| Command::Get { branch_name })),
        "-h" | "--help" | "help" => {
            usage()?;
            Ok(None)
        }
        other => Err(Error::msg(format!("unknown command: {other}"))),
    }
}

/// Parse `[--flag] <branch-name>`. Returns `None` when help was requested.
fn parse_command_args(
    args: Vec<String>,
    command: &str,
    allowed_flag: Option<&str>,
) -> Result<Option<(bool, String)>> {
    let mut flag = false;
    let mut branch = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                usage()?;
                return Ok(None);
            }
            "--" => {
                let name = args
                    .next()
                    .ok_or_else(|| Error::msg("a branch name is required"))?;
                if args.next().is_some() {
                    return Err(Error::msg("too many arguments"));
                }
                return Ok(Some((flag, name)));
            }
            option if option.starts_with('-') => {
                if allowed_flag == Some(option) {
                    flag = true;
                } else {
                    return Err(Error::msg(format!(
                        "unknown option for {command}: {option}"
                    )));
                }
            }
            _ => {
                if branch.is_some() {
                    return Err(Error::msg("too many arguments"));
                }
                branch = Some(arg);
            }
        }
    }

    let branch = branch.ok_or_else(|| Error::msg("a branch name is required"))?;
    Ok(Some((flag, branch)))
}

fn usage() -> Result<()> {
    let mut stdout = io::stdout().lock();
    write_usage(&mut stdout).map_err(|err| Error::msg(format!("failed to write usage: {err}")))
}

fn usage_to_stderr() -> Result<()> {
    let mut stderr = io::stderr().lock();
    write_usage(&mut stderr).map_err(|err| Error::msg(format!("failed to write usage: {err}")))
}

fn write_usage(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "\
Usage:
  wt create [--allow-dev-branches] <branch-name>
  wt cleanup [--force] <branch-name>
  wt get <branch-name>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Command {
        parse_args(args.iter().map(|arg| (*arg).to_string()).collect())
            .unwrap()
            .expect("expected a command")
    }

    #[test]
    fn parse_create_accepts_flag_before_or_after_branch() {
        match parse(&["create", "--allow-dev-branches", "feat"]) {
            Command::Create {
                allow_dev_branches,
                branch_name,
            } => {
                assert!(allow_dev_branches);
                assert_eq!(branch_name, "feat");
            }
            _ => panic!("expected create"),
        }
        match parse(&["create", "feat", "--allow-dev-branches"]) {
            Command::Create {
                allow_dev_branches,
                branch_name,
            } => {
                assert!(allow_dev_branches);
                assert_eq!(branch_name, "feat");
            }
            _ => panic!("expected create"),
        }
    }

    #[test]
    fn parse_rejects_unknown_options() {
        let err = parse_args(vec!["cleanup".into(), "--nope".into(), "feat".into()]).unwrap_err();
        assert!(err.to_string().contains("unknown option for cleanup"));
    }

    #[test]
    fn parse_rejects_removed_switch_command() {
        let err = parse_args(vec!["switch".into(), "feat".into()]).unwrap_err();
        assert!(err.to_string().contains("unknown command: switch"));
    }
}
