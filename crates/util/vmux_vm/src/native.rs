use std::ffi::CString;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use libloading::Library;

use crate::Settings;
use crate::process::{Lease, ParentWatch};

pub(crate) struct Context {
    library: Library,
    firmware: PathBuf,
    id: u32,
    consumed: bool,
}

impl Context {
    pub(crate) fn run(settings: Settings) -> io::Result<()> {
        let _lease = Lease::named(&settings.directory, "machine.lock")?;
        let mut context = Self::new()?;
        context.configure(&settings)?;
        context.listen(&settings)?;
        ParentWatch::monitor(settings.clone())?;
        let enter = context.function::<unsafe extern "C" fn(u32) -> i32>(b"krun_start_enter\0")?;
        context.consumed = true;
        Self::check(unsafe { enter(context.id) }, "start VM")
    }

    fn new() -> io::Result<Self> {
        let paths = [
            (
                "/opt/homebrew/opt/libkrun/lib/libkrun.1.dylib",
                "/opt/homebrew/opt/libkrunfw/lib/libkrunfw.5.dylib",
            ),
            (
                "/usr/local/opt/libkrun/lib/libkrun.1.dylib",
                "/usr/local/opt/libkrunfw/lib/libkrunfw.5.dylib",
            ),
        ];
        for (path, firmware) in paths {
            if !Path::new(path).exists() {
                continue;
            }
            let library = unsafe { Library::new(path) }.map_err(io::Error::other)?;
            let create =
                unsafe { library.get::<unsafe extern "C" fn() -> i32>(b"krun_create_ctx\0") }
                    .map_err(io::Error::other)?;
            let id = unsafe { create() };
            Self::check(id, "create VM")?;
            return Ok(Self {
                library,
                firmware: PathBuf::from(firmware),
                id: id as u32,
                consumed: false,
            });
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "libkrun is not installed",
        ))
    }

    fn function<T: Copy>(&self, name: &[u8]) -> io::Result<T> {
        let symbol = unsafe { self.library.get::<T>(name) }.map_err(io::Error::other)?;
        Ok(*symbol)
    }

    fn configure(&self, settings: &Settings) -> io::Result<()> {
        let kernel = self.kernel_image(settings)?;
        let load_kernel = self.function::<unsafe extern "C" fn(
            u32,
            *const i8,
            u32,
            *const i8,
            *const i8,
        ) -> i32>(b"krun_set_kernel\0")?;
        let kernel = Self::string(kernel)?;
        Self::check(
            unsafe {
                load_kernel(
                    self.id,
                    kernel.as_ptr(),
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            "configure Linux kernel",
        )?;
        let configure =
            self.function::<unsafe extern "C" fn(u32, u8, u32) -> i32>(b"krun_set_vm_config\0")?;
        Self::check(
            unsafe { configure(self.id, settings.cpus, settings.memory_mib) },
            "configure VM",
        )?;
        self.path(b"krun_set_root\0", &settings.rootfs())?;
        self.path(
            b"krun_set_console_output\0",
            &settings.directory.join("guest.log"),
        )?;

        let disk = self.function::<unsafe extern "C" fn(
            u32,
            *const i8,
            *const i8,
            u32,
            bool,
            bool,
            u32,
        ) -> i32>(b"krun_add_disk3\0")?;
        let disk_path = Self::string(settings.disk())?;
        Self::check(
            unsafe {
                disk(
                    self.id,
                    c"data".as_ptr(),
                    disk_path.as_ptr(),
                    0,
                    false,
                    false,
                    2,
                )
            },
            "attach data disk",
        )?;

        let shared = self.function::<unsafe extern "C" fn(u32, *const i8, *const i8) -> i32>(
            b"krun_add_virtiofs\0",
        )?;
        let shared_path = Self::string(&settings.shared_directory)?;
        Self::check(
            unsafe { shared(self.id, c"vmux-shared".as_ptr(), shared_path.as_ptr()) },
            "share project directory",
        )?;

        let network = self
            .function::<unsafe extern "C" fn(u32, *const i8, i32, *mut u8, u32, u32) -> i32>(
                b"krun_add_net_unixgram\0",
            )?;
        let network_path = Self::string(settings.network_socket())?;
        let mut mac = [0x5a, 0x94, 0xef, 0xe4, 0x0c, 0xee];
        Self::check(
            unsafe { network(self.id, network_path.as_ptr(), -1, mac.as_mut_ptr(), 0, 1) },
            "attach network",
        )?;

        let vsock = self.function::<unsafe extern "C" fn(u32, u32, *const i8, bool) -> i32>(
            b"krun_add_vsock_port2\0",
        )?;
        let socket_path = Self::string(settings.transport_socket())?;
        Self::check(
            unsafe { vsock(self.id, 2375, socket_path.as_ptr(), true) },
            "attach Docker socket",
        )?;
        let control_path = Self::string(settings.guest_control())?;
        Self::check(
            unsafe { vsock(self.id, 2376, control_path.as_ptr(), true) },
            "attach guest control socket",
        )?;

        let execute = self.function::<unsafe extern "C" fn(
            u32,
            *const i8,
            *const *const i8,
            *const *const i8,
        ) -> i32>(b"krun_set_exec\0")?;
        let shared_environment = CString::new(format!(
            "VMUX_SHARED_PATH={}",
            settings.shared_directory.display()
        ))
        .map_err(io::Error::other)?;
        let disk_environment = CString::new(format!(
            "VMUX_NEW_DISK={}",
            u8::from(settings.directory.join("data.new").exists())
        ))
        .map_err(io::Error::other)?;
        let environment = [
            c"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".as_ptr(),
            shared_environment.as_ptr(),
            disk_environment.as_ptr(),
            std::ptr::null(),
        ];
        let arguments = [std::ptr::null()];
        Self::check(
            unsafe {
                execute(
                    self.id,
                    c"/sbin/vmux-init".as_ptr(),
                    arguments.as_ptr(),
                    environment.as_ptr(),
                )
            },
            "configure guest init",
        )
    }

    fn listen(&self, settings: &Settings) -> io::Result<()> {
        let listener = UnixListener::bind(settings.control_socket())?;
        let guest_control = settings.guest_control();
        std::thread::Builder::new()
            .name("vmux-vm-control".into())
            .spawn(move || {
                for connection in listener.incoming() {
                    let Ok(mut connection) = connection else {
                        continue;
                    };
                    let _ = connection.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                    let mut line = String::new();
                    let mut reader = BufReader::new(&mut connection);
                    let request = reader.read_line(&mut line);
                    if request.is_err() || line.len() > 32 || line != "STOP\n" {
                        let _ = connection.write_all(b"INVALID\n");
                        continue;
                    }
                    let forwarded = Self::stop_guest(&guest_control);
                    match forwarded {
                        Ok(()) => {
                            let _ = connection.write_all(b"STOPPING\n");
                        }
                        Err(_) => {
                            let _ = connection.write_all(b"FAILED\n");
                        }
                    }
                }
            })?;
        Ok(())
    }

    fn kernel_image(&self, settings: &Settings) -> io::Result<PathBuf> {
        let firmware = unsafe { Library::new(&self.firmware) }.map_err(io::Error::other)?;
        let load = unsafe {
            firmware.get::<unsafe extern "C" fn(*mut u64, *mut u64, *mut usize) -> *const u8>(
                b"krunfw_get_kernel\0",
            )
        }
        .map_err(io::Error::other)?;
        let mut base = 0;
        let mut entry = 0;
        let mut length = 0;
        let image = unsafe { load(&mut base, &mut entry, &mut length) };
        if image.is_null() || !(64..=256 * 1024 * 1024).contains(&length) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid firmware kernel image",
            ));
        }
        let image = unsafe { std::slice::from_raw_parts(image, length) };
        if &image[56..60] != b"ARMd" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "firmware kernel is not an ARM64 Linux image",
            ));
        }
        let path = settings.directory.join("kernel.raw");
        let mut temporary = tempfile::NamedTempFile::new_in(&settings.directory)?;
        temporary.write_all(image)?;
        temporary.as_file().sync_all()?;
        temporary.persist(&path).map_err(io::Error::other)?;
        Ok(path)
    }

    fn stop_guest(path: &Path) -> io::Result<()> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
        stream.write_all(b"STOP\n")?;
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply)?;
        if reply != "STOPPING\n" {
            return Err(io::Error::other("guest refused shutdown"));
        }
        Ok(())
    }

    fn path(&self, name: &[u8], path: &Path) -> io::Result<()> {
        let configure = self.function::<unsafe extern "C" fn(u32, *const i8) -> i32>(name)?;
        let path = Self::string(path)?;
        Self::check(
            unsafe { configure(self.id, path.as_ptr()) },
            "configure VM path",
        )
    }

    fn string(path: impl AsRef<Path>) -> io::Result<CString> {
        CString::new(path.as_ref().as_os_str().as_bytes()).map_err(io::Error::other)
    }

    fn check(result: i32, operation: &str) -> io::Result<()> {
        if result >= 0 {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "{operation}: {}",
            io::Error::from_raw_os_error(-result)
        )))
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        if let Ok(free) = self.function::<unsafe extern "C" fn(u32) -> i32>(b"krun_free_ctx\0") {
            unsafe { free(self.id) };
        }
    }
}
