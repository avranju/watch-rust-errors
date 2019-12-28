use std::fmt::{self, Display};
use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref REGEX_ERR: Regex = Regex::new(r"(error|warning)(\[(E[0-9]+)\])?: (.*)").unwrap();
    static ref REGEX_CONTEXT: Regex = Regex::new(r" +--> ([^:]+):([0-9]+):([0-9]+)").unwrap();
}

#[derive(Clone, Debug)]
pub enum Type {
    Error,
    Warning,
}

impl Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Error => write!(f, "error"),
            Type::Warning => write!(f, "warning"),
        }
    }
}

impl FromStr for Type {
    type Err = String;

    fn from_str(inp: &str) -> Result<Self, Self::Err> {
        match inp {
            "error" => Ok(Type::Error),
            "warning" => Ok(Type::Warning),
            _ => Err(format!("Invalid rust diagnostic type {}", inp)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RustDiagnostic {
    pub type_: Type,
    pub num: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub details: Option<String>,
}

impl RustDiagnostic {
    fn new(
        type_: Type,
        num: Option<&str>,
        message: &str,
        file: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
        details: Option<&str>,
    ) -> Self {
        RustDiagnostic {
            type_,
            num: num.map(|s| s.to_owned()),
            message: message.to_owned(),
            file: file.map(ToString::to_string),
            line,
            column,
            details: details.map(ToString::to_string),
        }
    }
}

impl Display for RustDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}: {}\n",
            self.type_,
            self.num
                .as_ref()
                .map(|n| format!("[{}]", n))
                .unwrap_or_else(|| "".to_string()),
            self.message
        )?;

        if self.file.is_some() {
            write!(
                f,
                "  --> {}:{}:{}\n",
                self.file.as_ref().unwrap(),
                self.line
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                self.column
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "-".to_string())
            )?;
        }

        if self.details.is_some() {
            write!(f, "{}\n", self.details.as_ref().unwrap())?;
        }

        Ok(())
    }
}

impl FromStr for RustDiagnostic {
    type Err = String;

    fn from_str(inp: &str) -> Result<Self, Self::Err> {
        let err_handler = || format!("Invalid input: {}", inp);

        // split input into 3 lines delimited by \n
        let lines: Vec<&str> = inp.splitn(3, '\n').collect();

        // extract error number and message
        let err = REGEX_ERR.captures(lines[0]).ok_or_else(err_handler)?;

        let err_or_warn = err.get(1).ok_or_else(err_handler)?;
        let err_num = err.get(3);
        let msg = err.get(4).ok_or_else(err_handler)?;

        // extract file, line and col
        let (file, line, col) = if lines.len() > 1 && !lines[1].is_empty() {
            let context = REGEX_CONTEXT.captures(lines[1]).ok_or_else(err_handler)?;
            let file = context.get(1);
            let line = context.get(2);
            let col = context.get(3);

            (file, line, col)
        } else {
            (None, None, None)
        };

        let details = if lines.len() > 2 && !lines[2].is_empty() {
            Some(lines[2])
        } else {
            None
        };

        Ok(RustDiagnostic::new(
            err_or_warn.as_str().parse()?,
            err_num.map(|e| e.as_str()),
            msg.as_str(),
            file.map(|m| m.as_str()),
            line.map(|m| m.as_str().parse().expect("Line number was not a number!")),
            col.map(|m| m.as_str().parse().expect("Column number was not a number!")),
            details,
        ))
    }
}
