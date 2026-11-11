pub use engine::{Engine, Status};
pub use settings::Settings;

mod engine;
mod http;
mod image;
mod ports;
mod settings;
mod ssh;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod native;

#[cfg(target_os = "macos")]
mod sandbox;

pub struct NativeVm;

impl NativeVm {
    pub fn run(settings: Settings) -> std::io::Result<()> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            native::Context::run(settings)
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            let _ = settings;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "the native VM backend currently requires Apple Silicon macOS",
            ))
        }
    }
}
