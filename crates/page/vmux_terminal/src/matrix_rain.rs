use dioxus::prelude::*;

const FONT_PX: f64 = 16.0;
const COLUMNS: usize = 120;
const COLUMN_GLYPHS: usize = 28;
const GLYPHS: &str = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789";

#[component]
pub fn MatrixRain(accent_rgb: String, words: Vec<String>) -> Element {
    let head = format!(
        "light-dark(rgb({}), {})",
        Accent::darkened(&accent_rgb, 42),
        Accent::brightened(&accent_rgb)
    );
    let trail = format!(
        "light-dark(rgb({} / 0.55), rgb({} / 0.5))",
        Accent::darkened(&accent_rgb, 55),
        accent_rgb
    );
    let words: Vec<Vec<char>> = words
        .iter()
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect();

    rsx! {
        div {
            class: "absolute inset-0 overflow-hidden",
            style: "font:{FONT_PX}px monospace;line-height:{FONT_PX}px;color:{trail};",

            style { {RainColumn::KEYFRAMES} }

            for index in 0..COLUMNS {
                {
                    let column = RainColumn::at(index, &words);
                    rsx! {
                        div {
                            key: "{index}",
                            class: "absolute top-0 whitespace-pre motion-reduce:animate-none",
                            style: "{column.style}",
                            "{column.trail}"
                            span { style: "color:{head};", "{column.head}" }
                        }
                    }
                }
            }
        }
    }
}

struct RainColumn {
    trail: String,
    head: char,
    style: String,
}

impl RainColumn {
    const KEYFRAMES: &'static str =
        "@keyframes vmux-rain{from{transform:translateY(-100%)}to{transform:translateY(100vh)}}";

    fn at(index: usize, words: &[Vec<char>]) -> Self {
        let glyphs: Vec<char> = GLYPHS.chars().collect();
        let word = (!words.is_empty() && index % 7 == 3).then(|| &words[index % words.len()]);

        let mut column = String::new();
        for row in 0..COLUMN_GLYPHS {
            let glyph = match word {
                Some(word) => word[row % word.len()],
                None => glyphs[Self::noise(index * 97 + row) as usize % glyphs.len()],
            };
            column.push(glyph);
            column.push('\n');
        }
        let head = column.pop().map(|_| column.pop()).unwrap_or_default();

        let seconds = 3.0 + (Self::noise(index) % 1000) as f64 / 200.0;
        let delay = (Self::noise(index * 31) % 1000) as f64 / 160.0;
        Self {
            trail: column,
            head: head.unwrap_or(' '),
            style: format!(
                "left:{}px;animation:vmux-rain {seconds:.2}s linear {delay:.2}s infinite;\
                 mask-image:linear-gradient(to bottom,transparent,#000 60%,#000 100%);",
                index as f64 * FONT_PX
            ),
        }
    }

    fn noise(seed: usize) -> u64 {
        let mut x = (seed as u64)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51afd7ed558ccd);
        x ^ (x >> 33)
    }
}

struct Accent;

impl Accent {
    fn brightened(accent_rgb: &str) -> String {
        let Some([r, g, b]) = Self::parse(accent_rgb) else {
            return "rgb(220 230 255)".to_string();
        };
        let mix = |c: u16| c + (255 - c) * 7 / 10;
        format!("rgb({} {} {})", mix(r), mix(g), mix(b))
    }

    fn darkened(accent_rgb: &str, pct: u16) -> String {
        let Some([r, g, b]) = Self::parse(accent_rgb) else {
            return "20 24 33".to_string();
        };
        let mix = |c: u16| c * pct / 100;
        format!("{} {} {}", mix(r), mix(g), mix(b))
    }

    fn parse(accent_rgb: &str) -> Option<[u16; 3]> {
        let mut parts = accent_rgb.split_whitespace();
        let mut channel = || parts.next()?.parse::<u16>().ok();
        let rgb = [channel()?, channel()?, channel()?];
        parts.next().is_none().then_some(rgb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_woven_column_reads_as_the_word_it_was_given() {
        let words = vec!["CLAUDE".chars().collect::<Vec<_>>()];
        let column = RainColumn::at(3, &words);

        let shown: String = column
            .trail
            .chars()
            .filter(|c| *c != '\n')
            .chain(std::iter::once(column.head))
            .collect();
        assert_eq!(shown.chars().count(), COLUMN_GLYPHS);
        assert!(shown.starts_with("CLAUDE"), "got {shown}");
    }

    #[test]
    fn adjacent_columns_do_not_share_a_fall() {
        let first = RainColumn::at(10, &[]);
        let second = RainColumn::at(11, &[]);

        assert_ne!(first.style, second.style);
        assert_ne!(first.trail, second.trail);
    }

    #[test]
    fn a_malformed_accent_falls_back_rather_than_producing_broken_css() {
        assert_eq!(Accent::parse("1 2"), None);
        assert_eq!(Accent::parse("1 2 3 4"), None);
        assert_eq!(Accent::parse("no such colour"), None);
        assert_eq!(Accent::brightened("oops"), "rgb(220 230 255)");
        assert_eq!(Accent::darkened("oops", 42), "20 24 33");
    }
}
