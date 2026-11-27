use std::path::Path;
use std::time::Duration;

use crate::policy::GIB;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    fn lerp(self, other: Self, num: usize, den: usize) -> Self {
        if den == 0 {
            return self;
        }
        let mix = |a: u8, b: u8| -> u8 {
            let a = a as i32;
            let b = b as i32;
            (a + ((b - a) * num as i32) / den as i32).clamp(0, 255) as u8
        };
        Self(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }

    fn lift(self, fg: Self, amount: usize) -> Self {
        self.lerp(fg, amount, 100)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Palette {
    pub fg: Rgb,
    pub muted: Rgb,
    pub surface: Rgb,
    pub primary: Rgb,
    pub accent: Rgb,
    pub gold: Rgb,
    pub green: Rgb,
    pub red: Rgb,
}

impl Palette {
    fn from_desktop(colors: [[u8; 3]; 8]) -> Self {
        let rgb = |c: [u8; 3]| Rgb(c[0], c[1], c[2]);
        let bg = rgb(colors[0]);
        let fg = rgb(colors[1]);
        Self {
            fg,
            muted: bg.lift(fg, 44),
            surface: bg.lift(fg, 12),
            primary: rgb(colors[2]),
            accent: rgb(colors[3]),
            gold: rgb(colors[4]),
            green: rgb(colors[5]),
            red: rgb(colors[6]),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ThemePreset {
    #[default]
    CatppuccinMocha,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    TokyoNight,
    Dracula,
    RosePine,
    Kanagawa,
    Nord,
    GruvboxDark,
    OneDark,
    GithubDark,
    GithubLight,
    SolarizedDark,
    SolarizedLight,
}

impl ThemePreset {
    pub(crate) const ALL: [Self; 15] = [
        Self::CatppuccinMocha,
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::TokyoNight,
        Self::Dracula,
        Self::RosePine,
        Self::Kanagawa,
        Self::Nord,
        Self::GruvboxDark,
        Self::OneDark,
        Self::GithubDark,
        Self::GithubLight,
        Self::SolarizedDark,
        Self::SolarizedLight,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::CatppuccinLatte => "catppuccin-latte",
            Self::CatppuccinFrappe => "catppuccin-frappe",
            Self::CatppuccinMacchiato => "catppuccin-macchiato",
            Self::TokyoNight => "tokyo-night",
            Self::Dracula => "dracula",
            Self::RosePine => "rose-pine",
            Self::Kanagawa => "kanagawa",
            Self::Nord => "nord",
            Self::GruvboxDark => "gruvbox-dark",
            Self::OneDark => "one-dark",
            Self::GithubDark => "github-dark",
            Self::GithubLight => "github-light",
            Self::SolarizedDark => "solarized-dark",
            Self::SolarizedLight => "solarized-light",
        }
    }

    pub(crate) fn parse(name: &str) -> Option<Self> {
        let normalized = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|preset| preset.id() == normalized)
    }

    pub(crate) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|preset| *preset == self)
            .unwrap_or(0)
    }

    pub(crate) fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub(crate) fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub(crate) fn palette(self) -> Palette {
        match self {
            Self::CatppuccinMocha
            | Self::CatppuccinLatte
            | Self::CatppuccinFrappe
            | Self::CatppuccinMacchiato => self.catppuccin_palette(),
            Self::TokyoNight
            | Self::Dracula
            | Self::RosePine
            | Self::Kanagawa
            | Self::Nord
            | Self::GruvboxDark => self.vivid_palette(),
            Self::OneDark
            | Self::GithubDark
            | Self::GithubLight
            | Self::SolarizedDark
            | Self::SolarizedLight => self.classic_palette(),
        }
    }

    fn catppuccin_palette(self) -> Palette {
        match self {
            Self::CatppuccinMocha => Palette::from_desktop([
                [30, 30, 46],
                [205, 214, 244],
                [137, 180, 250],
                [245, 194, 231],
                [249, 226, 175],
                [166, 227, 161],
                [243, 139, 168],
                [148, 226, 213],
            ]),
            Self::CatppuccinLatte => Palette::from_desktop([
                [239, 241, 245],
                [76, 79, 105],
                [30, 102, 245],
                [234, 118, 203],
                [223, 142, 29],
                [64, 160, 43],
                [210, 15, 57],
                [4, 165, 229],
            ]),
            Self::CatppuccinFrappe => Palette::from_desktop([
                [48, 52, 70],
                [198, 208, 245],
                [140, 170, 238],
                [244, 184, 228],
                [229, 200, 144],
                [166, 209, 137],
                [231, 130, 132],
                [129, 200, 190],
            ]),
            _ => Palette::from_desktop([
                [36, 39, 58],
                [202, 211, 245],
                [138, 173, 244],
                [245, 189, 230],
                [238, 212, 159],
                [166, 218, 149],
                [237, 135, 150],
                [139, 213, 202],
            ]),
        }
    }

    fn vivid_palette(self) -> Palette {
        match self {
            Self::TokyoNight => Palette::from_desktop([
                [26, 27, 38],
                [192, 202, 245],
                [122, 162, 247],
                [187, 154, 247],
                [224, 175, 104],
                [158, 206, 106],
                [247, 118, 142],
                [125, 207, 255],
            ]),
            Self::Dracula => Palette::from_desktop([
                [40, 42, 54],
                [248, 248, 242],
                [189, 147, 249],
                [255, 121, 198],
                [241, 250, 140],
                [80, 250, 123],
                [255, 85, 85],
                [139, 233, 253],
            ]),
            Self::RosePine => Palette::from_desktop([
                [25, 23, 36],
                [224, 222, 244],
                [196, 167, 231],
                [235, 111, 146],
                [246, 193, 119],
                [156, 207, 216],
                [235, 111, 146],
                [156, 207, 216],
            ]),
            Self::Kanagawa => Palette::from_desktop([
                [31, 31, 40],
                [220, 215, 186],
                [126, 156, 216],
                [149, 127, 184],
                [224, 195, 117],
                [135, 169, 117],
                [231, 115, 118],
                [127, 180, 202],
            ]),
            Self::Nord => Palette::from_desktop([
                [46, 52, 64],
                [216, 222, 233],
                [136, 192, 208],
                [180, 142, 173],
                [235, 203, 139],
                [163, 190, 140],
                [191, 97, 106],
                [129, 161, 193],
            ]),
            _ => Palette::from_desktop([
                [40, 40, 40],
                [235, 219, 178],
                [142, 192, 124],
                [211, 134, 155],
                [250, 189, 47],
                [184, 187, 38],
                [251, 73, 52],
                [131, 165, 152],
            ]),
        }
    }

    fn classic_palette(self) -> Palette {
        match self {
            Self::OneDark => Palette::from_desktop([
                [40, 44, 52],
                [171, 178, 191],
                [97, 175, 239],
                [198, 120, 221],
                [229, 192, 123],
                [152, 195, 121],
                [224, 108, 117],
                [86, 182, 194],
            ]),
            Self::GithubDark => Palette::from_desktop([
                [13, 17, 23],
                [230, 237, 243],
                [88, 166, 255],
                [210, 168, 255],
                [227, 179, 65],
                [86, 211, 100],
                [255, 123, 114],
                [86, 212, 221],
            ]),
            Self::GithubLight => Palette::from_desktop([
                [255, 255, 255],
                [31, 35, 40],
                [9, 105, 218],
                [130, 80, 223],
                [154, 103, 0],
                [26, 127, 55],
                [207, 34, 46],
                [27, 124, 131],
            ]),
            Self::SolarizedDark => Palette::from_desktop([
                [0, 43, 54],
                [147, 161, 161],
                [38, 139, 210],
                [211, 54, 130],
                [181, 137, 0],
                [133, 153, 0],
                [220, 50, 47],
                [42, 161, 152],
            ]),
            _ => Palette::from_desktop([
                [253, 246, 227],
                [101, 123, 131],
                [38, 139, 210],
                [211, 54, 130],
                [181, 137, 0],
                [133, 153, 0],
                [220, 50, 47],
                [42, 161, 152],
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Theme {
    pub preset: ThemePreset,
    pub color: bool,
    pub width: u16,
}

impl Theme {
    pub(crate) fn new(preset: ThemePreset, color: bool, width: u16) -> Self {
        Self {
            preset,
            color,
            width: width.clamp(56, 240),
        }
    }

    pub(crate) fn palette(self) -> Palette {
        self.preset.palette()
    }

    pub(crate) fn card_width(self) -> usize {
        usize::from(self.width).clamp(56, 88)
    }

    pub(crate) fn fg(self, rgb: Rgb, text: &str) -> String {
        if self.color {
            format!("\x1b[38;2;{};{};{}m{text}\x1b[0m", rgb.0, rgb.1, rgb.2)
        } else {
            text.to_owned()
        }
    }

    pub(crate) fn fg_bold(self, rgb: Rgb, text: &str) -> String {
        if self.color {
            format!("\x1b[1;38;2;{};{};{}m{text}\x1b[0m", rgb.0, rgb.1, rgb.2)
        } else {
            text.to_owned()
        }
    }

    pub(crate) fn surface_row(self, text: &str, width: usize) -> String {
        let trimmed = Self::truncate_ellipsis(text, width);
        let pad = width.saturating_sub(Self::display_width(&trimmed));
        let p = self.palette();
        if self.color {
            let bg = format!("\x1b[48;2;{};{};{}m", p.surface.0, p.surface.1, p.surface.2);
            let body = trimmed.replace("\x1b[0m", &format!("\x1b[0m{bg}"));
            format!("{bg}{body}{}\x1b[0m", " ".repeat(pad))
        } else {
            format!("{trimmed}{}", " ".repeat(pad))
        }
    }

    pub(crate) fn bold(self, text: &str) -> String {
        self.fg_bold(self.palette().fg, text)
    }

    pub(crate) fn text(self, text: &str) -> String {
        self.fg(self.palette().fg, text)
    }

    pub(crate) fn muted(self, text: &str) -> String {
        self.fg(self.palette().muted, text)
    }

    pub(crate) fn primary(self, text: &str) -> String {
        self.fg(self.palette().primary, text)
    }

    pub(crate) fn accent(self, text: &str) -> String {
        self.fg(self.palette().accent, text)
    }

    pub(crate) fn gold(self, text: &str) -> String {
        self.fg(self.palette().gold, text)
    }

    pub(crate) fn green(self, text: &str) -> String {
        self.fg(self.palette().green, text)
    }

    pub(crate) fn red(self, text: &str) -> String {
        self.fg(self.palette().red, text)
    }

    pub(crate) fn swatches(self, preset: ThemePreset) -> String {
        let p = preset.palette();
        [p.primary, p.accent, p.green]
            .into_iter()
            .map(|rgb| self.fg(rgb, "●"))
            .collect::<Vec<_>>()
            .join("")
    }

    pub(crate) fn pane_box(
        self,
        title: &str,
        badge: &str,
        focused: bool,
        width: usize,
        height: usize,
        rows: &[String],
    ) -> Vec<String> {
        let w = width.max(18);
        let inner = w.saturating_sub(2);
        let p = self.palette();
        let border_rgb = if focused { p.primary } else { p.muted };
        let title_styled = if focused {
            self.fg_bold(p.fg, title)
        } else {
            self.muted(title)
        };
        let badge_styled = if focused {
            self.fg(p.primary, badge)
        } else {
            self.muted(badge)
        };
        let header_content = if badge.is_empty() {
            format!(" {title_styled} ")
        } else {
            format!(" {badge_styled} {title_styled} ")
        };
        let header_w = Self::display_width(&header_content);
        let fill = w.saturating_sub(3 + header_w);
        let top = format!(
            "{}{header_content}{}",
            self.fg(border_rgb, "╭─"),
            self.fg(border_rgb, &format!("{}╮", "─".repeat(fill)))
        );
        let body_rows = height.saturating_sub(2);
        let mut out = Vec::with_capacity(height);
        out.push(top);
        for index in 0..body_rows {
            let line = rows.get(index).map(String::as_str).unwrap_or("");
            let fitted = Self::truncate_ellipsis(line, inner);
            let pad = inner.saturating_sub(Self::display_width(&fitted));
            out.push(format!(
                "{}{fitted}{}{}",
                self.fg(border_rgb, "│"),
                " ".repeat(pad),
                self.fg(border_rgb, "│")
            ));
        }
        out.push(self.fg(border_rgb, &format!("╰{}╯", "─".repeat(inner))));
        out
    }

    pub(crate) fn card(self, title: &str, title_rgb: Rgb, lines: &[String]) -> String {
        let total = self.card_width();
        let inner = total.saturating_sub(4);
        let title_plain = format!(" {title} ");
        let fill = total.saturating_sub(3 + Self::display_width(&title_plain));
        let mut out = String::new();
        out.push_str(&self.muted("╭─"));
        out.push_str(&self.fg_bold(title_rgb, &title_plain));
        out.push_str(&self.muted(&format!("{}╮\n", "─".repeat(fill))));
        for line in lines {
            let trimmed = Self::truncate_ellipsis(line, inner);
            let pad = inner.saturating_sub(Self::display_width(&trimmed));
            out.push_str(&self.muted("│"));
            out.push(' ');
            out.push_str(&trimmed);
            out.push_str(&" ".repeat(pad));
            out.push(' ');
            out.push_str(&self.muted("│"));
            out.push('\n');
        }
        out.push_str(&self.muted(&format!("╰{}╯\n", "─".repeat(total.saturating_sub(2)))));
        out
    }

    pub(crate) fn header_card(
        self,
        command: &str,
        branch: Option<&str>,
        profile: Option<&str>,
    ) -> String {
        let mut meta = Vec::new();
        if let Some(branch) = branch {
            meta.push(self.text(branch));
        }
        if let Some(profile) = profile {
            meta.push(self.muted(profile));
        }
        let suffix = if meta.is_empty() {
            String::new()
        } else {
            format!("  {}", meta.join(&self.muted(" · ")))
        };
        format!(
            "{}  {}{suffix}\n",
            self.primary("┌"),
            self.bold(&format!("vmux {command}"))
        )
    }

    pub(crate) fn active_line(
        self,
        frame: usize,
        completed: usize,
        total: usize,
        step_name: &str,
        elapsed: Duration,
        detail: Option<&str>,
    ) -> String {
        let spinner = self.primary(Self::spinner(frame));
        let bar = self.gradient_bar(completed, total, 12, frame);
        let label = self.bold(step_name);
        let timer = self.muted(&Self::format_duration(elapsed));
        let hint = match detail {
            Some(text) if !text.is_empty() => format!("  {}", self.muted(text)),
            _ => format!("  {}", self.muted(Mascot::from_step_name(step_name).quip())),
        };
        let count = if total > 0 {
            let current = (completed + 1).min(total);
            format!(" {}", self.muted(&format!("{current}/{total}")))
        } else {
            String::new()
        };
        let raw = format!(
            "{}  {spinner} {bar}{count}  {label}{hint}  {timer}",
            self.muted("│")
        );
        Self::truncate_ellipsis(&raw, usize::from(self.width))
    }

    pub(crate) fn step_banner(self, completed: usize, total: usize, step_name: &str) -> String {
        let badge = if total > 0 {
            let current = (completed + 1).min(total);
            self.primary(&format!("[{current}/{total}]"))
        } else {
            self.primary("▸")
        };
        format!(
            "{}\n{}  {badge} {}  {}\n",
            self.muted("│"),
            self.muted("├"),
            self.bold(step_name),
            self.muted(Mascot::from_step_name(step_name).quip())
        )
    }

    pub(crate) fn step_done_line(
        self,
        step_name: &str,
        detail: Option<&str>,
        elapsed: Duration,
    ) -> String {
        let node = self.green("✓");
        let timer = self.muted(&Self::format_duration(elapsed));
        match detail {
            Some(detail) if !detail.is_empty() => {
                format!(
                    "{}  {node} {}  {}  {timer}\n",
                    self.muted("│"),
                    self.text(step_name),
                    self.muted(detail)
                )
            }
            _ => format!(
                "{}  {node} {}  {timer}\n",
                self.muted("│"),
                self.text(step_name)
            ),
        }
    }

    pub(crate) fn step_fail_line(self, step_name: &str, elapsed: Duration) -> String {
        let node = self.red("✗");
        let timer = self.muted(&Self::format_duration(elapsed));
        format!(
            "{}  {node} {}  {timer}\n",
            self.muted("│"),
            self.red(step_name)
        )
    }

    pub(crate) fn notice_line(self, tone: NoticeStyle, text: &str) -> String {
        let (glyph, styled) = match tone {
            NoticeStyle::Info => (self.primary("·"), self.text(text)),
            NoticeStyle::Warning => (self.gold("!"), self.gold(text)),
            NoticeStyle::Detail => (self.muted("·"), self.muted(text)),
        };
        format!("{}  {glyph} {styled}\n", self.muted("│"))
    }

    pub(crate) fn summary_card(
        self,
        command: &str,
        branch_commit: (Option<&str>, Option<&str>),
        profile: Option<&str>,
        artifact: Option<&Path>,
        free_bytes: Option<u64>,
        elapsed: Duration,
    ) -> String {
        let (branch, commit) = branch_commit;
        let p = self.palette();
        let title = format!("ready · {}", Self::format_duration(elapsed));
        let mut lines = vec![format!(
            "{}  {}",
            self.accent(Mascot::Celebrate.face(0)),
            self.bold(&Self::celebration_line(command))
        )];
        if let Some(branch) = branch {
            let revision = match commit {
                Some(sha) if sha != branch => format!("{branch} ({sha})"),
                _ => branch.to_owned(),
            };
            lines.push(self.kv("branch", &revision));
        }
        if let Some(profile) = profile {
            lines.push(self.kv("profile", profile));
        }
        if let Some(artifact) = artifact {
            lines.push(self.kv(
                "artifact",
                &Self::compact_path(artifact, self.card_width().saturating_sub(15)),
            ));
        }
        if let Some(free) = free_bytes {
            lines.push(self.kv("storage", &format!("{:.1} GiB free", Self::gib(free))));
        }
        format!(
            "{}\n{}",
            self.muted("└"),
            self.card(&title, p.green, &lines)
        )
    }

    pub(crate) fn error_card(self, message: &str, hint: Option<&str>) -> String {
        let p = self.palette();
        let mut lines = Vec::new();
        let wrap = self.card_width().saturating_sub(6).max(24);
        for chunk in Self::wrap_text(message, wrap) {
            lines.push(self.text(&chunk));
        }
        if let Some(hint) = hint {
            lines.push(String::new());
            for (index, chunk) in Self::wrap_text(hint, wrap.saturating_sub(6))
                .into_iter()
                .enumerate()
            {
                if index == 0 {
                    lines.push(format!("{} {}", self.gold("try"), self.muted(&chunk)));
                } else {
                    lines.push(format!("    {}", self.muted(&chunk)));
                }
            }
        }
        self.card("vmux error", p.red, &lines)
    }

    pub(crate) fn clean_prompt(
        self,
        frame: usize,
        apply_selected: bool,
        reclaimable_count: usize,
        free_bytes: u64,
        cache_bytes: u64,
    ) -> String {
        let p = self.palette();
        let gauge = self.storage_gauge(free_bytes, cache_bytes, 16);
        let apply_btn = if apply_selected {
            self.fg_bold(p.green, "› Apply cleanup")
        } else {
            self.muted("  Apply cleanup")
        };
        let keep_btn = if !apply_selected {
            self.fg_bold(p.primary, "› Keep preview")
        } else {
            self.muted("  Keep preview")
        };
        let plural = if reclaimable_count == 1 {
            "item"
        } else {
            "items"
        };
        let sparkle = self.accent(Self::sparkle(frame));
        let lines = vec![
            format!(
                "{sparkle}  {} reclaimable cache {plural}",
                self.bold(&reclaimable_count.to_string())
            ),
            format!(
                "   {gauge}  {} free · {} caches",
                self.green(&format!("{:.1} GiB", Self::gib(free_bytes))),
                self.muted(&format!("{:.1} GiB", Self::gib(cache_bytes)))
            ),
            String::new(),
            format!("   {apply_btn}     {keep_btn}"),
            self.muted("   ←/→ switch · y/n · ↵ confirm · esc cancel"),
        ];
        self.card("cache clean", p.gold, &lines)
    }

    pub(crate) fn gradient_bar(
        self,
        completed: usize,
        total: usize,
        width: usize,
        frame: usize,
    ) -> String {
        let p = self.palette();
        let mut bar = String::new();
        let pulse = frame % width.max(1);
        let filled = ((completed.min(total)) * width)
            .checked_div(total)
            .unwrap_or(0);
        for index in 0..width {
            let rgb = p
                .primary
                .lerp(p.accent, index, width.saturating_sub(1).max(1));
            if total == 0 {
                let dist = index.abs_diff(pulse);
                let glyph = if dist <= 1 {
                    self.fg(rgb, "━")
                } else {
                    self.muted("─")
                };
                bar.push_str(&glyph);
            } else if index < filled {
                bar.push_str(&self.fg(rgb, "━"));
            } else if index == filled && completed < total {
                bar.push_str(&self.fg(rgb, "╸"));
            } else {
                bar.push_str(&self.muted("─"));
            }
        }
        bar
    }

    pub(crate) fn storage_gauge(self, free_bytes: u64, cache_bytes: u64, width: usize) -> String {
        let total = free_bytes.saturating_add(cache_bytes).max(1);
        let cache_cells = ((cache_bytes.saturating_mul(width as u64)) / total) as usize;
        let cache_cells = cache_cells.min(width);
        let free_cells = width.saturating_sub(cache_cells);
        format!(
            "{}{}",
            self.gold(&"━".repeat(cache_cells)),
            self.muted(&"─".repeat(free_cells))
        )
    }

    pub(crate) fn theme_list(self, active: ThemePreset) -> String {
        let mut out = String::new();
        for preset in ThemePreset::ALL {
            let marker = if preset == active {
                self.green("★")
            } else {
                " ".into()
            };
            let swatches = self.swatches(preset);
            let name = if preset == active {
                self.bold(preset.id())
            } else {
                preset.id().to_owned()
            };
            out.push_str(&format!("  {marker} {swatches}  {name}\n"));
        }
        out
    }

    fn kv(self, label: &str, value: &str) -> String {
        format!(
            "  {}  {}",
            self.muted(&format!("{label:<8}")),
            self.text(value)
        )
    }

    fn celebration_line(command: &str) -> String {
        match command {
            "desktop" | "run" => "Vmux Desktop launched".into(),
            "build" => "Build packaged and ready".into(),
            "cache clean" => "Build caches cleaned".into(),
            "cache status" => "Storage check complete".into(),
            "doctor" => "Toolchain looks healthy".into(),
            "setup" => "Development tools ready".into(),
            "stop" => "Development service stopped".into(),
            "reset-layout" => "Layout reset complete".into(),
            _ => "Done".into(),
        }
    }

    pub(crate) fn spinner(frame: usize) -> &'static str {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[frame % FRAMES.len()]
    }

    pub(crate) fn sparkle(frame: usize) -> &'static str {
        const FRAMES: [&str; 4] = ["✦", "✧", "·", "✧"];
        FRAMES[(frame / 3) % FRAMES.len()]
    }

    pub(crate) fn format_duration(duration: Duration) -> String {
        let millis = duration.as_millis();
        if millis < 1000 {
            return format!("{:.1}s", millis as f64 / 1000.0);
        }
        let secs = duration.as_secs();
        if secs < 60 {
            return format!("{:.1}s", duration.as_secs_f64());
        }
        let mins = secs / 60;
        let rem = secs % 60;
        format!("{mins}m {rem:02}s")
    }

    pub(crate) fn gib(bytes: u64) -> f64 {
        bytes as f64 / GIB as f64
    }

    pub(crate) fn display_width(text: &str) -> usize {
        let mut width = 0usize;
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for esc in chars.by_ref() {
                    if esc.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            width += Self::char_width(ch);
        }
        width
    }

    fn char_width(ch: char) -> usize {
        let code = ch as u32;
        if matches!(code, 0x200D | 0xFE0E | 0xFE0F | 0x0300..=0x036F) {
            return 0;
        }
        if matches!(
            code,
            0x1100..=0x115F
                | 0x2E80..=0x303E
                | 0x3040..=0xA4CF
                | 0xAC00..=0xD7A3
                | 0xF900..=0xFAFF
                | 0xFE10..=0xFE19
                | 0xFE30..=0xFE6F
                | 0xFF00..=0xFF60
                | 0xFFE0..=0xFFE6
                | 0x1F300..=0x1FAFF
        ) {
            return 2;
        }
        1
    }

    fn truncate_ellipsis(text: &str, max_width: usize) -> String {
        if Self::display_width(text) <= max_width {
            return text.to_owned();
        }
        let limit = max_width.saturating_sub(1);
        let mut out = String::new();
        let mut width = 0usize;
        let mut in_ansi = false;
        let mut saw_ansi = false;
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                in_ansi = true;
                saw_ansi = true;
                out.push(ch);
                continue;
            }
            if in_ansi {
                out.push(ch);
                if ch.is_ascii_alphabetic() {
                    in_ansi = false;
                }
                continue;
            }
            let w = Self::char_width(ch);
            if width + w > limit {
                out.push('…');
                if saw_ansi {
                    out.push_str("\x1b[0m");
                }
                break;
            }
            out.push(ch);
            width += w;
        }
        out
    }

    fn compact_path(path: &Path, max_width: usize) -> String {
        let text = path.display().to_string();
        if text.chars().count() <= max_width || max_width < 12 {
            return text;
        }
        let keep = max_width.saturating_sub(1);
        let tail: String = text
            .chars()
            .rev()
            .take(keep)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("…{tail}")
    }

    fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.lines() {
            let mut current = String::new();
            for word in paragraph.split_whitespace() {
                if current.is_empty() {
                    current.push_str(word);
                } else if current.chars().count() + 1 + word.chars().count() <= max_width {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    lines.push(current);
                    current = word.to_owned();
                }
            }
            if !current.is_empty() || lines.is_empty() {
                lines.push(current);
            }
        }
        lines
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeStyle {
    Info,
    Warning,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mascot {
    Idle,
    Cli,
    Storage,
    Lock,
    Seed,
    Tools,
    Cargo,
    Sign,
    Launch,
    Clean,
    Celebrate,
}

impl Mascot {
    pub(crate) fn from_step_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower.contains("cli") || lower.contains("bootstrap") {
            Self::Cli
        } else if lower.contains("clean") {
            Self::Clean
        } else if lower.contains("storage") || lower.contains("space") {
            Self::Storage
        } else if lower.contains("lock") {
            Self::Lock
        } else if lower.contains("seed") {
            Self::Seed
        } else if lower.contains("cargo")
            || lower.contains("compile")
            || lower.contains("lint")
            || lower.contains("build ")
        {
            Self::Cargo
        } else if lower.contains("sign")
            || lower.contains("bundle")
            || lower.contains("package")
            || lower.contains("publish")
        {
            Self::Sign
        } else if lower.contains("run") || lower.contains("launch") {
            Self::Launch
        } else if lower.contains("tool")
            || lower.contains("helper")
            || lower.contains("stylesheet")
            || lower.contains("dioxus")
            || lower.contains("wasm")
            || lower.contains("simulator")
            || lower.contains("doctor")
        {
            Self::Tools
        } else {
            Self::Idle
        }
    }

    pub(crate) fn face(self, tick: usize) -> &'static str {
        let frames: &[&str] = match self {
            Self::Idle => &["(•‿•)", "(•ᴗ•)"],
            Self::Cli => &["(•̀ᴗ•́)", "(•ᴗ•)✧"],
            Self::Storage => &["(•_•)", "(•‿•)"],
            Self::Lock => &["(·_·)", "(•_•)"],
            Self::Seed => &["(˘‿˘)", "(•‿•)"],
            Self::Tools => &["(•̀‿•́)", "(•ᴗ•)"],
            Self::Cargo => &["(•̀_•́)", "(•̀ᴗ•́)"],
            Self::Sign => &["(•ᴗ•)✧", "(•‿•)✦"],
            Self::Launch => &["(•ᴗ•)✦", "(^‿^)✧"],
            Self::Clean => &["(•‿•)✧", "(^‿^)"],
            Self::Celebrate => &["(•ᴗ•)✦"],
        };
        frames[tick % frames.len()]
    }

    pub(crate) fn quip(self) -> &'static str {
        match self {
            Self::Idle => "working...",
            Self::Cli => "updating dev cli...",
            Self::Storage => "checking storage...",
            Self::Lock => "waiting for target lock...",
            Self::Seed => "warming target cache...",
            Self::Tools => "preparing tools...",
            Self::Cargo => "compiling workspace...",
            Self::Sign => "signing & bundling...",
            Self::Launch => "launching app...",
            Self::Clean => "sweeping caches...",
            Self::Celebrate => "ready",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryHint;

impl RecoveryHint {
    pub(crate) fn for_error(message: &str) -> Option<&'static str> {
        if message.contains("Insufficient build space") {
            Some("Run `vmux cache clean --apply` or close unused builds to free disk space.")
        } else if message.contains("No completed") && message.contains("run vmux build") {
            Some("Remove `--no-build` to auto-build, or run the `vmux build` command shown above.")
        } else if message.contains("Branch has no worktree") {
            Some(
                "Run `git worktree list` to see active checkouts, or `vmux tui` to pick interactively.",
            )
        } else if message.contains("ambiguous") || message.contains("matches multiple worktrees") {
            Some("Pass the full branch name, or run `vmux tui` to select the exact worktree.")
        } else if message.contains("Not inside a Vmux repository") {
            Some("cd into a Vmux checkout or pass `--root /path/to/vmux`.")
        } else if message.contains("target cache lock") || message.contains("Target lock") {
            Some(
                "Another vmux build owns this target directory; wait for it or stop the other build.",
            )
        } else if message.contains("incomplete or has changed") {
            Some("Rebuild cleanly with `vmux build` before launching.")
        } else if message.contains("Build command exited with") {
            Some("Inspect the compiler diagnostic above, fix the issue, and re-run.")
        } else if message.contains("requires a terminal") {
            Some(
                "Run explicit subcommands like `vmux desktop`, `vmux build`, or `vmux cache status`.",
            )
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CockpitAction {
    Desktop,
    Build,
    CacheClean,
    CacheStatus,
    Doctor,
    Serve,
    ResetLayout,
    Quit,
}

impl CockpitAction {
    pub(crate) const ALL: [Self; 8] = [
        Self::Desktop,
        Self::Build,
        Self::CacheClean,
        Self::CacheStatus,
        Self::Doctor,
        Self::Serve,
        Self::ResetLayout,
        Self::Quit,
    ];

    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Self::Desktop => "▸",
            Self::Build => "◆",
            Self::CacheClean => "◇",
            Self::CacheStatus => "◎",
            Self::Doctor => "✦",
            Self::Serve => "◉",
            Self::ResetLayout => "↺",
            Self::Quit => "×",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Desktop => "Launch Desktop",
            Self::Build => "Build App",
            Self::CacheClean => "Clean Cache",
            Self::CacheStatus => "Cache Status",
            Self::Doctor => "Toolchain Doctor",
            Self::Serve => "Serve Docs",
            Self::ResetLayout => "Reset Layout",
            Self::Quit => "Quit",
        }
    }

    pub(crate) fn subtitle(self) -> &'static str {
        match self {
            Self::Desktop => "Open desktop app (auto-builds if stale)",
            Self::Build => "Compile and bundle without launching",
            Self::CacheClean => "Preview and reclaim unused build caches",
            Self::CacheStatus => "Inspect free space and compile cache usage",
            Self::Doctor => "Verify CEF, signing identity, and tools",
            Self::Serve => "Run website and docs with live reload",
            Self::ResetLayout => "Reset inactive local layout files",
            Self::Quit => "Exit back to shell",
        }
    }

    pub(crate) fn command_id(self) -> Option<&'static str> {
        match self {
            Self::Desktop => Some("dx.desktop"),
            Self::Build => Some("dx.build"),
            Self::CacheClean => Some("dx.cache.clean"),
            Self::CacheStatus => Some("dx.cache.status"),
            Self::Doctor => Some("dx.doctor"),
            Self::Serve => Some("dx.serve"),
            Self::ResetLayout => Some("dx.reset-layout"),
            Self::Quit => None,
        }
    }

    pub(crate) fn uses_profile(self) -> bool {
        matches!(self, Self::Desktop | Self::Build | Self::ResetLayout)
    }

    pub(crate) fn mascot(self) -> Mascot {
        match self {
            Self::Desktop => Mascot::Launch,
            Self::Build => Mascot::Cargo,
            Self::CacheClean => Mascot::Clean,
            Self::CacheStatus => Mascot::Storage,
            Self::Doctor => Mascot::Tools,
            Self::Serve => Mascot::Idle,
            Self::ResetLayout => Mascot::Sign,
            Self::Quit => Mascot::Idle,
        }
    }

    pub(crate) fn preview_command(self, branch: Option<&str>, profile: CockpitProfile) -> String {
        let mut command = match self {
            Self::Desktop => "vmux desktop".to_owned(),
            Self::Build => "vmux build".to_owned(),
            Self::CacheClean => "vmux cache clean".to_owned(),
            Self::CacheStatus => "vmux cache status".to_owned(),
            Self::Doctor => "vmux doctor".to_owned(),
            Self::Serve => "vmux serve".to_owned(),
            Self::ResetLayout => "vmux reset-layout".to_owned(),
            Self::Quit => "exit".to_owned(),
        };
        if self == Self::Quit {
            return command;
        }
        if let Some(branch) = branch {
            if matches!(self, Self::CacheClean | Self::CacheStatus) {
                command.push_str(&format!(" -b {branch}"));
            } else {
                command.push_str(&format!(" {branch}"));
            }
        }
        if self.uses_profile() && profile != CockpitProfile::Dev {
            command.push_str(&format!(" -p {}", profile.as_str()));
        }
        command
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CockpitProfile {
    Dev,
    Local,
    Release,
}

impl CockpitProfile {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Local => "local",
            Self::Release => "release",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Dev => Self::Local,
            Self::Local => Self::Release,
            Self::Release => Self::Dev,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Dev => Self::Release,
            Self::Local => Self::Dev,
            Self::Release => Self::Local,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CockpitPanel {
    Spaces,
    Actions,
    Themes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CockpitInput {
    Up,
    Down,
    Left,
    Right,
    Tab,
    Leader,
    NextPane,
    ZoomPane,
    ToggleSidebar,
    FocusSpaces,
    FocusActions,
    FocusThemes,
    Profile,
    ThemeNext,
    ThemePrev,
    Enter,
    Escape,
    Quit,
    ConfirmYes,
    ConfirmNo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CockpitState {
    pub worktrees: Vec<(String, Option<String>, String, bool, bool)>,
    pub action_index: usize,
    pub worktree_index: usize,
    pub panel: CockpitPanel,
    pub profile: CockpitProfile,
    pub theme: ThemePreset,
    pub free_bytes: Option<u64>,
    pub frame: usize,
    pub leader_active: bool,
    pub zoomed: bool,
    pub sidebar_open: bool,
}

impl CockpitState {
    pub(crate) fn new(
        worktrees: Vec<(String, Option<String>, String, bool, bool)>,
        free_bytes: Option<u64>,
        theme: ThemePreset,
    ) -> Self {
        let current = worktrees.iter().position(|entry| entry.3).unwrap_or(0);
        Self {
            worktrees,
            action_index: 0,
            worktree_index: current,
            panel: CockpitPanel::Actions,
            profile: CockpitProfile::Dev,
            theme,
            free_bytes,
            frame: 0,
            leader_active: false,
            zoomed: false,
            sidebar_open: true,
        }
    }

    pub(crate) fn selected_action(&self) -> CockpitAction {
        CockpitAction::ALL[self.action_index % CockpitAction::ALL.len()]
    }

    pub(crate) fn selected_branch(&self) -> Option<String> {
        self.worktrees
            .get(self.worktree_index)
            .and_then(|entry| entry.1.clone())
    }

    pub(crate) fn apply(
        &mut self,
        input: CockpitInput,
    ) -> Option<(CockpitAction, Option<String>, CockpitProfile, ThemePreset)> {
        if self.leader_active {
            return self.apply_chord(input);
        }
        match input {
            CockpitInput::Leader => self.leader_active = true,
            CockpitInput::Up => self.move_selection(-1),
            CockpitInput::Down => self.move_selection(1),
            CockpitInput::Left => self.profile = self.profile.prev(),
            CockpitInput::Right | CockpitInput::Profile => self.profile = self.profile.next(),
            CockpitInput::ThemeNext => self.theme = self.theme.next(),
            CockpitInput::ThemePrev => self.theme = self.theme.prev(),
            CockpitInput::ZoomPane => self.zoomed = !self.zoomed,
            CockpitInput::ToggleSidebar => self.toggle_sidebar(),
            CockpitInput::FocusSpaces if !self.worktrees.is_empty() => {
                self.sidebar_open = true;
                self.panel = CockpitPanel::Spaces;
            }
            CockpitInput::FocusActions => self.panel = CockpitPanel::Actions,
            CockpitInput::FocusThemes => self.panel = CockpitPanel::Themes,
            CockpitInput::Tab | CockpitInput::NextPane => self.cycle_pane(1),
            CockpitInput::Enter => {
                if self.panel == CockpitPanel::Themes {
                    self.panel = CockpitPanel::Actions;
                    return None;
                }
                let action = self.selected_action();
                return Some((action, self.selected_branch(), self.profile, self.theme));
            }
            CockpitInput::Quit | CockpitInput::Escape => {
                return Some((CockpitAction::Quit, None, self.profile, self.theme));
            }
            CockpitInput::FocusSpaces | CockpitInput::ConfirmYes | CockpitInput::ConfirmNo => {}
        }
        None
    }

    fn apply_chord(
        &mut self,
        input: CockpitInput,
    ) -> Option<(CockpitAction, Option<String>, CockpitProfile, ThemePreset)> {
        self.leader_active = false;
        match input {
            CockpitInput::Left => self.cycle_pane(-1),
            CockpitInput::Right | CockpitInput::NextPane | CockpitInput::Tab => self.cycle_pane(1),
            CockpitInput::ZoomPane => self.zoomed = !self.zoomed,
            CockpitInput::ToggleSidebar => self.toggle_sidebar(),
            CockpitInput::ConfirmNo => self.profile = self.profile.next(),
            CockpitInput::Profile => self.profile = self.profile.prev(),
            CockpitInput::FocusSpaces if !self.worktrees.is_empty() => {
                self.sidebar_open = true;
                self.panel = CockpitPanel::Spaces;
            }
            CockpitInput::FocusActions => self.panel = CockpitPanel::Actions,
            CockpitInput::FocusThemes => self.panel = CockpitPanel::Themes,
            CockpitInput::ThemeNext => self.theme = self.theme.next(),
            CockpitInput::ThemePrev => self.theme = self.theme.prev(),
            CockpitInput::Quit => {
                return Some((CockpitAction::Quit, None, self.profile, self.theme));
            }
            _ => {}
        }
        None
    }

    fn toggle_sidebar(&mut self) {
        self.sidebar_open = !self.sidebar_open;
        if !self.sidebar_open && self.panel == CockpitPanel::Spaces {
            self.panel = CockpitPanel::Actions;
        }
    }

    fn cycle_pane(&mut self, delta: isize) {
        let panes: &[CockpitPanel] = if !self.sidebar_open || self.worktrees.is_empty() {
            &[CockpitPanel::Actions, CockpitPanel::Themes]
        } else {
            &[
                CockpitPanel::Spaces,
                CockpitPanel::Actions,
                CockpitPanel::Themes,
            ]
        };
        let current = panes.iter().position(|p| *p == self.panel).unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(panes.len() as isize) as usize;
        self.panel = panes[next];
    }

    fn move_selection(&mut self, delta: isize) {
        match self.panel {
            CockpitPanel::Actions => {
                let count = CockpitAction::ALL.len() as isize;
                self.action_index = (self.action_index as isize + delta).rem_euclid(count) as usize;
            }
            CockpitPanel::Spaces if !self.worktrees.is_empty() => {
                let count = self.worktrees.len() as isize;
                self.worktree_index =
                    (self.worktree_index as isize + delta).rem_euclid(count) as usize;
            }
            CockpitPanel::Spaces => {}
            CockpitPanel::Themes => {
                self.theme = if delta < 0 {
                    self.theme.prev()
                } else {
                    self.theme.next()
                };
            }
        }
    }

    pub(crate) fn render(&self, color: bool, cols: u16, rows: u16) -> String {
        let theme = Theme::new(self.theme, color, cols);
        let width = usize::from(cols).max(56);
        let height = usize::from(rows).max(16);
        let body_h = height.saturating_sub(1);
        let usable = width.saturating_sub(2);
        let show_sidebar = self.sidebar_open && usable >= 78;
        let (side_w, main_w) = if show_sidebar {
            let sw = ((usable * 22) / 100).clamp(24, 34);
            (sw, usable.saturating_sub(sw + 1))
        } else {
            (0, usable)
        };
        let main_lines = self.build_main_area(theme, main_w, body_h);
        let composed = if show_sidebar {
            let side_lines = self.build_side_sheet(theme, side_w, body_h);
            Self::join_columns(&[side_lines, main_lines])
        } else {
            main_lines
        };
        let mut out = String::new();
        for line in composed {
            out.push(' ');
            out.push_str(&line);
            out.push('\n');
        }
        out.push(' ');
        out.push_str(&self.footer_bar(theme, usable));
        out
    }

    fn build_main_area(&self, theme: Theme, width: usize, height: usize) -> Vec<String> {
        let pane_h = height.saturating_sub(3).max(8);
        let mut out = vec![
            self.tab_strip(theme, width),
            self.url_bar(theme, width),
            String::new(),
        ];
        let panes = if self.zoomed {
            match self.panel {
                CockpitPanel::Themes => self.build_inspector_pane(theme, width, pane_h),
                _ => self.build_actions_pane(theme, width, pane_h),
            }
        } else if width >= 66 {
            let w1 = (width.saturating_sub(1) * 48) / 100;
            let w2 = width.saturating_sub(w1 + 1);
            let p1 = self.build_actions_pane(theme, w1, pane_h);
            let p2 = self.build_inspector_pane(theme, w2, pane_h);
            Self::join_columns(&[p1, p2])
        } else {
            self.build_actions_pane(theme, width, pane_h)
        };
        out.extend(panes);
        out
    }

    fn tab_strip(&self, theme: Theme, width: usize) -> String {
        let mut tabs = Vec::new();
        for candidate in [
            CockpitProfile::Dev,
            CockpitProfile::Local,
            CockpitProfile::Release,
        ] {
            let label = format!("  {}  ", candidate.as_str());
            if candidate == self.profile {
                tabs.push(theme.surface_row(&theme.bold(&label), label.len()));
            } else {
                tabs.push(theme.muted(&label));
            }
        }
        let zoom = if self.zoomed {
            format!("  {}", theme.gold("zoom"))
        } else {
            String::new()
        };
        let left = format!("{} {}{zoom}", tabs.join(" "), theme.muted("+"));
        let right = match self.free_bytes {
            Some(bytes) => theme.muted(&format!("{:.1} GiB free", Theme::gib(bytes))),
            None => String::new(),
        };
        let used = Theme::display_width(&left) + Theme::display_width(&right);
        let gap = width.saturating_sub(used).max(2);
        Theme::truncate_ellipsis(&format!("{left}{}{right}", " ".repeat(gap)), width)
    }

    fn url_bar(&self, theme: Theme, width: usize) -> String {
        let nav = theme.muted("←  →  ↻");
        let url = self.current_url();
        let right = format!(
            "{} {}",
            theme.swatches(self.theme),
            theme.muted(self.theme.id())
        );
        let nav_w = Theme::display_width(&nav);
        let right_w = Theme::display_width(&right);
        let addr_w = width.saturating_sub(nav_w + right_w + 6).max(16);
        let addr_text = format!("  {url}");
        let addr = theme.surface_row(&theme.text(&addr_text), addr_w);
        format!("{nav}   {addr}   {right}")
    }

    fn current_url(&self) -> String {
        match self.panel {
            CockpitPanel::Spaces => {
                let branch = self.selected_branch().unwrap_or_else(|| "current".into());
                format!("vmux://spaces/{branch}")
            }
            CockpitPanel::Actions => {
                let slug = self
                    .selected_action()
                    .label()
                    .to_ascii_lowercase()
                    .replace(' ', "-");
                format!("vmux://commands/{slug}")
            }
            CockpitPanel::Themes => format!("vmux://settings/appearance?theme={}", self.theme.id()),
        }
    }

    fn build_side_sheet(&self, theme: Theme, width: usize, height: usize) -> Vec<String> {
        let focused = self.panel == CockpitPanel::Spaces;
        let inner = width.saturating_sub(1);
        let sep = if focused {
            theme.primary("│")
        } else {
            theme.muted("│")
        };
        let mut rows = Vec::new();
        let (space_name, space_meta) = self
            .worktrees
            .get(self.worktree_index)
            .map(|(branch, _, folder, _, dirty)| {
                let state = if *dirty { "modified" } else { "clean" };
                (branch.as_str(), format!("{state} · {folder}"))
            })
            .unwrap_or(("workspace", "current checkout".into()));
        let dots = format!(
            "{} {} {}",
            theme.muted("●"),
            theme.muted("●"),
            theme.muted("●")
        );
        rows.push(format!("{dots}  {}", theme.muted("vmux")));
        rows.push(String::new());
        let card_top = format!(" {} {}", theme.primary("●"), theme.bold(space_name));
        rows.push(theme.surface_row(&card_top, inner));
        rows.push(format!("   {}", theme.muted(&space_meta)));
        rows.push(String::new());
        let spaces_head = if focused {
            theme.primary("SPACES")
        } else {
            theme.muted("SPACES")
        };
        rows.push(format!(" {spaces_head}"));
        let max_wt = height.saturating_sub(14).clamp(3, 12);
        let start = self.worktree_index.saturating_sub(max_wt / 2);
        let end = (start + max_wt).min(self.worktrees.len());
        for index in start..end {
            let (branch, _, _, current, dirty) = &self.worktrees[index];
            let selected = index == self.worktree_index;
            let dot = if *dirty {
                theme.gold("●")
            } else if selected {
                theme.primary("●")
            } else {
                theme.muted("○")
            };
            let suffix = if *current {
                format!(" {}", theme.muted("·"))
            } else {
                String::new()
            };
            let name = if selected {
                theme.bold(branch)
            } else {
                theme.text(branch)
            };
            let line = format!(" {dot} {name}{suffix}");
            if selected && focused {
                rows.push(theme.surface_row(&line, inner));
            } else {
                rows.push(line);
            }
        }
        rows.push(String::new());
        self.push_side_pane(
            &mut rows,
            theme,
            inner,
            "PANE 1",
            "▸",
            "Commands",
            CockpitPanel::Actions,
        );
        rows.push(String::new());
        self.push_side_pane(
            &mut rows,
            theme,
            inner,
            "PANE 2",
            "✦",
            "Appearance",
            CockpitPanel::Themes,
        );
        let mut out = Vec::with_capacity(height);
        for index in 0..height {
            let raw = rows.get(index).map(String::as_str).unwrap_or("");
            let fitted = Theme::truncate_ellipsis(raw, inner);
            let pad = inner.saturating_sub(Theme::display_width(&fitted));
            out.push(format!("{fitted}{}{sep}", " ".repeat(pad)));
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn push_side_pane(
        &self,
        rows: &mut Vec<String>,
        theme: Theme,
        inner: usize,
        heading: &str,
        glyph: &str,
        title: &str,
        panel: CockpitPanel,
    ) {
        rows.push(format!(" {}", theme.muted(heading)));
        let active = self.panel == panel;
        let line = format!(
            " {} {}",
            if active {
                theme.primary(glyph)
            } else {
                theme.muted(glyph)
            },
            if active {
                theme.bold(title)
            } else {
                theme.muted(title)
            }
        );
        if active {
            rows.push(theme.surface_row(&line, inner));
        } else {
            rows.push(line);
        }
    }

    fn join_columns(columns: &[Vec<String>]) -> Vec<String> {
        let rows = columns.iter().map(Vec::len).max().unwrap_or(0);
        let mut out = Vec::with_capacity(rows);
        for row in 0..rows {
            let line = columns
                .iter()
                .map(|col| col.get(row).map(String::as_str).unwrap_or(""))
                .collect::<Vec<_>>()
                .join(" ");
            out.push(line);
        }
        out
    }

    fn build_actions_pane(&self, theme: Theme, width: usize, height: usize) -> Vec<String> {
        let focused = self.panel == CockpitPanel::Actions;
        let inner = width.saturating_sub(2);
        let mut rows = vec![String::new()];
        for (index, item) in CockpitAction::ALL.iter().enumerate() {
            let selected = index == self.action_index;
            let cursor = if selected && focused {
                theme.primary("›")
            } else {
                " ".into()
            };
            let glyph = if selected {
                theme.accent(item.glyph())
            } else {
                theme.muted(item.glyph())
            };
            let label = if selected {
                theme.bold(item.label())
            } else {
                theme.text(item.label())
            };
            let line = format!(" {cursor} {glyph} {label}");
            if selected {
                rows.push(theme.surface_row(&line, inner));
            } else {
                rows.push(line);
            }
        }
        rows.push(String::new());
        rows.push(format!(
            "   {}",
            theme.muted(self.selected_action().subtitle())
        ));
        theme.pane_box("Commands", "2", focused, width, height, &rows)
    }

    fn build_inspector_pane(&self, theme: Theme, width: usize, height: usize) -> Vec<String> {
        let focused = self.panel == CockpitPanel::Themes;
        let inner = width.saturating_sub(2);
        let action = self.selected_action();
        let branch = self.selected_branch();
        let cmd = action.preview_command(branch.as_deref(), self.profile);
        let face = theme.accent(action.mascot().face(self.frame / 4));
        let mut rows = vec![
            String::new(),
            format!("  {face}  {}", theme.text(action.label())),
            String::new(),
            format!("  {}  {}", theme.muted("$"), theme.primary(&cmd)),
        ];
        if let Some((display, _, folder, _, dirty)) = self.worktrees.get(self.worktree_index) {
            let status = if *dirty {
                theme.gold("modified")
            } else {
                theme.green("clean")
            };
            rows.push(format!(
                "  {}  {}  {}",
                theme.muted("space"),
                theme.text(display),
                status
            ));
            rows.push(format!(
                "  {}  {}",
                theme.muted("dir  "),
                theme.muted(folder)
            ));
        }
        rows.push(String::new());
        rows.push(format!("  {}", theme.muted("Themes")));
        let avail = height.saturating_sub(rows.len() + 3).clamp(4, 15);
        let active_idx = self.theme.index();
        let start = active_idx.saturating_sub(avail / 2);
        let end = (start + avail).min(ThemePreset::ALL.len());
        for index in start..end {
            let preset = ThemePreset::ALL[index];
            let selected = preset == self.theme;
            let cursor = if selected && focused {
                theme.primary("›")
            } else if selected {
                theme.accent("·")
            } else {
                " ".into()
            };
            let name = if selected {
                theme.bold(preset.id())
            } else {
                theme.muted(preset.id())
            };
            let line = format!(" {cursor} {}  {name}", theme.swatches(preset));
            if selected {
                rows.push(theme.surface_row(&line, inner));
            } else {
                rows.push(line);
            }
        }
        theme.pane_box("Appearance", "3", focused, width, height, &rows)
    }

    fn footer_bar(&self, theme: Theme, width: usize) -> String {
        let text = if self.leader_active {
            format!(
                "{}  {}",
                theme.fg_bold(theme.palette().gold, "Ctrl+b"),
                theme.text(
                    "h/l pane · o next · z zoom · s sidebar · n/p tab · 1-3 jump · esc cancel"
                )
            )
        } else {
            format!(
                "{} {}  {} {}  {} {}  {} {}  {} {}  {} {}",
                theme.text("Ctrl+b"),
                theme.muted("h/l/o/z/s"),
                theme.text("Tab/1-3"),
                theme.muted("pane"),
                theme.text("↑↓"),
                theme.muted("move"),
                theme.text("←→"),
                theme.muted("tab"),
                theme.text("t"),
                theme.muted("theme"),
                theme.text("↵/q"),
                theme.muted("run/quit")
            )
        };
        Theme::truncate_ellipsis(&text, width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mascot_maps_step_names_and_recovery_hints_match_known_failures() {
        assert_eq!(
            Mascot::from_step_name("Check build storage"),
            Mascot::Storage
        );
        assert_eq!(Mascot::from_step_name("Acquire target lock"), Mascot::Lock);
        assert_eq!(Mascot::from_step_name("Build desktop app"), Mascot::Cargo);
        assert_eq!(Mascot::from_step_name("Sign debug binaries"), Mascot::Sign);
        assert_eq!(Mascot::from_step_name("Launch desktop app"), Mascot::Launch);
        assert_eq!(Mascot::from_step_name("Clean build cache"), Mascot::Clean);
        assert!(
            RecoveryHint::for_error("Insufficient build space: need 12 GiB free")
                .unwrap()
                .contains("vmux cache clean --apply")
        );
        assert!(
            RecoveryHint::for_error("Branch has no worktree; create its worktree explicitly first")
                .unwrap()
                .contains("vmux tui")
        );
        assert!(
            RecoveryHint::for_error(
                "No completed dev app for branch main; run vmux build main first"
            )
            .unwrap()
            .contains("--no-build")
        );
    }

    #[test]
    fn cockpit_navigation_switches_panels_profiles_themes_and_confirms_selection() {
        let mut state = CockpitState::new(
            vec![
                (
                    "main".into(),
                    Some("main".into()),
                    "vmux".into(),
                    true,
                    false,
                ),
                (
                    "feat/tui".into(),
                    Some("feat/tui".into()),
                    "vmx-293".into(),
                    false,
                    true,
                ),
            ],
            Some(400 * GIB),
            ThemePreset::CatppuccinMocha,
        );
        assert_eq!(state.selected_action(), CockpitAction::Desktop);
        assert_eq!(state.selected_branch().as_deref(), Some("main"));
        assert!(state.apply(CockpitInput::Down).is_none());
        assert_eq!(state.selected_action(), CockpitAction::Build);
        assert!(state.apply(CockpitInput::Right).is_none());
        assert_eq!(state.profile, CockpitProfile::Local);
        assert!(state.apply(CockpitInput::ThemeNext).is_none());
        assert_eq!(state.theme, ThemePreset::CatppuccinLatte);
        assert!(state.apply(CockpitInput::FocusSpaces).is_none());
        assert!(state.apply(CockpitInput::Down).is_none());
        assert_eq!(state.selected_branch().as_deref(), Some("feat/tui"));
        let chosen = state.apply(CockpitInput::Enter).unwrap();
        assert_eq!(
            chosen,
            (
                CockpitAction::Build,
                Some("feat/tui".into()),
                CockpitProfile::Local,
                ThemePreset::CatppuccinLatte,
            )
        );
        let rendered = state.render(false, 120, 32);
        assert!(rendered.contains("SPACES"));
        assert!(rendered.contains("Commands"));
        assert!(rendered.contains("Appearance"));
        assert!(rendered.contains("vmux://"));
        assert!(rendered.contains("vmux build feat/tui -p local"));
        assert!(rendered.contains("catppuccin-latte"));
        assert!(rendered.contains("400.0 GiB"));
    }

    #[test]
    fn desktop_ctrl_b_chords_focus_panes_zoom_and_switch_profiles() {
        let mut state = CockpitState::new(
            vec![(
                "main".into(),
                Some("main".into()),
                "vmux".into(),
                true,
                false,
            )],
            None,
            ThemePreset::CatppuccinMocha,
        );
        assert_eq!(state.panel, CockpitPanel::Actions);
        assert!(state.apply(CockpitInput::Leader).is_none());
        assert!(state.leader_active);
        assert!(state.apply(CockpitInput::Left).is_none());
        assert!(!state.leader_active);
        assert_eq!(state.panel, CockpitPanel::Spaces);
        assert!(state.apply(CockpitInput::Leader).is_none());
        assert!(state.apply(CockpitInput::ToggleSidebar).is_none());
        assert!(!state.sidebar_open);
        assert_eq!(state.panel, CockpitPanel::Actions);
        assert!(state.apply(CockpitInput::Leader).is_none());
        assert!(state.apply(CockpitInput::ZoomPane).is_none());
        assert!(state.zoomed);
        let zoomed_view = state.render(false, 120, 32);
        assert!(zoomed_view.contains("zoom"));
        assert!(zoomed_view.contains("Commands"));
        assert!(!zoomed_view.contains("SPACES"));
        assert!(state.apply(CockpitInput::Leader).is_none());
        assert!(state.apply(CockpitInput::ConfirmNo).is_none());
        assert_eq!(state.profile, CockpitProfile::Local);
        assert!(state.apply(CockpitInput::Leader).is_none());
        assert!(state.apply(CockpitInput::Escape).is_none());
        assert!(!state.leader_active);
    }

    #[test]
    fn desktop_themes_round_trip_and_render_cards_cleanly_without_color() {
        for preset in ThemePreset::ALL {
            assert_eq!(ThemePreset::parse(preset.id()), Some(preset));
        }
        let theme = Theme::new(ThemePreset::TokyoNight, false, 72);
        let summary = theme.summary_card(
            "desktop",
            (Some("feat/tui"), Some("150435e")),
            Some("dev"),
            Some(Path::new("/tmp/target/debug/Vmux Dev.app")),
            Some(250 * GIB),
            Duration::from_millis(2400),
        );
        assert!(!summary.contains('\x1b'));
        assert!(summary.contains("feat/tui (150435e)"));
        assert!(summary.contains("250.0 GiB free"));
        assert!(summary.contains("2.4s"));
        let error = theme.error_card(
            "Insufficient build space",
            RecoveryHint::for_error("Insufficient build space"),
        );
        assert!(!error.contains('\x1b'));
        assert!(error.contains("try"));
        assert!(error.contains("vmux cache clean --apply"));
    }
}
