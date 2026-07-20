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

#[cfg(any(test, target_arch = "wasm32"))]
const PROMPT_PAUSE_TICKS: usize = 40;

#[cfg(any(test, target_arch = "wasm32"))]
fn distinct_prompt_example_index(len: usize, current: Option<usize>, candidate: usize) -> usize {
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

#[cfg(any(test, target_arch = "wasm32"))]
fn next_prompt_typed_count(typed: usize, full: usize) -> Option<usize> {
    (typed < full + PROMPT_PAUSE_TICKS).then_some(typed + 1)
}

#[cfg(target_arch = "wasm32")]
mod component {
    use std::cell::RefCell;
    use std::rc::Rc;

    use dioxus::prelude::*;
    use wasm_bindgen::{JsCast, closure::Closure};

    use super::{
        AGENT_PROMPT_EXAMPLES, TERMINAL_PROMPT_EXAMPLES, distinct_prompt_example_index,
        next_prompt_typed_count,
    };

    const PROMPT_CARET_CSS: &str = ".vmux-prompt-caret{animation:vmux-prompt-caret-blink 1s step-end infinite}@keyframes vmux-prompt-caret-blink{0%,49%{opacity:1}50%,100%{opacity:0}}";
    type PromptTimerCallback = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;

    #[component]
    pub fn PromptGhost(accent_bg: String, terminal: bool) -> Element {
        let examples = if terminal {
            TERMINAL_PROMPT_EXAMPLES
        } else {
            AGENT_PROMPT_EXAMPLES
        };
        let ex_idx = use_signal(|| random_prompt_example_index(examples.len(), None));
        let typed = use_signal(|| 0usize);
        let cb: PromptTimerCallback = use_hook(|| Rc::new(RefCell::new(None)));
        let timer: Rc<RefCell<Option<i32>>> = use_hook(|| Rc::new(RefCell::new(None)));
        use_effect({
            let cb = cb.clone();
            let timer = timer.clone();
            move || start_prompt_typewriter(examples, ex_idx, typed, cb.clone(), timer.clone())
        });
        use_drop({
            let cb = cb.clone();
            let timer = timer.clone();
            move || {
                if let Some(id) = timer.borrow_mut().take()
                    && let Some(win) = web_sys::window()
                {
                    win.clear_interval_with_handle(id);
                }
                *cb.borrow_mut() = None;
            }
        });
        let example = examples[ex_idx() % examples.len()];
        let full = example.chars().count();
        let shown: String = example.chars().take(typed().min(full)).collect();
        let ghost_class = if terminal {
            "w-80 whitespace-pre-wrap break-words font-mono text-sm text-muted-foreground/50"
        } else {
            "flex max-w-full items-center whitespace-nowrap text-[15px] leading-6 text-muted-foreground/50"
        };
        let caret_class = if terminal {
            format!("vmux-prompt-caret ml-px inline-block h-3.5 w-1.5 align-middle {accent_bg}")
        } else {
            format!("vmux-prompt-caret relative top-px ml-px h-4 w-1.5 shrink-0 {accent_bg}")
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

    fn random_prompt_example_index(len: usize, current: Option<usize>) -> usize {
        let candidate = (js_sys::Math::random() * len as f64) as usize;
        distinct_prompt_example_index(len, current, candidate)
    }

    fn start_prompt_typewriter(
        examples: &'static [&'static str],
        mut ex_idx: Signal<usize>,
        mut typed: Signal<usize>,
        cb_cell: PromptTimerCallback,
        timer_cell: Rc<RefCell<Option<i32>>>,
    ) {
        let cb = Closure::wrap(Box::new(move || {
            let idx = *ex_idx.peek();
            let full = examples[idx % examples.len()].chars().count();
            let t = *typed.peek();
            if let Some(next) = next_prompt_typed_count(t, full) {
                typed.set(next);
            } else {
                typed.set(0);
                ex_idx.set(random_prompt_example_index(examples.len(), Some(idx)));
            }
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

#[cfg(target_arch = "wasm32")]
pub use component::PromptGhost;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_example_index_never_repeats_current() {
        for current in 0..4 {
            assert_ne!(
                distinct_prompt_example_index(4, Some(current), current),
                current
            );
        }
    }

    #[test]
    fn prompt_typewriter_resets_after_pause() {
        let full = 12;
        assert_eq!(
            next_prompt_typed_count(full + PROMPT_PAUSE_TICKS - 1, full),
            Some(full + PROMPT_PAUSE_TICKS)
        );
        assert_eq!(
            next_prompt_typed_count(full + PROMPT_PAUSE_TICKS, full),
            None
        );
    }
}
