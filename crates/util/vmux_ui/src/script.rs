pub struct PageScript;

impl PageScript {
    pub fn run(script: impl Into<String>) {
        crate::transport::Host::run_script(script.into());
    }
}
