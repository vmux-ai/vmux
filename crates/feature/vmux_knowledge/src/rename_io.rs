use std::io;
use std::path::Path;

pub(crate) struct RenameIo;

impl RenameIo {
    pub(crate) fn apply(
        source: &Path,
        replacements: &[(usize, usize, &str, &str)],
    ) -> io::Result<()> {
        let mut text = std::fs::read_to_string(source)?;
        let mut ordered: Vec<(usize, usize, &str, &str)> = replacements.to_vec();
        ordered.sort_by_key(|replacement| std::cmp::Reverse(replacement.0));
        for (start, end, expected, replacement) in ordered {
            if text.get(start..end) != Some(expected) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Knowledge note changed before its link rewrite",
                ));
            }
            text.replace_range(start..end, replacement);
        }
        std::fs::write(source, text)
    }
}
