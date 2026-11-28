use std::io::{self, Write};

use bevy::ecs::entity::Entity;
use bevy::ecs::message::Messages;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};

pub struct RawGuard {
    original: libc::termios,
    track_motion: bool,
}

impl RawGuard {
    pub fn enter(track_motion: bool) -> io::Result<Self> {
        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = original;
        raw.c_iflag &= !(libc::ICRNL | libc::IXON);
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut stdout = io::stdout().lock();
        let _ =
            stdout.write_all(b"\x1b[?1049h\x1b[2J\x1b[H\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[>1u");
        if track_motion {
            let _ = stdout.write_all(b"\x1b[?1003h");
        }
        let _ = stdout.flush();
        Ok(Self {
            original,
            track_motion,
        })
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
        }
        let mut stdout = io::stdout().lock();
        if self.track_motion {
            let _ = stdout.write_all(b"\x1b[?1003l");
        }
        let _ = stdout.write_all(b"\x1b[<u\x1b[?1006l\x1b[?1000l\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = stdout.flush();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub button: u16,
    pub col: u16,
    pub row: u16,
    pub pressed: bool,
    pub consumed: usize,
}

impl MouseEvent {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 9 || bytes[0] != 0x1b || bytes[1] != b'[' || bytes[2] != b'<' {
            return None;
        }
        let mut idx = 3usize;
        let mut nums = [0u16; 3];
        let mut part = 0usize;
        while idx < bytes.len() {
            let b = bytes[idx];
            if b.is_ascii_digit() {
                nums[part] = nums[part]
                    .saturating_mul(10)
                    .saturating_add(u16::from(b - b'0'));
            } else if b == b';' && part < 2 {
                part += 1;
            } else if (b == b'M' || b == b'm') && part == 2 {
                return Some(Self {
                    button: nums[0],
                    col: nums[1],
                    row: nums[2],
                    pressed: b == b'M',
                    consumed: idx + 1,
                });
            } else {
                return None;
            }
            idx += 1;
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedKey {
    pub key: KeyCode,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
    pub consumed: usize,
    csi_byte: Option<u8>,
}

impl DecodedKey {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if let Some(csi_u) = Self::parse_csi_u(bytes) {
            return Some(csi_u);
        }
        match bytes {
            [0x1b, b'[' | b'O', b'A', ..] => Some(Self::plain(KeyCode::ArrowUp, 3)),
            [0x1b, b'[' | b'O', b'B', ..] => Some(Self::plain(KeyCode::ArrowDown, 3)),
            [0x1b, b'[' | b'O', b'C', ..] => Some(Self::plain(KeyCode::ArrowRight, 3)),
            [0x1b, b'[' | b'O', b'D', ..] => Some(Self::plain(KeyCode::ArrowLeft, 3)),
            [0x1b, byte, ..] if byte.is_ascii_graphic() => {
                let mut inner = Self::parse_ascii(*byte)?;
                inner.alt = true;
                inner.consumed = 2;
                Some(inner)
            }
            [0x1b] => Some(Self::plain(KeyCode::Escape, 1)),
            [byte, ..] => Self::parse_ascii(*byte),
            [] => None,
        }
    }

    pub fn pty_bytes(self, raw: &[u8]) -> Vec<u8> {
        let Some(byte) = self.csi_byte else {
            return raw.to_vec();
        };
        if self.super_key {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(2);
        if self.alt {
            out.push(0x1b);
        }
        if self.ctrl {
            let lower = byte.to_ascii_lowercase();
            if lower.is_ascii_lowercase() {
                out.push(lower - b'a' + 1);
            } else if byte == b'[' {
                out.push(0x1b);
            } else {
                out.push(byte);
            }
        } else if self.shift && byte.is_ascii_lowercase() {
            out.push(byte.to_ascii_uppercase());
        } else {
            out.push(byte);
        }
        out
    }

    pub fn write_events(
        self,
        state: ButtonState,
        window: Entity,
        messages: &mut Messages<KeyboardInput>,
    ) {
        let mut emit = |key_code: KeyCode| {
            messages.write(KeyboardInput {
                key_code,
                logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
                state,
                text: None,
                repeat: false,
                window,
            });
        };
        if state == ButtonState::Pressed {
            if self.ctrl {
                emit(KeyCode::ControlLeft);
            }
            if self.shift {
                emit(KeyCode::ShiftLeft);
            }
            if self.alt {
                emit(KeyCode::AltLeft);
            }
            if self.super_key {
                emit(KeyCode::SuperLeft);
            }
            emit(self.key);
        } else {
            emit(self.key);
            if self.super_key {
                emit(KeyCode::SuperLeft);
            }
            if self.alt {
                emit(KeyCode::AltLeft);
            }
            if self.shift {
                emit(KeyCode::ShiftLeft);
            }
            if self.ctrl {
                emit(KeyCode::ControlLeft);
            }
        }
    }

    fn parse_csi_u(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 || bytes[0] != 0x1b || bytes[1] != b'[' {
            return None;
        }
        let end = bytes[2..].iter().position(|&b| b == b'u')? + 2;
        let body = std::str::from_utf8(&bytes[2..end]).ok()?;
        let mut parts = body.split(';');
        let code_part = parts.next()?.split(':').next()?;
        let codepoint = code_part.parse::<u32>().ok()?;
        let mods = parts
            .next()
            .and_then(|value| value.split(':').next())
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1)
            .saturating_sub(1);
        let byte = u8::try_from(codepoint).ok()?;
        let mut decoded = if byte == 0x1b {
            Self::plain(KeyCode::Escape, end + 1)
        } else {
            Self::parse_ascii(byte)?
        };
        decoded.shift |= (mods & 1) != 0;
        decoded.alt |= (mods & 2) != 0;
        decoded.ctrl |= (mods & 4) != 0;
        decoded.super_key |= (mods & 8) != 0;
        decoded.consumed = end + 1;
        decoded.csi_byte = Some(byte);
        Some(decoded)
    }

    fn plain(key: KeyCode, consumed: usize) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
            super_key: false,
            consumed,
            csi_byte: None,
        }
    }

    fn parse_ascii(byte: u8) -> Option<Self> {
        match byte {
            0x01..=0x1a if !matches!(byte, b'\t' | b'\r' | b'\n') => {
                let key = Self::letter(byte - 1 + b'a')?;
                Some(Self {
                    key,
                    ctrl: true,
                    shift: false,
                    alt: false,
                    super_key: false,
                    consumed: 1,
                    csi_byte: None,
                })
            }
            b'a'..=b'z' => Some(Self::plain(Self::letter(byte)?, 1)),
            b'A'..=b'Z' => Some(Self {
                key: Self::letter(byte.to_ascii_lowercase())?,
                ctrl: false,
                shift: true,
                alt: false,
                super_key: false,
                consumed: 1,
                csi_byte: None,
            }),
            b'0'..=b'9' => Some(Self::plain(Self::digit(byte)?, 1)),
            b'%' => Some(Self::shifted(KeyCode::Digit5)),
            b'"' => Some(Self::shifted(KeyCode::Quote)),
            b'&' => Some(Self::shifted(KeyCode::Digit7)),
            b'{' => Some(Self::shifted(KeyCode::BracketLeft)),
            b'}' => Some(Self::shifted(KeyCode::BracketRight)),
            b'<' => Some(Self::shifted(KeyCode::Comma)),
            b'>' => Some(Self::shifted(KeyCode::Period)),
            b'=' => Some(Self::plain(KeyCode::Equal, 1)),
            b'-' => Some(Self::plain(KeyCode::Minus, 1)),
            b'[' => Some(Self::plain(KeyCode::BracketLeft, 1)),
            b']' => Some(Self::plain(KeyCode::BracketRight, 1)),
            b'\'' => Some(Self::plain(KeyCode::Quote, 1)),
            b',' => Some(Self::plain(KeyCode::Comma, 1)),
            b'.' => Some(Self::plain(KeyCode::Period, 1)),
            b' ' => Some(Self::plain(KeyCode::Space, 1)),
            b'\r' | b'\n' => Some(Self::plain(KeyCode::Enter, 1)),
            b'\t' => Some(Self::plain(KeyCode::Tab, 1)),
            _ => None,
        }
    }

    fn shifted(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: false,
            shift: true,
            alt: false,
            super_key: false,
            consumed: 1,
            csi_byte: None,
        }
    }

    fn letter(byte: u8) -> Option<KeyCode> {
        Some(match byte {
            b'a' => KeyCode::KeyA,
            b'b' => KeyCode::KeyB,
            b'c' => KeyCode::KeyC,
            b'd' => KeyCode::KeyD,
            b'e' => KeyCode::KeyE,
            b'f' => KeyCode::KeyF,
            b'g' => KeyCode::KeyG,
            b'h' => KeyCode::KeyH,
            b'i' => KeyCode::KeyI,
            b'j' => KeyCode::KeyJ,
            b'k' => KeyCode::KeyK,
            b'l' => KeyCode::KeyL,
            b'm' => KeyCode::KeyM,
            b'n' => KeyCode::KeyN,
            b'o' => KeyCode::KeyO,
            b'p' => KeyCode::KeyP,
            b'q' => KeyCode::KeyQ,
            b'r' => KeyCode::KeyR,
            b's' => KeyCode::KeyS,
            b't' => KeyCode::KeyT,
            b'u' => KeyCode::KeyU,
            b'v' => KeyCode::KeyV,
            b'w' => KeyCode::KeyW,
            b'x' => KeyCode::KeyX,
            b'y' => KeyCode::KeyY,
            b'z' => KeyCode::KeyZ,
            _ => return None,
        })
    }

    fn digit(byte: u8) -> Option<KeyCode> {
        Some(match byte {
            b'0' => KeyCode::Digit0,
            b'1' => KeyCode::Digit1,
            b'2' => KeyCode::Digit2,
            b'3' => KeyCode::Digit3,
            b'4' => KeyCode::Digit4,
            b'5' => KeyCode::Digit5,
            b'6' => KeyCode::Digit6,
            b'7' => KeyCode::Digit7,
            b'8' => KeyCode::Digit8,
            b'9' => KeyCode::Digit9,
            _ => return None,
        })
    }
}

pub struct TerminalIo;

impl TerminalIo {
    pub fn size() -> (u16, u16) {
        let mut ws = unsafe { std::mem::zeroed::<libc::winsize>() };
        if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0
            && ws.ws_col > 0
            && ws.ws_row > 0
        {
            (ws.ws_col, ws.ws_row)
        } else {
            (120, 36)
        }
    }

    pub fn poll_stdin(buffer: &mut [u8], timeout_ms: i32) -> Option<usize> {
        let mut pfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if ready < 0 {
            return Some(0);
        }
        if ready == 0 {
            return Some(0);
        }
        if (pfd.revents & libc::POLLIN) == 0 {
            if (pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL)) != 0 {
                return None;
            }
            return Some(0);
        }
        let n = unsafe {
            libc::read(
                libc::STDIN_FILENO,
                buffer.as_mut_ptr().cast::<libc::c_void>(),
                buffer.len(),
            )
        };
        if n > 0 {
            Some(n as usize)
        } else if n == 0 {
            None
        } else {
            match io::Error::last_os_error().kind() {
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => Some(0),
                _ => None,
            }
        }
    }

    pub fn incomplete_escape_tail(bytes: &[u8]) -> Option<usize> {
        if bytes.is_empty() || bytes[0] != 0x1b {
            return None;
        }
        if bytes.len() == 1 {
            return Some(1);
        }
        if bytes[1] == b'O' {
            if bytes.len() == 2 {
                return Some(2);
            }
            return None;
        }
        if bytes[1] == b'[' {
            if bytes.len() == 2 {
                return Some(2);
            }
            if bytes[2] == b'<' {
                let complete = bytes[3..].iter().any(|&b| b == b'M' || b == b'm');
                if !complete && bytes.len() < 32 {
                    return Some(bytes.len());
                }
                return None;
            }
            let complete = bytes[2..].iter().any(|&b| (0x40..=0x7e).contains(&b));
            if !complete && bytes.len() < 32 {
                return Some(bytes.len());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_ctrl_chords_shifted_split_symbols_arrows_and_sgr_mouse() {
        let leader = DecodedKey::parse(b"\x02").unwrap();
        assert_eq!(
            (leader.key, leader.ctrl, leader.shift),
            (KeyCode::KeyB, true, false)
        );
        let vsplit = DecodedKey::parse(b"%").unwrap();
        assert_eq!((vsplit.key, vsplit.shift), (KeyCode::Digit5, true));
        let hsplit = DecodedKey::parse(b"\"").unwrap();
        assert_eq!((hsplit.key, hsplit.shift), (KeyCode::Quote, true));
        let left = DecodedKey::parse(b"\x1b[D").unwrap();
        assert_eq!((left.key, left.consumed), (KeyCode::ArrowLeft, 3));
        let click = MouseEvent::parse(b"\x1b[<0;12;5M").unwrap();
        assert_eq!(
            (click.button, click.col, click.row, click.pressed),
            (0, 12, 5, true)
        );
        let super_shift_z = DecodedKey::parse(b"\x1b[122;10u").unwrap();
        assert_eq!(
            (
                super_shift_z.key,
                super_shift_z.super_key,
                super_shift_z.shift
            ),
            (KeyCode::KeyZ, true, true)
        );
        let ctrl_c = DecodedKey::parse(b"\x1b[99;5u").unwrap();
        assert_eq!(ctrl_c.pty_bytes(b"\x1b[99;5u"), vec![0x03]);
    }
}
