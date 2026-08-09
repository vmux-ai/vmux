use super::*;

#[test]
fn already_dead_pid_returns_alreadydead() {
    let mut pid = 999_999;
    while pid_alive(pid) {
        pid -= 1;
    }
    let outcome = replace_running(pid, || Ok(()));
    assert_eq!(outcome, ReplaceOutcome::AlreadyDead);
}

fn start_and_detach() -> i32 {
    let child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id() as i32;
    std::thread::spawn(move || {
        let mut c = child;
        let _ = c.wait();
    });
    pid
}

#[test]
fn graceful_shutdown_when_send_succeeds_and_pid_exits() {
    let pid = start_and_detach();

    let outcome = replace_running(pid, || {
        unsafe { libc::kill(pid, libc::SIGTERM) };
        Ok(())
    });
    assert_eq!(outcome, ReplaceOutcome::GracefulShutdown);
}

#[test]
fn escalates_to_sigterm_when_shutdown_send_fails() {
    let pid = start_and_detach();

    let outcome = replace_running(pid, || {
        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "no socket",
        ))
    });
    assert_eq!(outcome, ReplaceOutcome::SigtermExit);
}
