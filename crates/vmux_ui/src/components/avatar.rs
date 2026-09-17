use crate::util::cn;
use dioxus::prelude::*;

const AVATAR: &str = "inline-flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-[var(--avatar-background)] font-semibold text-white";
const GENERATED_AVATAR_COLORS: [&str; 6] = [
    "#fda4af", "#fdba74", "#fde68a", "#86efac", "#67e8f9", "#c4b5fd",
];

#[component]
pub fn Avatar(
    src: Option<String>,
    seed: String,
    background: String,
    #[props(default)] alt: String,
    #[props(default)] class: String,
) -> Element {
    let class = cn([AVATAR, class.as_str()]);
    let avatar_seed = if seed.trim().is_empty() { &alt } else { &seed };
    let generated = GeneratedAvatar::of(avatar_seed, &background);
    rsx! {
        div {
            class,
            style: "--avatar-background:{background}",
            if let Some(src) = src {
                img { class: "size-full object-cover", src, alt }
            } else {
                svg {
                    class: "size-full",
                    view_box: "0 0 36 36",
                    role: "img",
                    title { "{alt}" }
                    rect { width: "36", height: "36", fill: generated.background_color }
                    rect {
                        width: "36",
                        height: "36",
                        rx: generated.shape_radius,
                        fill: generated.shape_color,
                        transform: generated.shape_transform,
                    }
                    g { transform: generated.face_transform,
                        if generated.smiles {
                            path {
                                d: generated.mouth_path,
                                fill: "none",
                                stroke: generated.face_color,
                                stroke_width: "1.5",
                                stroke_linecap: "round",
                            }
                        } else {
                            path { d: generated.mouth_path, fill: generated.face_color }
                        }
                        rect {
                            x: generated.left_eye_x,
                            y: "14",
                            width: "1.75",
                            height: "2.25",
                            rx: "1",
                            fill: generated.face_color,
                        }
                        rect {
                            x: generated.right_eye_x,
                            y: "14",
                            width: "1.75",
                            height: "2.25",
                            rx: "1",
                            fill: generated.face_color,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct GeneratedAvatar {
    background_color: &'static str,
    shape_color: String,
    face_color: &'static str,
    shape_radius: usize,
    shape_transform: String,
    face_transform: String,
    smiles: bool,
    mouth_path: String,
    left_eye_x: usize,
    right_eye_x: usize,
}

impl GeneratedAvatar {
    fn of(seed: &str, shape_color: &str) -> Self {
        let mut random = AvatarRandom::of(seed);
        let background_color = GENERATED_AVATAR_COLORS[random.below(GENERATED_AVATAR_COLORS.len())];
        let translate_x = 3 + random.below(8);
        let translate_y = 3 + random.below(8);
        let rotate = random.below(360);
        let scale = 100 + random.below(21);
        let shape_radius = if random.yes() { 18 } else { 6 };
        let smiles = random.yes();
        let eye_spread = random.below(5);
        let mouth_spread = random.below(4);
        let face_rotate = random.below(11) as isize - 5;
        let face_translate_x = if translate_x > 6 {
            translate_x / 2
        } else {
            random.below(8)
        };
        let face_translate_y = if translate_y > 6 {
            translate_y / 2
        } else {
            random.below(7)
        };
        let mouth_y = 19 + mouth_spread;
        let mouth_path = if smiles {
            format!("M14 {mouth_y} Q18 {} 22 {mouth_y}", mouth_y + 3)
        } else {
            format!("M13 {mouth_y} a1 0.75 0 0 0 10 0")
        };

        Self {
            background_color,
            shape_color: shape_color.to_string(),
            face_color: Self::contrast_color(shape_color),
            shape_radius,
            shape_transform: format!(
                "translate({translate_x} {translate_y}) rotate({rotate} 18 18) scale({}.{:02})",
                scale / 100,
                scale % 100
            ),
            face_transform: format!(
                "translate({face_translate_x} {face_translate_y}) rotate({face_rotate} 18 18)"
            ),
            smiles,
            mouth_path,
            left_eye_x: 14 - eye_spread,
            right_eye_x: 20 + eye_spread,
        }
    }

    fn contrast_color(color: &str) -> &'static str {
        let value = color.strip_prefix('#').unwrap_or(color);
        if value.len() != 6 || !value.is_ascii() {
            return "#fafafa";
        }
        let Ok(red) = u8::from_str_radix(&value[0..2], 16) else {
            return "#fafafa";
        };
        let Ok(green) = u8::from_str_radix(&value[2..4], 16) else {
            return "#fafafa";
        };
        let Ok(blue) = u8::from_str_radix(&value[4..6], 16) else {
            return "#fafafa";
        };
        let luminance = u32::from(red) * 299 + u32::from(green) * 587 + u32::from(blue) * 114;
        if luminance > 150_000 {
            "#18181b"
        } else {
            "#fafafa"
        }
    }
}

struct AvatarRandom(u64);

impl AvatarRandom {
    fn of(seed: &str) -> Self {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in seed.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Self(hash)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, ceiling: usize) -> usize {
        self.next() as usize % ceiling
    }

    fn yes(&mut self) -> bool {
        self.below(2) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::GeneratedAvatar;

    #[test]
    fn generated_avatar_is_stable_for_a_profile() {
        assert_eq!(
            GeneratedAvatar::of("Personal", "#3b82f6"),
            GeneratedAvatar::of("Personal", "#3b82f6")
        );
    }

    #[test]
    fn different_profiles_get_different_faces() {
        assert_ne!(
            GeneratedAvatar::of("Personal", "#3b82f6"),
            GeneratedAvatar::of("Work", "#3b82f6")
        );
    }

    #[test]
    fn face_color_contrasts_with_the_profile_color() {
        assert_eq!(GeneratedAvatar::contrast_color("#ffffff"), "#18181b");
        assert_eq!(GeneratedAvatar::contrast_color("#000000"), "#fafafa");
    }
}
