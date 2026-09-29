use unicode_width::UnicodeWidthChar;

struct DisplayCellWidth(u32);

impl From<char> for DisplayCellWidth {
    fn from(character: char) -> Self {
        Self(UnicodeWidthChar::width(character).unwrap_or(0) as u32)
    }
}

impl DisplayCellWidth {
    fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DisplayCells<'a> {
    text: &'a str,
}

impl<'a> From<&'a str> for DisplayCells<'a> {
    fn from(text: &'a str) -> Self {
        Self { text }
    }
}

impl DisplayCells<'_> {
    pub(crate) fn width(self) -> u32 {
        let mut cells = 0;
        for character in self.text.chars() {
            cells += Self::width_of(character);
        }
        cells
    }

    pub(crate) fn char_at(self, cell: u32) -> usize {
        let mut cells = 0;
        for (index, character) in self.text.chars().enumerate() {
            if cells >= cell {
                return index;
            }
            let width = Self::width_of(character);
            if cells + width > cell {
                return index;
            }
            cells += width;
        }
        self.text.chars().count()
    }

    pub(crate) fn width_of(character: char) -> u32 {
        DisplayCellWidth::from(character).get()
    }
}
