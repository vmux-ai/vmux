use std::path::Path;

#[derive(Clone, Debug)]
pub struct CodeRequirement(#[cfg(target_os = "macos")] String);

impl CodeRequirement {
    pub fn for_binary(path: &Path) -> std::io::Result<Self> {
        #[cfg(target_os = "macos")]
        return native::Code::requirement(path).map(Self);
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            Err(std::io::Error::other(
                "Desktop signing verification is unavailable",
            ))
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalPeer {
    uid: u32,
    pid: Option<u32>,
    verified_code: bool,
}

impl LocalPeer {
    pub fn inspect(
        stream: &tokio::net::UnixStream,
        requirement: Option<&CodeRequirement>,
    ) -> std::io::Result<Self> {
        let credentials = stream.peer_cred()?;
        let uid = credentials.uid();
        let pid = credentials.pid().map(|pid| pid as u32);
        #[cfg(target_os = "macos")]
        let verified_code = match requirement {
            Some(requirement) => native::Code::peer(stream, &requirement.0).unwrap_or(false),
            None => false,
        };
        #[cfg(not(target_os = "macos"))]
        let verified_code = {
            let _ = requirement;
            false
        };
        Ok(Self {
            uid,
            pid,
            verified_code,
        })
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
    pub fn verified_code(&self) -> bool {
        self.verified_code
    }
}

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::c_void;
    use std::os::fd::AsRawFd;
    use std::path::Path;

    type NativeRef = *const c_void;

    pub(super) struct Code;

    impl Code {
        pub(super) fn requirement(path: &Path) -> std::io::Result<String> {
            let path = std::fs::canonicalize(path)?;
            let url = Object::url(&path)?;
            let mut pointer = std::ptr::null();
            let status = unsafe { SecStaticCodeCreateWithPath(url.0, 0, &mut pointer) };
            let code = Object::status(status, pointer)?;
            let status = unsafe { SecStaticCodeCheckValidity(code.0, 0, std::ptr::null()) };
            if status != 0 {
                return Err(std::io::Error::other(format!(
                    "invalid trusted executable signature: {status}"
                )));
            }
            let mut pointer = std::ptr::null();
            let status = unsafe { SecCodeCopyDesignatedRequirement(code.0, 0, &mut pointer) };
            let requirement = Object::status(status, pointer)?;
            let mut pointer = std::ptr::null();
            let status = unsafe { SecRequirementCopyString(requirement.0, 0, &mut pointer) };
            Object::status(status, pointer)?.text()
        }

        pub(super) fn peer(
            stream: &tokio::net::UnixStream,
            requirement: &str,
        ) -> std::io::Result<bool> {
            let mut token = [0u32; 8];
            let mut length = std::mem::size_of_val(&token) as libc::socklen_t;
            let status = unsafe {
                libc::getsockopt(
                    stream.as_raw_fd(),
                    0,
                    0x006,
                    token.as_mut_ptr().cast(),
                    &mut length,
                )
            };
            if status != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if length as usize != std::mem::size_of_val(&token) {
                return Err(std::io::Error::other("invalid Unix peer audit token"));
            }
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    token.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(&token),
                )
            };
            let audit = Object::data(bytes)?;
            let key = unsafe { kSecGuestAttributeAudit };
            let attributes = Object::dictionary(key, audit.0)?;
            let mut pointer = std::ptr::null();
            let status = unsafe {
                SecCodeCopyGuestWithAttributes(std::ptr::null(), attributes.0, 0, &mut pointer)
            };
            let guest = Object::status(status, pointer)?;
            let text = Object::string(requirement)?;
            let mut pointer = std::ptr::null();
            let status = unsafe { SecRequirementCreateWithString(text.0, 0, &mut pointer) };
            let requirement = Object::status(status, pointer)?;
            Ok(unsafe { SecCodeCheckValidity(guest.0, 0, requirement.0) } == 0)
        }
    }

    struct Object(NativeRef);

    impl Object {
        fn status(status: i32, pointer: NativeRef) -> std::io::Result<Self> {
            if status != 0 || pointer.is_null() {
                return Err(std::io::Error::other(format!(
                    "native signing operation failed: {status}"
                )));
            }
            Ok(Self(pointer))
        }

        fn url(path: &Path) -> std::io::Result<Self> {
            let bytes = path.as_os_str().as_encoded_bytes();
            let pointer = unsafe {
                CFURLCreateFromFileSystemRepresentation(
                    std::ptr::null(),
                    bytes.as_ptr(),
                    bytes.len() as isize,
                    false,
                )
            };
            Self::status(0, pointer)
        }

        fn string(text: &str) -> std::io::Result<Self> {
            let pointer = unsafe {
                CFStringCreateWithBytes(
                    std::ptr::null(),
                    text.as_ptr(),
                    text.len() as isize,
                    0x08000100,
                    false,
                )
            };
            Self::status(0, pointer)
        }

        fn data(bytes: &[u8]) -> std::io::Result<Self> {
            let pointer =
                unsafe { CFDataCreate(std::ptr::null(), bytes.as_ptr(), bytes.len() as isize) };
            Self::status(0, pointer)
        }

        fn dictionary(key: NativeRef, value: NativeRef) -> std::io::Result<Self> {
            let pointer = unsafe {
                CFDictionaryCreate(
                    std::ptr::null(),
                    &key,
                    &value,
                    1,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            Self::status(0, pointer)
        }

        fn text(&self) -> std::io::Result<String> {
            let length = unsafe { CFStringGetLength(self.0) };
            let maximum = unsafe { CFStringGetMaximumSizeForEncoding(length, 0x08000100) };
            if !(0..=65535).contains(&maximum) {
                return Err(std::io::Error::other(
                    "signing requirement exceeds native bound",
                ));
            }
            let mut bytes = vec![0u8; maximum as usize + 1];
            if !unsafe {
                CFStringGetCString(
                    self.0,
                    bytes.as_mut_ptr().cast(),
                    bytes.len() as isize,
                    0x08000100,
                )
            } {
                return Err(std::io::Error::other(
                    "invalid signing requirement encoding",
                ));
            }
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| std::io::Error::other("unterminated signing requirement"))?;
            String::from_utf8(bytes[..end].to_vec()).map_err(std::io::Error::other)
        }
    }

    impl Drop for Object {
        fn drop(&mut self) {
            unsafe {
                CFRelease(self.0);
            }
        }
    }

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecGuestAttributeAudit: NativeRef;
        fn SecStaticCodeCreateWithPath(path: NativeRef, flags: u32, code: *mut NativeRef) -> i32;
        fn SecStaticCodeCheckValidity(code: NativeRef, flags: u32, requirement: NativeRef) -> i32;
        fn SecCodeCopyDesignatedRequirement(
            code: NativeRef,
            flags: u32,
            requirement: *mut NativeRef,
        ) -> i32;
        fn SecRequirementCopyString(
            requirement: NativeRef,
            flags: u32,
            text: *mut NativeRef,
        ) -> i32;
        fn SecRequirementCreateWithString(
            text: NativeRef,
            flags: u32,
            requirement: *mut NativeRef,
        ) -> i32;
        fn SecCodeCopyGuestWithAttributes(
            host: NativeRef,
            attributes: NativeRef,
            flags: u32,
            guest: *mut NativeRef,
        ) -> i32;
        fn SecCodeCheckValidity(code: NativeRef, flags: u32, requirement: NativeRef) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: NativeRef);
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: NativeRef,
            bytes: *const u8,
            length: isize,
            directory: bool,
        ) -> NativeRef;
        fn CFStringCreateWithBytes(
            allocator: NativeRef,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            external: bool,
        ) -> NativeRef;
        fn CFDataCreate(allocator: NativeRef, bytes: *const u8, length: isize) -> NativeRef;
        fn CFDictionaryCreate(
            allocator: NativeRef,
            keys: *const NativeRef,
            values: *const NativeRef,
            count: isize,
            key_callbacks: NativeRef,
            value_callbacks: NativeRef,
        ) -> NativeRef;
        fn CFStringGetLength(string: NativeRef) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(
            string: NativeRef,
            bytes: *mut i8,
            length: isize,
            encoding: u32,
        ) -> bool;
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn kernel_peer_matches_its_live_signed_executable_and_refuses_a_foreign_requirement() {
        let (server, _client) = tokio::net::UnixStream::pair().unwrap();
        let expected = CodeRequirement::for_binary(&std::env::current_exe().unwrap()).unwrap();
        let peer = LocalPeer::inspect(&server, Some(&expected)).unwrap();
        assert_eq!(peer.uid(), unsafe { libc::geteuid() });
        assert_eq!(peer.pid(), Some(std::process::id()));
        assert!(peer.verified_code());
        let foreign = CodeRequirement::for_binary(Path::new("/bin/ls")).unwrap();
        assert!(
            !LocalPeer::inspect(&server, Some(&foreign))
                .unwrap()
                .verified_code()
        );
        assert!(!LocalPeer::inspect(&server, None).unwrap().verified_code());
    }

    #[test]
    fn an_unsigned_file_cannot_become_a_trusted_executable_requirement() {
        let path = std::env::temp_dir().join(format!("vmx235-unsigned-{}", std::process::id()));
        std::fs::write(&path, b"not a signed executable").unwrap();
        let rejected = CodeRequirement::for_binary(&path).is_err();
        std::fs::remove_file(path).unwrap();
        assert!(rejected);
    }
}
