use std::rc::Rc;

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_ui::hooks::send;
use vmux_ui::platform::Platform;

use crate::event::{TabActivateRequest, TabDropPlacement, TabReorderRequest};

#[derive(Clone, PartialEq)]
struct TabDragState {
    source_id: String,
    source_index: usize,
    target_index: usize,
    order: Vec<String>,
    start_x: f64,
    start_y: f64,
    current_x: f64,
    active: bool,
}

#[derive(Clone, PartialEq)]
struct TabClickBlock {
    source_id: String,
    target_id: String,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct TabDragVisual {
    offset_x: f64,
    source: bool,
    active: bool,
}

const TAB_WIDTH_PX: f64 = 208.0;
const TAB_GAP_PX: f64 = 4.0;
const TAB_STEP_PX: f64 = TAB_WIDTH_PX + TAB_GAP_PX;

impl TabDragState {
    fn update_target(&mut self) {
        let slots = ((self.current_x - self.start_x) / TAB_STEP_PX).round() as isize;
        let last = self.order.len().saturating_sub(1) as isize;
        self.target_index = (self.source_index as isize + slots).clamp(0, last) as usize;
    }

    fn source_offset(&self) -> f64 {
        let min = -(self.source_index as f64) * TAB_STEP_PX;
        let max = self.order.len().saturating_sub(self.source_index + 1) as f64 * TAB_STEP_PX;
        (self.current_x - self.start_x).clamp(min, max)
    }

    fn offset_for(&self, index: usize) -> f64 {
        if index == self.source_index {
            return self.source_offset();
        }
        if self.source_index < self.target_index
            && index > self.source_index
            && index <= self.target_index
        {
            return -TAB_STEP_PX;
        }
        if self.target_index < self.source_index
            && index >= self.target_index
            && index < self.source_index
        {
            return TAB_STEP_PX;
        }
        0.0
    }
}

impl TabDragVisual {
    fn resolve(state: Option<&TabDragState>, tab_id: &str, index: usize) -> Self {
        let Some(state) = state.filter(|state| state.active) else {
            return Self::default();
        };
        Self {
            offset_x: state.offset_for(index),
            source: state.source_id == tab_id,
            active: true,
        }
    }

    pub(super) fn style(self) -> String {
        if !self.active || (!self.source && self.offset_x.abs() < f64::EPSILON) {
            return "transform:none;z-index:auto;pointer-events:auto;transition:none;".to_string();
        }
        if self.source {
            return format!(
                "transform:translate3d({}px,0,0);z-index:20;pointer-events:none;transition:none;",
                self.offset_x
            );
        }
        format!(
            "transform:translate3d({}px,0,0);z-index:auto;pointer-events:auto;transition:transform {}ms ease-out;",
            self.offset_x,
            vmux_ui::motion::Motion::FEEDBACK_MS
        )
    }

    pub(super) fn active(self) -> bool {
        self.active
    }
}

#[derive(Clone, PartialEq)]
struct PendingSelection {
    id: String,
    previous: Option<String>,
}

impl PendingSelection {
    fn resolved(&self, active: Option<&str>, order: &[String]) -> bool {
        active == Some(self.id.as_str())
            || active != self.previous.as_deref()
            || !order.contains(&self.id)
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct TabDrag {
    state: Signal<Option<Rc<TabDragState>>>,
    click_block: Signal<Option<TabClickBlock>>,
    order: Memo<Vec<String>>,
    active: Memo<Option<String>>,
    selection: Signal<Option<PendingSelection>>,
}

impl TabDrag {
    pub(crate) fn use_state(order: Memo<Vec<String>>, active: Memo<Option<String>>) -> Self {
        let mut selection = use_signal(|| None::<PendingSelection>);
        use_effect(move || {
            let active = active();
            let order = order();
            if selection
                .peek()
                .as_ref()
                .is_some_and(|pending| pending.resolved(active.as_deref(), &order))
            {
                selection.set(None);
            }
        });
        Self {
            state: use_signal(|| None::<Rc<TabDragState>>),
            click_block: use_signal(|| None),
            order,
            active,
            selection,
        }
    }

    pub(crate) fn is_selected(self, id: &str, projected: bool) -> bool {
        match self.selection.read().as_ref() {
            Some(pending) => pending.id == id,
            None => projected,
        }
    }

    pub(crate) fn press(
        &mut self,
        event: &Event<PointerData>,
        id: String,
        index: usize,
        active: bool,
    ) {
        if event.trigger_button() != Some(MouseButton::Primary) {
            return;
        }
        if active {
            self.begin(event, id, index, true);
            return;
        }
        event.prevent_default();
        self.activate(id.clone());
        self.click_block.set(Some(TabClickBlock {
            source_id: id.clone(),
            target_id: id,
        }));
    }

    pub(crate) fn metrics_style() -> String {
        format!("--tab-width:{TAB_WIDTH_PX}px;--tab-gap:{TAB_GAP_PX}px;")
    }

    pub(crate) fn listeners(self) -> Vec<Attribute> {
        if self.state.read().is_none() {
            return Vec::new();
        }
        let mut advancing = self;
        let mut finishing = self;
        let mut cancelling = self;
        let mut leaving = self;
        vec![
            dioxus_elements::events::onpointermove(move |event| advancing.advance(&event)),
            dioxus_elements::events::onpointerup(move |event| finishing.finish(&event)),
            dioxus_elements::events::onpointercancel(move |_| cancelling.cancel()),
            dioxus_elements::events::onpointerleave(move |_| leaving.cancel()),
        ]
    }

    pub(crate) fn begin(
        &mut self,
        event: &Event<PointerData>,
        source_id: String,
        source_index: usize,
        draggable: bool,
    ) {
        if !draggable || event.trigger_button() != Some(MouseButton::Primary) {
            return;
        }
        event.prevent_default();
        self.click_block.set(None);
        let point = event.client_coordinates();
        let order = (self.order)();
        let source_index = order
            .iter()
            .position(|id| id == &source_id)
            .unwrap_or(source_index);
        self.state.set(Some(Rc::new(TabDragState {
            source_id,
            source_index,
            target_index: source_index,
            order,
            start_x: point.x,
            start_y: point.y,
            current_x: point.x,
            active: false,
        })));
    }

    fn advance(&mut self, event: &Event<PointerData>) {
        let Some(mut state) = (self.state)().as_deref().cloned() else {
            return;
        };
        let point = event.client_coordinates();
        let dx = point.x - state.start_x;
        let dy = point.y - state.start_y;
        state.current_x = point.x;
        state.update_target();
        if !state.active && dx * dx + dy * dy < 16.0 {
            return;
        }
        if !state.active {
            state.active = true;
        }
        self.state.set(Some(Rc::new(state)));
    }

    fn finish(&mut self, event: &Event<PointerData>) {
        let Some(state) = (self.state)() else {
            return;
        };
        if !state.active {
            self.state.set(None);
            return;
        }
        event.prevent_default();
        event.stop_propagation();
        let target_id = state
            .order
            .get(state.target_index)
            .cloned()
            .unwrap_or_else(|| state.source_id.clone());
        if state.source_index != state.target_index {
            let _ = send(&TabReorderRequest {
                tab_id: state.source_id.clone(),
                target_tab_id: target_id.clone(),
                drop_placement: if state.target_index < state.source_index {
                    TabDropPlacement::Before
                } else {
                    TabDropPlacement::After
                },
            });
        }
        let block = TabClickBlock {
            source_id: state.source_id.clone(),
            target_id,
        };
        self.click_block.set(Some(block.clone()));
        self.state.set(None);
        let mut click_block = self.click_block;
        spawn(async move {
            Platform::sleep(100).await;
            if click_block() == Some(block) {
                click_block.set(None);
            }
        });
    }

    fn cancel(&mut self) {
        self.state.set(None);
    }

    pub(crate) fn blocks_click(&mut self, tab_id: &str) -> bool {
        let Some(block) = (self.click_block)() else {
            return false;
        };
        if block.source_id != tab_id && block.target_id != tab_id {
            return false;
        }
        self.click_block.set(None);
        true
    }

    pub(super) fn visual(self, tab_id: &str, index: usize) -> TabDragVisual {
        let state = (self.state)();
        TabDragVisual::resolve(state.as_deref(), tab_id, index)
    }

    pub(crate) fn activate(&mut self, tab_id: String) {
        if self
            .selection
            .peek()
            .as_ref()
            .is_some_and(|pending| pending.id == tab_id)
        {
            return;
        }
        let pending = PendingSelection {
            id: tab_id.clone(),
            previous: (self.active)(),
        };
        self.selection.set(Some(pending.clone()));
        let mut selection = self.selection;
        spawn(async move {
            Platform::sleep(1000).await;
            if selection.peek().as_ref() == Some(&pending) {
                selection.set(None);
            }
        });
        let _ = send(&TabActivateRequest { tab_id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_shifts_tabs_between_source_and_target() {
        let mut state = TabDragState {
            source_id: "a".into(),
            source_index: 0,
            target_index: 0,
            order: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            start_x: 100.0,
            start_y: 0.0,
            current_x: 100.0 + TAB_STEP_PX * 2.1,
            active: true,
        };

        state.update_target();

        assert_eq!(state.target_index, 2);
        assert_eq!(state.offset_for(1), -TAB_STEP_PX);
        assert_eq!(state.offset_for(2), -TAB_STEP_PX);
        assert_eq!(state.offset_for(3), 0.0);
    }

    #[test]
    fn drag_clamps_to_available_slots() {
        let mut state = TabDragState {
            source_id: "c".into(),
            source_index: 2,
            target_index: 2,
            order: vec!["a".into(), "b".into(), "c".into()],
            start_x: 100.0,
            start_y: 0.0,
            current_x: -1000.0,
            active: true,
        };

        state.update_target();

        assert_eq!(state.target_index, 0);
        assert_eq!(state.offset_for(0), TAB_STEP_PX);
        assert_eq!(state.offset_for(1), TAB_STEP_PX);
        assert_eq!(state.source_offset(), -TAB_STEP_PX * 2.0);
    }
}
