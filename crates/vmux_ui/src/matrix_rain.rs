use dioxus::prelude::*;

use crate::util::cn;

const DEFAULT_COLUMNS: usize = 120;
const COLUMN_GLYPHS: usize = 96;
const GLYPHS: &str = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789";

#[component]
pub fn MatrixRain(
    accent_rgb: String,
    words: Vec<String>,
    #[props(default = DEFAULT_COLUMNS)] columns: usize,
) -> Element {
    let accent = Accent::css(&accent_rgb);
    let head = format!(
        "light-dark(color-mix(in oklab, {accent} 72%, black), color-mix(in oklab, {accent} 68%, white))"
    );
    let trail = format!("color-mix(in oklab, {accent} 52%, transparent)");
    let words: Vec<Vec<char>> = words
        .iter()
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect();

    rsx! {
        div {
            class: "absolute inset-0 overflow-hidden font-mono text-[16px] leading-[16px]",
            "aria-hidden": "true",
            style: "--vmux-rain-head:{head};--vmux-rain-trail:{trail};",

            for index in 0..columns {
                {
                    let column = RainColumn::at(index, &words);
                    rsx! {
                        div {
                            key: "{index}",
                            class: "absolute top-0 whitespace-pre",
                            style: "{column.style(columns)}",
                            div {
                                class: "absolute left-0 top-0 text-[var(--vmux-rain-trail)] [-webkit-mask-image:linear-gradient(to_bottom,transparent_0%,rgb(0_0_0/.08)_18%,rgb(0_0_0/.4)_65%,#000_100%)] [-webkit-mask-repeat:no-repeat] [-webkit-mask-size:100%_320px] [mask-image:linear-gradient(to_bottom,transparent_0%,rgb(0_0_0/.08)_18%,rgb(0_0_0/.4)_65%,#000_100%)] [mask-repeat:no-repeat] [mask-size:100%_320px] motion-reduce:!animate-none motion-reduce:opacity-[0.08] motion-reduce:[-webkit-mask-image:none] motion-reduce:[mask-image:none]",
                                style: "{column.animation_style()}",
                                "{column.glyphs}"
                            }
                            div {
                                class: "absolute left-0 top-0 text-[var(--vmux-rain-head)] [text-shadow:0_0_8px_var(--vmux-rain-head)] [-webkit-mask-image:linear-gradient(to_bottom,transparent_0%,transparent_88%,#000_96%,transparent_100%)] [-webkit-mask-repeat:no-repeat] [-webkit-mask-size:100%_320px] [mask-image:linear-gradient(to_bottom,transparent_0%,transparent_88%,#000_96%,transparent_100%)] [mask-repeat:no-repeat] [mask-size:100%_320px] motion-reduce:hidden",
                                style: "{column.animation_style()}",
                                "{column.glyphs}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MatrixLoader(
    label: String,
    #[props(default)] words: Vec<String>,
    #[props(default = "h-full w-full".to_string())] class: String,
) -> Element {
    let words = if words.is_empty() {
        vec!["VMUX".to_string()]
    } else {
        words
    };

    rsx! {
        div {
            class: cn(["overflow-hidden", class.as_str()]),
            role: "status",
            "aria-busy": "true",
            div { class: "relative h-full w-full overflow-hidden bg-background",
                MatrixRain {
                    accent_rgb: "var(--primary)".to_string(),
                    words,
                }
                div { class: "relative z-10 flex h-full w-full items-center justify-center",
                    div { class: "glass max-w-[min(28rem,calc(100%-3rem))] rounded-2xl px-5 py-3 text-center text-sm font-medium text-foreground ring-1 ring-inset ring-border/70 backdrop-blur-xl",
                        "{label}"
                    }
                }
            }
        }
    }
}

struct RainColumn {
    index: usize,
    glyphs: String,
    duration_seconds: f64,
    delay_seconds: f64,
}

impl RainColumn {
    fn at(index: usize, words: &[Vec<char>]) -> Self {
        let glyphs: Vec<char> = GLYPHS.chars().collect();
        let word = (!words.is_empty() && index % 7 == 3).then(|| &words[index % words.len()]);
        let mut column_glyphs = String::with_capacity(COLUMN_GLYPHS * 2);
        for row in 0..COLUMN_GLYPHS {
            if row > 0 {
                column_glyphs.push('\n');
            }
            let character = match word {
                Some(word) => word[row % word.len()],
                None => glyphs[Self::noise(index * 97 + row) as usize % glyphs.len()],
            };
            column_glyphs.push(character);
        }
        let duration_seconds = 5.0 + (Self::noise(index * 17) % 4500) as f64 / 1000.0;
        let delay_seconds =
            -((Self::noise(index * 31) % 10_000) as f64 / 10_000.0 * duration_seconds);
        Self {
            index,
            glyphs: column_glyphs,
            duration_seconds,
            delay_seconds,
        }
    }

    fn style(&self, columns: usize) -> String {
        let columns = columns.max(1);
        format!("left:{:.4}%;", self.index as f64 * 100.0 / columns as f64)
    }

    fn animation_style(&self) -> String {
        format!(
            "animation:vmux-rain-window {:.3}s linear {:.3}s infinite both;",
            self.duration_seconds, self.delay_seconds
        )
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
    fn css(accent: &str) -> String {
        if accent == "var(--primary)" {
            return "var(--primary)".to_string();
        };
        let channels = accent
            .split_whitespace()
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>();
        match channels.as_deref() {
            Ok([red, green, blue]) => format!("rgb({red} {green} {blue})"),
            _ => "var(--primary)".to_string(),
        }
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
            .glyphs
            .chars()
            .filter(|glyph| *glyph != '\n')
            .collect();
        assert_eq!(shown.chars().count(), COLUMN_GLYPHS);
        assert!(shown.starts_with("CLAUDE"), "got {shown}");
    }

    #[test]
    fn adjacent_columns_do_not_share_a_fall() {
        let first = RainColumn::at(10, &[]);
        let second = RainColumn::at(11, &[]);

        assert_ne!(first.animation_style(), second.animation_style());
        assert_ne!(first.glyphs, second.glyphs);
    }

    #[test]
    fn accent_accepts_rgb_channels_and_the_primary_theme_token() {
        assert_eq!(Accent::css("251 146 60"), "rgb(251 146 60)");
        assert_eq!(Accent::css("var(--primary)"), "var(--primary)");
        assert_eq!(Accent::css("var(--primary);color:red"), "var(--primary)");
        assert_eq!(Accent::css("oops"), "var(--primary)");
    }
}
