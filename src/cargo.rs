use std::ops::Deref;
use std::path::Path;
use std::process::Command;
use std::str;

use crate::rust::{RustDiagnostic, Type};

#[derive(Clone, Debug, Default)]
pub struct CompileResult {
    pub success: bool,
    pub errors: Vec<RustDiagnostic>,
    pub warnings: Vec<RustDiagnostic>,
}

enum ParseState {
    Nothing,
    Diagnostic(String),
}

pub fn run<P: AsRef<Path>>(project_root: P, command: &str) -> Result<CompileResult, String> {
    let inp;
    let (cmd, args) = if cfg!(target_os = "windows") {
        inp = ["/C", command];
        ("cmd", inp.into_iter().map(Deref::deref).collect::<Vec<_>>())
    } else {
        inp = ["-c", command];
        ("sh", inp.into_iter().map(Deref::deref).collect::<Vec<_>>())
    };

    let command = Command::new(cmd)
        .args(&args)
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("{:?}", e))?;
    let output = str::from_utf8(&command.stderr).map_err(|e| format!("{:?}", e))?;

    let mut state = ParseState::Nothing;
    let mut result = CompileResult {
        success: command.status.success(),
        errors: vec![],
        warnings: vec![],
    };
    for line in output.lines() {
        match state {
            ParseState::Nothing => {
                // skip the line if it does not begin with "warning" or "error"
                if line.starts_with("warning") || line.starts_with("error") {
                    state = ParseState::Diagnostic(String::from(&format!("{}\n", line)));
                }
            }
            ParseState::Diagnostic(mut diag) => {
                // if the line is empty, then we are done
                state = if line.is_empty() {
                    let diag: RustDiagnostic = diag.parse()?;
                    match diag.type_ {
                        Type::Error => result.errors.push(diag),
                        Type::Warning => result.warnings.push(diag),
                    };
                    ParseState::Nothing
                } else {
                    diag.push_str(&format!("{}\n", line));
                    ParseState::Diagnostic(diag)
                }
            }
        }
    }

    Ok(result)
}
