use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;
use vmux_core::event::FileEncoding;

const BINARY_SNIFF_BYTES: usize = 8 * 1024;

const DETECT_SAMPLE_BYTES: usize = 64 * 1024;

const UTF16_SNIFF_BYTES: usize = 4 * 1024;

const UTF16_STRAY_PERCENT: usize = 1;

pub struct DecodedText {
    pub text: String,
    pub encoding: FileEncoding,
}

impl DecodedText {
    pub fn of(bytes: &[u8]) -> Option<Self> {
        if let Some(encoding) = Bom::of(bytes) {
            return Some(Self::forced(bytes, encoding));
        }
        if BinarySniff::rejects(bytes) {
            let encoding = Utf16Sniff::encoding(bytes)?;
            return Some(Self::forced(bytes, encoding));
        }
        if let Ok(text) = std::str::from_utf8(bytes) {
            return Some(Self {
                text: text.to_string(),
                encoding: FileEncoding::Utf8,
            });
        }
        Some(Self::forced(bytes, Detected::of(bytes)))
    }

    pub fn forced(bytes: &[u8], encoding: FileEncoding) -> Self {
        let body = Bom::stripped(bytes, encoding);
        let text = match encoding {
            FileEncoding::Utf16Le => Utf16::decoded(body, u16::from_le_bytes),
            FileEncoding::Utf16Be => Utf16::decoded(body, u16::from_be_bytes),
            FileEncoding::Iso8859_1 => Latin1::decoded(body),
            _ => encoding
                .charset()
                .decode_without_bom_handling(body)
                .0
                .into_owned(),
        };
        Self { text, encoding }
    }
}

pub struct BinarySniff;

impl BinarySniff {
    pub fn rejects(bytes: &[u8]) -> bool {
        bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
    }
}

struct Utf16Sniff {
    ascii: usize,
    strays: usize,
    total: usize,
}

impl Utf16Sniff {
    fn encoding(bytes: &[u8]) -> Option<FileEncoding> {
        if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
            return None;
        }
        let capped = bytes.len().min(UTF16_SNIFF_BYTES);
        let sample = &bytes[..capped - capped % 2];
        let le = Self::of(sample, u16::from_le_bytes);
        let be = Self::of(sample, u16::from_be_bytes);
        match (le.reads_as_text(), be.reads_as_text()) {
            (false, false) => None,
            (true, false) => Some(FileEncoding::Utf16Le),
            (false, true) => Some(FileEncoding::Utf16Be),
            (true, true) => match be.ascii > le.ascii {
                true => Some(FileEncoding::Utf16Be),
                false => Some(FileEncoding::Utf16Le),
            },
        }
    }

    fn of(sample: &[u8], unit: fn([u8; 2]) -> u16) -> Self {
        let mut units = Vec::with_capacity(sample.len() / 2);
        for pair in sample.chunks_exact(2) {
            units.push(unit([pair[0], pair[1]]));
        }
        let mut reading = Self {
            ascii: 0,
            strays: 0,
            total: 0,
        };
        for decoded in char::decode_utf16(units) {
            reading.total += 1;
            let Ok(ch) = decoded else {
                reading.strays += 1;
                continue;
            };
            if Self::stray(ch) {
                reading.strays += 1;
                continue;
            }
            if ch.is_ascii() {
                reading.ascii += 1;
            }
        }
        reading
    }

    fn reads_as_text(&self) -> bool {
        self.total > 0 && self.strays * 100 <= self.total * UTF16_STRAY_PERCENT
    }

    fn stray(ch: char) -> bool {
        if matches!(ch, '\t' | '\n' | '\r') {
            return false;
        }
        let point = u32::from(ch);
        if point < 0x20 || (0x7F..=0x9F).contains(&point) {
            return true;
        }
        if (0xE000..=0xF8FF).contains(&point) || (0xF0000..=0x10FFFF).contains(&point) {
            return true;
        }
        if (0xFDD0..=0xFDEF).contains(&point) {
            return true;
        }
        point & 0xFFFE == 0xFFFE
    }
}

struct Bom;

impl Bom {
    const UTF8: [u8; 3] = [0xEF, 0xBB, 0xBF];
    const UTF16LE: [u8; 2] = [0xFF, 0xFE];
    const UTF16BE: [u8; 2] = [0xFE, 0xFF];

    fn of(bytes: &[u8]) -> Option<FileEncoding> {
        if bytes.starts_with(&Self::UTF8) {
            return Some(FileEncoding::Utf8Bom);
        }
        if bytes.starts_with(&Self::UTF16LE) {
            return Some(FileEncoding::Utf16Le);
        }
        if bytes.starts_with(&Self::UTF16BE) {
            return Some(FileEncoding::Utf16Be);
        }
        None
    }

    fn stripped(bytes: &[u8], encoding: FileEncoding) -> &[u8] {
        let mark = Self::bytes(encoding);
        if mark.is_empty() {
            return bytes;
        }
        bytes.strip_prefix(mark).unwrap_or(bytes)
    }

    fn bytes(encoding: FileEncoding) -> &'static [u8] {
        match encoding {
            FileEncoding::Utf8Bom => &Self::UTF8,
            FileEncoding::Utf16Le => &Self::UTF16LE,
            FileEncoding::Utf16Be => &Self::UTF16BE,
            _ => &[],
        }
    }
}

struct Detected;

impl Detected {
    fn of(bytes: &[u8]) -> FileEncoding {
        let head = &bytes[..bytes.len().min(DETECT_SAMPLE_BYTES)];
        let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
        detector.feed(head, head.len() == bytes.len());
        FileEncoding::of_charset(detector.guess(None, Utf8Detection::Deny))
    }
}

struct Utf16;

impl Utf16 {
    fn decoded(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
        let mut units = Vec::with_capacity(bytes.len() / 2);
        for pair in bytes.chunks_exact(2) {
            units.push(unit([pair[0], pair[1]]));
        }
        String::from_utf16_lossy(&units)
    }

    fn encoded(text: &str, order: fn(u16) -> [u8; 2]) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len() * 2);
        let mut buffer = [0u16; 2];
        for ch in text.chars() {
            for unit in ch.encode_utf16(&mut buffer) {
                out.extend_from_slice(&order(*unit));
            }
        }
        out
    }
}

struct Latin1;

impl Latin1 {
    fn decoded(bytes: &[u8]) -> String {
        bytes.iter().map(|b| char::from(*b)).collect()
    }

    fn encoded(text: &str) -> Result<Vec<u8>, Unmappable> {
        let mut out = Vec::with_capacity(text.len());
        for ch in text.chars() {
            let point = u32::from(ch);
            if point > 0xFF {
                return Err(Unmappable {
                    ch,
                    encoding: FileEncoding::Iso8859_1,
                });
            }
            out.push(point as u8);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unmappable {
    pub ch: char,
    pub encoding: FileEncoding,
}

impl std::fmt::Display for Unmappable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} (U+{:04X}) cannot be written as {}",
            self.ch,
            u32::from(self.ch),
            self.encoding.label()
        )
    }
}

pub struct Reencode {
    pub encoding: FileEncoding,
}

impl Reencode {
    pub fn applied(&self, text: &str) -> Result<Vec<u8>, Unmappable> {
        let body = match self.encoding {
            FileEncoding::Utf8 | FileEncoding::Utf8Bom => text.as_bytes().to_vec(),
            FileEncoding::Utf16Le => Utf16::encoded(text, u16::to_le_bytes),
            FileEncoding::Utf16Be => Utf16::encoded(text, u16::to_be_bytes),
            FileEncoding::Iso8859_1 => Latin1::encoded(text)?,
            _ => {
                let (bytes, _, unmappable) = self.encoding.charset().encode(text);
                if unmappable {
                    return Err(self.first_unmappable(text));
                }
                bytes.into_owned()
            }
        };
        let mark = Bom::bytes(self.encoding);
        if mark.is_empty() {
            return Ok(body);
        }
        let mut out = Vec::with_capacity(mark.len() + body.len());
        out.extend_from_slice(mark);
        out.extend_from_slice(&body);
        Ok(out)
    }

    fn first_unmappable(&self, text: &str) -> Unmappable {
        let charset = self.encoding.charset();
        let mut buffer = [0u8; 4];
        for ch in text.chars() {
            let (_, _, unmappable) = charset.encode(ch.encode_utf8(&mut buffer));
            if unmappable {
                return Unmappable {
                    ch,
                    encoding: self.encoding,
                };
            }
        }
        Unmappable {
            ch: char::REPLACEMENT_CHARACTER,
            encoding: self.encoding,
        }
    }
}

pub trait Charset: Sized {
    fn charset(self) -> &'static Encoding;
    fn of_charset(charset: &'static Encoding) -> Self;
}

impl Charset for FileEncoding {
    fn charset(self) -> &'static Encoding {
        match self {
            Self::Utf8 | Self::Utf8Bom | Self::Utf16Le | Self::Utf16Be => encoding_rs::UTF_8,
            Self::ShiftJis => encoding_rs::SHIFT_JIS,
            Self::EucJp => encoding_rs::EUC_JP,
            Self::Iso2022Jp => encoding_rs::ISO_2022_JP,
            Self::Gbk => encoding_rs::GBK,
            Self::Big5 => encoding_rs::BIG5,
            Self::EucKr => encoding_rs::EUC_KR,
            Self::Windows1252 | Self::Iso8859_1 => encoding_rs::WINDOWS_1252,
        }
    }

    fn of_charset(charset: &'static Encoding) -> Self {
        match charset.name() {
            "UTF-8" => Self::Utf8,
            "Shift_JIS" => Self::ShiftJis,
            "EUC-JP" => Self::EucJp,
            "ISO-2022-JP" => Self::Iso2022Jp,
            "GBK" | "gb18030" => Self::Gbk,
            "Big5" => Self::Big5,
            "EUC-KR" => Self::EucKr,
            _ => Self::Windows1252,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Reencode {
        fn bytes(text: &str, encoding: FileEncoding) -> Vec<u8> {
            Self { encoding }
                .applied(text)
                .unwrap_or_else(|e| panic!("{} cannot encode: {e}", encoding.label()))
        }
    }

    #[test]
    fn a_shift_jis_file_decodes_to_the_japanese_it_holds() {
        let bytes = Reencode::bytes("日本語のテキスト\n", FileEncoding::ShiftJis);

        let out = DecodedText::of(&bytes).expect("shift_jis is text");

        assert_eq!(out.text, "日本語のテキスト\n");
        assert_eq!(out.encoding, FileEncoding::ShiftJis);
    }

    #[test]
    fn a_euc_jp_file_is_not_read_as_shift_jis() {
        let bytes = Reencode::bytes(
            "吾輩は猫である。名前はまだ無い。どこで生れたか頓と見当がつかぬ。\n",
            FileEncoding::EucJp,
        );

        let out = DecodedText::of(&bytes).expect("euc-jp is text");

        assert_eq!(out.encoding, FileEncoding::EucJp);
        assert!(out.text.starts_with("吾輩は猫である"), "got {}", out.text);
    }

    #[test]
    fn a_bom_decides_the_encoding_before_the_detector_runs() {
        for encoding in [
            FileEncoding::Utf8Bom,
            FileEncoding::Utf16Le,
            FileEncoding::Utf16Be,
        ] {
            let bytes = Reencode::bytes("héllo\n", encoding);

            let out = DecodedText::of(&bytes).expect("a bom marks text");

            assert_eq!(out.encoding, encoding, "{}", encoding.label());
            assert_eq!(out.text, "héllo\n", "{}", encoding.label());
        }
    }

    #[test]
    fn a_bom_is_kept_in_the_file_and_left_out_of_the_text() {
        let bytes = Reencode::bytes("x", FileEncoding::Utf8Bom);

        assert_eq!(bytes, [0xEF, 0xBB, 0xBF, b'x']);
        assert_eq!(DecodedText::of(&bytes).unwrap().text, "x");
    }

    #[test]
    fn a_binary_file_is_refused_rather_than_assigned_an_encoding() {
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x08";

        assert!(DecodedText::of(png).is_none());
    }

    #[test]
    fn utf16_without_a_bom_is_read_rather_than_condemned_by_the_binary_guard() {
        for (encoding, order) in [
            (
                FileEncoding::Utf16Le,
                u16::to_le_bytes as fn(u16) -> [u8; 2],
            ),
            (FileEncoding::Utf16Be, u16::to_be_bytes),
        ] {
            for text in [
                "hello, world\r\n",
                "日本語のテキスト\n",
                "中文和 english 混在\r\n",
            ] {
                let bytes = Utf16::encoded(text, order);
                assert!(
                    BinarySniff::rejects(&bytes),
                    "{text:?} must be one the guard would have refused"
                );

                let out = DecodedText::of(&bytes).expect("bom-less utf-16 is text");

                assert_eq!(out.encoding, encoding, "{text:?} as {}", encoding.label());
                assert_eq!(out.text, text, "{text:?} as {}", encoding.label());
            }
        }
    }

    #[test]
    fn sniffing_for_utf16_does_not_let_a_real_binary_through() {
        let cases: [(&str, &[u8]); 4] = [
            (
                "png",
                b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x08\x00\x00\x00\x08\x08\x06",
            ),
            (
                "zip",
                b"PK\x03\x04\x14\x00\x00\x00\x08\x00\x8d\x7fUZ\x00\x00\x00\x00\x00\x00\x00\x00",
            ),
            (
                "mach-o",
                b"\xcf\xfa\xed\xfe\x07\x00\x00\x01\x03\x00\x00\x00\x02\x00\x00\x00\x10\x00\x00\x00",
            ),
            (
                "elf",
                b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00>\x00\x01\x00\x00\x00",
            ),
        ];

        for (name, bytes) in cases {
            assert!(DecodedText::of(bytes).is_none(), "{name} is not text");
        }
    }

    #[test]
    fn utf8_text_carrying_a_nul_is_refused_rather_than_forced_into_utf16() {
        let bytes = b"log line\x00\x00\x00 more text\n";

        assert!(DecodedText::of(bytes).is_none());
    }

    #[test]
    fn a_legacy_japanese_encoding_is_not_stolen_by_the_utf16_sniff() {
        for encoding in [
            FileEncoding::ShiftJis,
            FileEncoding::EucJp,
            FileEncoding::Gbk,
            FileEncoding::Big5,
        ] {
            let text = match encoding {
                FileEncoding::Gbk => "中文文本，这里有很多汉字。\n",
                FileEncoding::Big5 => "繁體中文，這裡有很多漢字。\n",
                _ => "吾輩は猫である。名前はまだ無い。\n",
            };
            let bytes = Reencode::bytes(text, encoding);

            let out = DecodedText::of(&bytes).expect("legacy text stays text");

            assert_eq!(out.encoding, encoding, "{}", encoding.label());
            assert_eq!(out.text, text, "{}", encoding.label());
        }
    }

    #[test]
    fn plain_ascii_is_utf8_rather_than_whatever_the_detector_prefers() {
        let out = DecodedText::of(b"fn main() {}\n").expect("ascii is text");

        assert_eq!(out.encoding, FileEncoding::Utf8);
        assert_eq!(out.text, "fn main() {}\n");
    }

    #[test]
    fn utf8_japanese_without_a_bom_is_not_mistaken_for_a_legacy_encoding() {
        let out = DecodedText::of("日本語のテキスト\n".as_bytes()).expect("utf-8 is text");

        assert_eq!(out.encoding, FileEncoding::Utf8);
        assert_eq!(out.text, "日本語のテキスト\n");
    }

    #[test]
    fn every_offered_encoding_round_trips_its_own_output() {
        for encoding in FileEncoding::ALL {
            let text = match encoding {
                FileEncoding::Gbk => "中文文本\n",
                FileEncoding::Big5 => "繁體中文\n",
                FileEncoding::EucKr => "한국어\n",
                FileEncoding::ShiftJis | FileEncoding::EucJp | FileEncoding::Iso2022Jp => {
                    "日本語\n"
                }
                FileEncoding::Windows1252 | FileEncoding::Iso8859_1 => "café\n",
                _ => "héllo 日本語\n",
            };
            let bytes = Reencode::bytes(text, encoding);

            let back = DecodedText::forced(&bytes, encoding);

            assert_eq!(back.text, text, "{} round trip", encoding.label());
        }
    }

    #[test]
    fn a_character_the_target_cannot_hold_refuses_the_encode() {
        let err = Reencode {
            encoding: FileEncoding::ShiftJis,
        }
        .applied("price: 10€\n")
        .unwrap_err();

        assert_eq!(err.ch, '€');
        assert_eq!(err.encoding, FileEncoding::ShiftJis);
        assert!(err.to_string().contains("Shift_JIS"), "got {err}");
    }

    #[test]
    fn latin1_refuses_a_character_above_its_range() {
        let err = Reencode {
            encoding: FileEncoding::Iso8859_1,
        }
        .applied("日")
        .unwrap_err();

        assert_eq!(err.ch, '日');
    }

    #[test]
    fn latin1_is_not_silently_treated_as_windows_1252() {
        let bytes = [0x93u8, 0x94];

        assert_eq!(
            DecodedText::forced(&bytes, FileEncoding::Iso8859_1).text,
            "\u{93}\u{94}"
        );
        assert_eq!(
            DecodedText::forced(&bytes, FileEncoding::Windows1252).text,
            "\u{201C}\u{201D}"
        );
    }

    #[test]
    fn utf16_is_encoded_as_utf16_rather_than_falling_back_to_utf8() {
        let bytes = Reencode::bytes("ab", FileEncoding::Utf16Le);

        assert_eq!(bytes, [0xFF, 0xFE, b'a', 0x00, b'b', 0x00]);
    }

    #[test]
    fn an_astral_character_survives_a_utf16_round_trip() {
        for encoding in [FileEncoding::Utf16Le, FileEncoding::Utf16Be] {
            let bytes = Reencode::bytes("go 🚀\n", encoding);

            assert_eq!(DecodedText::of(&bytes).unwrap().text, "go 🚀\n");
        }
    }
}
