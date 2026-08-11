//! The example prompt that types itself into an empty composer.
//!
//! The animation is a pure state machine — [`PromptTypewriter`] — with one platform-shaped hole:
//! it needs a random index to move on to and cannot pick one itself. The frontend that has a
//! random source and a timer fills that in, which is the only part of this file the web build owns.

#[cfg(web)]
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

#[cfg(web)]
mod component {
    use std::cell::RefCell;
    use std::rc::Rc;

    use dioxus::prelude::*;
    use wasm_bindgen::{JsCast, closure::Closure};

    use super::{AGENT_PROMPT_EXAMPLES, PromptTypewriter, TERMINAL_PROMPT_EXAMPLES};
    use crate::platform::random_index;

    const PROMPT_CARET_CSS: &str = ".vmux-prompt-caret{animation:vmux-prompt-caret-blink 1s step-end infinite}.vmux-prompt-caret-paused{animation-play-state:paused}@keyframes vmux-prompt-caret-blink{0%,49%{opacity:1}50%,100%{opacity:0}}";
    type PromptTimerCallback = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;
    type ActivityCallback = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;

    #[component]
    pub fn PromptGhost(accent_bg: String, terminal: bool) -> Element {
        let examples = if terminal {
            TERMINAL_PROMPT_EXAMPLES
        } else {
            AGENT_PROMPT_EXAMPLES
        };
        let typewriter =
            use_signal(|| PromptTypewriter::new(examples, random_index(examples.len())));
        let cb: PromptTimerCallback = use_hook(|| Rc::new(RefCell::new(None)));
        let timer: Rc<RefCell<Option<i32>>> = use_hook(|| Rc::new(RefCell::new(None)));
        let mut active = use_signal(document_active);
        let activity_cb: ActivityCallback = use_hook(|| Rc::new(RefCell::new(None)));
        use_effect({
            let activity_cb = activity_cb.clone();
            move || {
                let callback = Closure::wrap(
                    Box::new(move || active.set(document_active())) as Box<dyn FnMut()>
                );
                if let Some(window) = web_sys::window() {
                    let window_target: &web_sys::EventTarget = window.as_ref();
                    let _ = window_target.add_event_listener_with_callback(
                        "focus",
                        callback.as_ref().unchecked_ref(),
                    );
                    let _ = window_target.add_event_listener_with_callback(
                        "blur",
                        callback.as_ref().unchecked_ref(),
                    );
                    if let Some(document) = window.document() {
                        let document_target: &web_sys::EventTarget = document.as_ref();
                        let _ = document_target.add_event_listener_with_callback(
                            "focusin",
                            callback.as_ref().unchecked_ref(),
                        );
                        let _ = document_target.add_event_listener_with_callback(
                            "focusout",
                            callback.as_ref().unchecked_ref(),
                        );
                    }
                }
                if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                    let _ = document.add_event_listener_with_callback(
                        "visibilitychange",
                        callback.as_ref().unchecked_ref(),
                    );
                }
                *activity_cb.borrow_mut() = Some(callback);
            }
        });
        use_effect({
            let cb = cb.clone();
            let timer = timer.clone();
            move || {
                stop_prompt_typewriter(cb.clone(), timer.clone());
                if active() {
                    start_prompt_typewriter(examples, typewriter, cb.clone(), timer.clone());
                }
            }
        });
        use_drop({
            let cb = cb.clone();
            let timer = timer.clone();
            let activity_cb = activity_cb.clone();
            move || {
                stop_prompt_typewriter(cb.clone(), timer.clone());
                if let Some(callback) = activity_cb.borrow_mut().take()
                    && let Some(window) = web_sys::window()
                {
                    let window_target: &web_sys::EventTarget = window.as_ref();
                    let _ = window_target.remove_event_listener_with_callback(
                        "focus",
                        callback.as_ref().unchecked_ref(),
                    );
                    let _ = window_target.remove_event_listener_with_callback(
                        "blur",
                        callback.as_ref().unchecked_ref(),
                    );
                    if let Some(document) = window.document() {
                        let document_target: &web_sys::EventTarget = document.as_ref();
                        let _ = document_target.remove_event_listener_with_callback(
                            "focusin",
                            callback.as_ref().unchecked_ref(),
                        );
                        let _ = document_target.remove_event_listener_with_callback(
                            "focusout",
                            callback.as_ref().unchecked_ref(),
                        );
                        let _ = document_target.remove_event_listener_with_callback(
                            "visibilitychange",
                            callback.as_ref().unchecked_ref(),
                        );
                    }
                }
            }
        });
        let shown = typewriter().shown();
        let ghost_class = if terminal {
            "w-80 whitespace-pre-wrap break-words font-mono text-sm text-muted-foreground/50"
        } else {
            "flex max-w-full items-center whitespace-nowrap text-[15px] leading-6 text-muted-foreground/50"
        };
        let caret_state = if active() {
            ""
        } else {
            " vmux-prompt-caret-paused"
        };
        let caret_class = if terminal {
            format!(
                "vmux-prompt-caret{caret_state} ml-px inline-block h-3.5 w-1.5 align-middle {accent_bg}"
            )
        } else {
            format!("vmux-prompt-caret{caret_state} ml-px h-5 w-px shrink-0 {accent_bg}")
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

    fn document_visible() -> bool {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| {
                js_sys::Reflect::get(
                    document.as_ref(),
                    &wasm_bindgen::JsValue::from_str("hidden"),
                )
                .ok()
                .and_then(|hidden| hidden.as_bool())
            })
            .is_none_or(|hidden| !hidden)
    }

    fn document_active() -> bool {
        document_visible()
            && web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.has_focus().ok())
                .unwrap_or(false)
    }

    fn stop_prompt_typewriter(cb_cell: PromptTimerCallback, timer_cell: Rc<RefCell<Option<i32>>>) {
        if let Some(id) = timer_cell.borrow_mut().take()
            && let Some(window) = web_sys::window()
        {
            window.clear_interval_with_handle(id);
        }
        *cb_cell.borrow_mut() = None;
    }

    fn start_prompt_typewriter(
        examples: &'static [&'static str],
        mut typewriter: Signal<PromptTypewriter>,
        cb_cell: PromptTimerCallback,
        timer_cell: Rc<RefCell<Option<i32>>>,
    ) {
        let cb = Closure::wrap(Box::new(move || {
            let mut next = *typewriter.peek();
            next.advance(random_index(examples.len()));
            typewriter.set(next);
        }) as Box<dyn FnMut()>);
        if let Some(win) = web_sys::window()
            && let Ok(id) = win.set_interval_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                50,
            )
        {
            *timer_cell.borrow_mut() = Some(id);
        }
        *cb_cell.borrow_mut() = Some(cb);
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
