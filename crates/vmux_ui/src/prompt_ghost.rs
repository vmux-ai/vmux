//! The example prompt that types itself into an empty composer.
//!
//! The animation is a pure state machine — [`PromptTypewriter`] — with one platform-shaped hole:
//! it needs a random index to move on to and cannot pick one itself. [`crate::platform`] fills
//! that in, along with the timer that ticks it, so the component itself is the same everywhere.

#[cfg(ui)]
pub use component::PromptGhost;

/// One example prompt being typed out, and how far through it the animation is.
///
/// [`advance`](Self::advance) is the whole animation, and the candidate index it takes is
/// everything it needs from the outside — so the state stays plain, total and testable on any
/// target, and `js_sys::Math::random` stays at the edge that has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptTypewriter {
    examples: &'static [&'static str],
    index: usize,
    typed: usize,
}

impl PromptTypewriter {
    /// How long a finished line holds before the next one starts, in ticks.
    const PAUSE_TICKS: usize = 40;

    /// Start on `candidate`, clamped into range.
    pub fn new(examples: &'static [&'static str], candidate: usize) -> Self {
        Self {
            examples,
            index: Self::distinct_index(examples.len(), None, candidate),
            typed: 0,
        }
    }

    /// The prefix to render this tick.
    pub fn shown(&self) -> String {
        let example = self.example();
        let full = example.chars().count();
        example.chars().take(self.typed.min(full)).collect()
    }

    /// One tick: another character, or — once the finished line has held for [`Self::PAUSE_TICKS`]
    /// — the start of `candidate`.
    ///
    /// `candidate` is taken every tick and used only on the one that wraps, which keeps this a
    /// function of its arguments rather than of a generator the caller has to thread through.
    pub fn advance(&mut self, candidate: usize) {
        let full = self.example().chars().count();
        if self.typed < full + Self::PAUSE_TICKS {
            self.typed += 1;
            return;
        }
        self.typed = 0;
        self.index = Self::distinct_index(self.examples.len(), Some(self.index), candidate);
    }

    fn example(&self) -> &'static str {
        self.examples[self.index]
    }

    /// Never the line already showing, so a reroll visibly changes something.
    fn distinct_index(len: usize, current: Option<usize>, candidate: usize) -> usize {
        if len <= 1 {
            return 0;
        }
        let next = candidate.min(len - 1);
        if current == Some(next) {
            (next + 1) % len
        } else {
            next
        }
    }
}

pub const AGENT_PROMPT_EXAMPLES: &[&str] = &[
    "Find me the best flight from Paris to Tokyo next month",
    "Find a quiet hotel with AC near central Paris for this weekend",
    "Plan a five-day food and culture trip through Kyoto",
    "Build me a relaxed weekend itinerary for Lisbon",
    "Compare rail passes for a two-week trip around Europe",
    "Find highly rated restaurants nearby with vegetarian options",
    "Research the visa requirements for my next international trip",
    "Create a lightweight packing list for a ten-day winter trip",
    "Plan a scenic road trip from San Francisco to Portland",
    "Compare the best neighborhoods for a month-long stay in Tokyo",
    "Find quiet coworking spaces with day passes and fast Wi-Fi",
    "Plan a memorable surprise birthday weekend on a sensible budget",
    "Build a healthy weekly meal plan and shopping list for two",
    "Turn this grocery budget into affordable meals for the week",
    "Create a beginner workout plan I can do at home in 30 minutes",
    "Make a six-week study plan for conversational Japanese",
    "Compare the best noise-canceling headphones under $300",
    "Find an ergonomic standing desk setup for a small apartment",
    "Research the best compact camera for travel and street photography",
    "Compare lightweight laptops for coding, travel, and battery life",
    "Summarize these PDFs and extract the decisions and action items",
    "Turn my meeting notes into a clear project plan with owners",
    "Draft a concise follow-up email from these scattered notes",
    "Organize my Downloads folder into a clean, useful structure",
    "Find duplicate photos and help me safely clean them up",
    "Analyze this CSV and explain the most important trends",
    "Turn these receipts into a categorized expense report",
    "Build a landing site for my new restaurant — make it themeable",
    "Prototype a clean dashboard from this product brief",
    "Debug the failing tests and explain the root cause",
    "Explain how this codebase works and where I should start",
    "Refactor this module without changing its behavior",
    "Add the requested feature and verify the important edge cases",
    "Review my staged changes for bugs, security, and maintainability",
    "Open a PR for my staged changes",
    "Generate release notes from the changes since the last version",
    "Investigate these application logs and find the likely failure",
    "Find the performance bottleneck and propose the smallest fix",
    "Update outdated dependencies and resolve compatibility issues",
    "Set up this project locally and verify the development workflow",
    "Automate this repetitive workflow with a reliable script",
    "Research my competitors and summarize their positioning",
    "Create a launch plan for this product with milestones and risks",
    "Turn this rough brief into a prioritized execution checklist",
    "Find the latest reliable information and summarize the sources",
    "Compare my subscriptions and identify easy ways to save money",
    "Design a comfortable home office setup for a tight space",
    "Create a realistic monthly budget from these transactions",
];

pub const TERMINAL_PROMPT_EXAMPLES: &[&str] = &[
    "git status --short",
    "rg \"TODO|FIXME\" .",
    "find . -type f -size +100M",
    "git log --oneline -10",
];

#[cfg(ui)]
mod component {
    use dioxus::prelude::*;

    use super::{AGENT_PROMPT_EXAMPLES, PromptTypewriter, TERMINAL_PROMPT_EXAMPLES};
    use crate::platform::{random_index, sleep_ms};

    const PROMPT_CARET_CSS: &str = ".vmux-prompt-caret{animation:vmux-prompt-caret-blink 1s step-end infinite}@keyframes vmux-prompt-caret-blink{0%,49%{opacity:1}50%,100%{opacity:0}}";

    /// One character typed, or one fortieth of the hold at the end of a line.
    const TICK_MS: u32 = 50;

    #[component]
    pub fn PromptGhost(accent_bg: String, terminal: bool) -> Element {
        let examples = if terminal {
            TERMINAL_PROMPT_EXAMPLES
        } else {
            AGENT_PROMPT_EXAMPLES
        };
        let mut typewriter =
            use_signal(|| PromptTypewriter::new(examples, random_index(examples.len())));

        // A future rather than an interval, which is what lets the whole teardown go: dioxus drops
        // this when the component unmounts, where a `setInterval` had to be cancelled by hand and
        // its closure kept alive until it was.
        use_future(move || async move {
            loop {
                sleep_ms(TICK_MS).await;
                let mut next = *typewriter.peek();
                next.advance(random_index(examples.len()));
                typewriter.set(next);
            }
        });

        let shown = typewriter().shown();
        let ghost_class = if terminal {
            "w-80 whitespace-pre-wrap break-words font-mono text-sm text-muted-foreground/50"
        } else {
            "flex max-w-full items-center whitespace-nowrap text-[15px] leading-6 text-muted-foreground/50"
        };
        let caret_class = if terminal {
            format!("vmux-prompt-caret ml-px inline-block h-3.5 w-1.5 align-middle {accent_bg}")
        } else {
            format!("vmux-prompt-caret ml-px h-5 w-px shrink-0 {accent_bg}")
        };
        rsx! {
            style { dangerous_inner_html: PROMPT_CARET_CSS }
            div {
                class: "{ghost_class}",
                span { class: if terminal { "" } else { "min-w-0 truncate" }, "{shown}" }
                span { class: "{caret_class}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLES: &[&str] = &["abc", "defgh"];

    #[test]
    fn prompt_example_index_never_repeats_current() {
        for current in 0..4 {
            assert_ne!(
                PromptTypewriter::distinct_index(4, Some(current), current),
                current
            );
        }
    }

    #[test]
    fn prompt_typewriter_resets_after_pause() {
        let mut typewriter = PromptTypewriter::new(EXAMPLES, 0);
        let full = EXAMPLES[0].chars().count();
        for _ in 0..full + PromptTypewriter::PAUSE_TICKS {
            typewriter.advance(0);
        }
        assert_eq!(typewriter.shown(), "abc");

        typewriter.advance(1);
        assert_eq!(typewriter.shown(), "");
        assert_eq!(typewriter.index, 1);
    }
}
