use std::path::Path;

use serde_json::Value;
use vmux_core::event::{DiagSeverity, FileDiagnostic};

use crate::lsp::registry::{LintFormat, LinterSpec};

fn diag(
    line: u32,
    col: u32,
    end_col: u32,
    sev: DiagSeverity,
    msg: String,
    src: &str,
) -> FileDiagnostic {
    let start = col.saturating_sub(1);
    FileDiagnostic {
        line: line.saturating_sub(1),
        start_col: start,
        end_col: end_col.saturating_sub(1).max(start),
        severity: sev,
        message: msg,
        source: Some(src.into()),
    }
}

pub fn parse_ruff(stdout: &str) -> Vec<FileDiagnostic> {
    let Ok(arr) = serde_json::from_str::<Vec<Value>>(stdout) else {
        return vec![];
    };
    arr.iter()
        .filter_map(|v| {
            let loc = v.get("location")?;
            let row = loc.get("row")?.as_u64()? as u32;
            let col = loc.get("column")?.as_u64()? as u32;
            let end_col = v
                .get("end_location")
                .and_then(|e| e.get("column"))
                .and_then(|c| c.as_u64())
                .map(|c| c as u32)
                .unwrap_or(col + 1);
            let code = v.get("code").and_then(|c| c.as_str()).unwrap_or("");
            let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let message = if code.is_empty() {
                msg.to_string()
            } else {
                format!("{code}: {msg}")
            };
            Some(diag(
                row,
                col,
                end_col,
                DiagSeverity::Warning,
                message,
                "ruff",
            ))
        })
        .collect()
}

pub fn parse_eslint(stdout: &str) -> Vec<FileDiagnostic> {
    let Ok(files) = serde_json::from_str::<Vec<Value>>(stdout) else {
        return vec![];
    };
    let mut out = Vec::new();
    for f in &files {
        let Some(msgs) = f.get("messages").and_then(|m| m.as_array()) else {
            continue;
        };
        for m in msgs {
            let line = m.get("line").and_then(|x| x.as_u64()).unwrap_or(1) as u32;
            let col = m.get("column").and_then(|x| x.as_u64()).unwrap_or(1) as u32;
            let end_col = m
                .get("endColumn")
                .and_then(|x| x.as_u64())
                .map(|c| c as u32)
                .unwrap_or(col + 1);
            let sev = match m.get("severity").and_then(|x| x.as_u64()).unwrap_or(1) {
                2 => DiagSeverity::Error,
                _ => DiagSeverity::Warning,
            };
            let rule = m.get("ruleId").and_then(|x| x.as_str()).unwrap_or("");
            let msg = m.get("message").and_then(|x| x.as_str()).unwrap_or("");
            let message = if rule.is_empty() {
                msg.to_string()
            } else {
                format!("{msg} ({rule})")
            };
            out.push(diag(line, col, end_col, sev, message, "eslint"));
        }
    }
    out
}

pub fn parse_shellcheck(stdout: &str) -> Vec<FileDiagnostic> {
    let Ok(arr) = serde_json::from_str::<Vec<Value>>(stdout) else {
        return vec![];
    };
    arr.iter()
        .filter_map(|v| {
            let line = v.get("line")?.as_u64()? as u32;
            let col = v.get("column")?.as_u64()? as u32;
            let end_col = v
                .get("endColumn")
                .and_then(|x| x.as_u64())
                .map(|c| c as u32)
                .unwrap_or(col + 1);
            let sev = match v.get("level").and_then(|x| x.as_str()).unwrap_or("warning") {
                "error" => DiagSeverity::Error,
                "info" => DiagSeverity::Info,
                "style" => DiagSeverity::Hint,
                _ => DiagSeverity::Warning,
            };
            let code = v.get("code").and_then(|x| x.as_u64());
            let msg = v.get("message").and_then(|x| x.as_str()).unwrap_or("");
            let message = code
                .map(|c| format!("SC{c}: {msg}"))
                .unwrap_or_else(|| msg.to_string());
            Some(diag(line, col, end_col, sev, message, "shellcheck"))
        })
        .collect()
}

fn parse(format: LintFormat, stdout: &str) -> Vec<FileDiagnostic> {
    match format {
        LintFormat::Ruff => parse_ruff(stdout),
        LintFormat::Eslint => parse_eslint(stdout),
        LintFormat::Shellcheck => parse_shellcheck(stdout),
    }
}

/// Run `spec` against `path` (blocking; call off the main thread). Linters exit
/// non-zero when they find issues — that's expected, not an error.
pub fn run_linter(spec: &LinterSpec, path: &Path) -> Vec<FileDiagnostic> {
    let output = std::process::Command::new(&spec.command)
        .args(&spec.args)
        .arg(path)
        .output();
    match output {
        Ok(o) => parse(spec.format, &String::from_utf8_lossy(&o.stdout)),
        Err(e) => {
            tracing::debug!(linter = %spec.command, "lint run failed: {e}");
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruff_json_parses() {
        let s = r#"[{"code":"F401","message":"unused import","location":{"row":3,"column":8},"end_location":{"row":3,"column":14}}]"#;
        let d = parse_ruff(s);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 2);
        assert_eq!(d[0].start_col, 7);
        assert_eq!(d[0].end_col, 13);
        assert_eq!(d[0].severity, DiagSeverity::Warning);
        assert!(d[0].message.starts_with("F401:"));
        assert_eq!(d[0].source.as_deref(), Some("ruff"));
    }

    #[test]
    fn eslint_json_parses_severity() {
        let s = r#"[{"filePath":"a.ts","messages":[{"ruleId":"no-unused","severity":2,"message":"x is unused","line":1,"column":5,"endColumn":6}]}]"#;
        let d = parse_eslint(s);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, DiagSeverity::Error);
        assert_eq!(d[0].line, 0);
        assert_eq!(d[0].start_col, 4);
        assert!(d[0].message.contains("no-unused"));
    }

    #[test]
    fn shellcheck_json_parses() {
        let s = r#"[{"file":"-","line":2,"column":1,"endColumn":5,"level":"warning","code":2086,"message":"Double quote"}]"#;
        let d = parse_shellcheck(s);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
        assert!(d[0].message.starts_with("SC2086:"));
        assert_eq!(d[0].source.as_deref(), Some("shellcheck"));
    }

    #[test]
    fn empty_or_garbage_is_no_diagnostics() {
        assert!(parse_ruff("").is_empty());
        assert!(parse_eslint("not json").is_empty());
        assert!(parse_shellcheck("[]").is_empty());
    }
}
