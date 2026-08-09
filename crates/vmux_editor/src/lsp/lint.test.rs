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
