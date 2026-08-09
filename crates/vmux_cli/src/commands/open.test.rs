use super::*;
use std::cell::RefCell;

struct RecordingLauncher {
    calls: RefCell<Vec<String>>,
}

impl AppLauncher for RecordingLauncher {
    fn launch(&self, app_name: &str) -> io::Result<()> {
        self.calls.borrow_mut().push(app_name.to_string());
        Ok(())
    }
}

#[test]
fn run_launches_desktop_app() {
    let launcher = RecordingLauncher {
        calls: RefCell::new(Vec::new()),
    };
    run(&launcher).unwrap();
    assert_eq!(launcher.calls.borrow().as_slice(), &["Vmux".to_string()]);
}

#[test]
fn run_propagates_launcher_error() {
    struct FailingLauncher;
    impl AppLauncher for FailingLauncher {
        fn launch(&self, _: &str) -> io::Result<()> {
            Err(io::Error::other("boom"))
        }
    }
    let err = run(&FailingLauncher).unwrap_err();
    assert_eq!(err.to_string(), "boom");
}
