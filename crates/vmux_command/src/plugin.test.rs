#[test]
fn command_plugin_logs_app_commands_before_readers() {
    let source = include_str!("plugin.rs");
    let log_needle = ["info!(target: ", "\"vmux_command::app_command\""].concat();
    assert!(source.contains("log_app_commands"));
    assert!(source.contains(".after(WriteAppCommands)"));
    assert!(source.contains(".before(ReadAppCommands)"));
    assert!(source.contains(&log_needle));
}
