use std::fmt;
use std::io::{self, Write};
use std::process::ExitCode;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    message: String,
    prefixed: bool,
}

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            prefixed: true,
        }
    }

    pub fn raw(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            prefixed: false,
        }
    }

    pub fn print(&self) {
        let mut stderr = io::stderr().lock();
        let _ = if self.prefixed {
            writeln!(stderr, "wt: {}", self.message)
        } else {
            writeln!(stderr, "{}", self.message)
        };
    }

    pub fn exit_code(&self) -> ExitCode {
        ExitCode::from(1)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}
