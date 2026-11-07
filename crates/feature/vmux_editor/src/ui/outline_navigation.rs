use dioxus::prelude::*;
use vmux_api::page_location::PageLocationUiState;
use vmux_ecs::event::{ExplorerGoto, NoteBlock, OutlineRow};
use vmux_ui::hooks::{send, use_ui_state};
use vmux_ui::scroll::{ScrollIntoView, SectionNavigation};

use super::SCROLL_ID;
use super::note::NoteBlocks;

#[derive(Clone, Copy)]
pub(super) struct OutlineNavigation {
    sections: SectionNavigation,
    note: Memo<bool>,
    blocks: Memo<Vec<NoteBlock>>,
    rows: Memo<Vec<OutlineRow>>,
    path: Memo<String>,
    line: Signal<Option<u32>>,
    measuring: Signal<bool>,
    pending: Signal<bool>,
}

impl OutlineNavigation {
    pub(super) fn use_state(
        note: Memo<bool>,
        blocks: Memo<Vec<NoteBlock>>,
        rows: Memo<Vec<OutlineRow>>,
        path: Memo<String>,
    ) -> Self {
        let navigation = Self {
            sections: SectionNavigation::use_state(),
            note,
            blocks,
            rows,
            path,
            line: use_signal(|| None),
            measuring: use_signal(|| false),
            pending: use_signal(|| false),
        };
        let location = use_ui_state::<PageLocationUiState>().state;
        let mut scope = use_signal(String::new);
        use_effect(move || {
            let path = path();
            if *scope.peek() == path {
                return;
            }
            scope.set(path);
            navigation.sections.reset();
            let mut line = navigation.line;
            line.set(None);
        });
        let mut applied = use_signal(String::new);
        use_effect(move || {
            let request = (navigation.sections.reveal)();
            note.read();
            blocks.read();
            let key = format!("{}\0{}\0{}", path(), request.revision, request.target);
            if request.target.is_empty() || *applied.peek() == key {
                return;
            }
            applied.set(key);
            if note() {
                ScrollIntoView::section(SCROLL_ID, &request.target, request.behavior);
            }
        });
        use_effect(move || {
            let fragment = location.read().fragment.clone();
            path.read();
            note.read();
            blocks.read();
            if let Some(line) = Self::fragment_line(&fragment) {
                navigation.receive(line, &fragment);
            } else if fragment.is_empty() {
                navigation.sections.receive("", String::new());
            }
        });
        use_effect(move || {
            note.read();
            rows.read();
            blocks.read();
            path.read();
            navigation.measure();
        });
        use_context_provider(|| navigation);
        navigation
    }

    pub(super) fn current_line(self) -> Option<u32> {
        (self.line)()
    }

    pub(super) fn select(mut self, line: u32) {
        let Some(target) = self.target(line) else {
            return;
        };
        self.line.set(Some(line));
        self.sections
            .select(target, format!("L{}", line.saturating_add(1)));
        if !(self.note)() {
            self.goto(line);
        }
    }

    fn receive(mut self, line: u32, fragment: &str) {
        let Some(target) = self.target(line) else {
            return;
        };
        if !self.sections.receive(fragment, target) {
            return;
        }
        self.line.set(Some(line));
        if !(self.note)() {
            self.goto(line);
        }
    }

    fn target(self, line: u32) -> Option<String> {
        if !(self.note)() {
            return Some(format!("file-line-{line}"));
        }
        let blocks = self.blocks.read();
        let index = blocks.as_slice().block_index_for_line(line)?;
        Some(format!("note-block-{index}"))
    }

    fn goto(self, line: u32) {
        let _ = send(&ExplorerGoto {
            path: String::new(),
            line,
        });
    }

    pub(super) fn follow(mut self, line: u32) {
        let mut section = None;
        for row in self.rows.read().iter() {
            if row.contains(line) {
                section = Some(row.line);
            }
        }
        let Some(section) = section else {
            return;
        };
        if *self.line.peek() != Some(line) {
            self.line.set(Some(line));
        }
        if let Some(target) = self.target(section) {
            self.sections
                .follow(target, format!("L{}", section.saturating_add(1)));
        }
    }

    pub(super) fn measure(mut self) {
        if !(self.note)() {
            return;
        }
        if *self.measuring.peek() {
            self.pending.set(true);
            return;
        }
        self.measuring.set(true);
        let path = (self.path)();
        spawn(async move {
            let line = ScrollIntoView::section_at_top(SCROLL_ID).await;
            self.measuring.set(false);
            if (self.path)() == path
                && (self.note)()
                && let Some(line) = line
            {
                self.follow(line);
            }
            if *self.pending.peek() {
                self.pending.set(false);
                self.measure();
            }
        });
    }

    fn fragment_line(fragment: &str) -> Option<u32> {
        fragment
            .strip_prefix('L')?
            .split([':', '-'])
            .next()?
            .parse::<u32>()
            .ok()?
            .checked_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{Mutation, VirtualDom};
    use dioxus::html::{EventData, SerializedMouseData, SerializedScrollData};
    use std::{
        cell::{Cell, RefCell},
        future::Future,
        rc::Rc,
    };
    use vmux_api::{BinEvent, UiStateFrame};
    use vmux_ecs::event::{MdBlock, MdInline};
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostScope, PageHost, TextOffsetAnswer};

    #[derive(Default)]
    struct NoteHost {
        section: Cell<u32>,
        locations: RefCell<Vec<String>>,
        scrolls: RefCell<Vec<(String, ScrollBehavior)>>,
        listeners: RefCell<Vec<BytesListener>>,
    }

    impl PageHost for NoteHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            assert_ne!(
                id,
                ExplorerGoto::id(),
                "following a Note must preserve the caret"
            );
            if id == vmux_api::page_location::PageLocationRequest::id() {
                let request = rkyv::from_bytes::<
                    vmux_api::page_location::PageLocationRequest,
                    rkyv::rancor::Error,
                >(bytes)
                .unwrap();
                self.locations.borrow_mut().push(request.fragment);
            }
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            if id == PageLocationUiState::id() {
                self.listeners.borrow_mut().push(listener);
            }
            Ok(())
        }

        fn section_at_top(&self, _: &str) -> TextOffsetAnswer {
            Box::pin(std::future::ready(Some(self.section.get())))
        }

        fn scroll_section(&self, container: &str, element: &str, behavior: ScrollBehavior) {
            assert_eq!(container, SCROLL_ID);
            self.scrolls
                .borrow_mut()
                .push((element.to_string(), behavior));
        }
    }

    #[derive(Clone, Default)]
    struct NavigationProbe(Rc<RefCell<Option<OutlineNavigation>>>);

    #[component]
    fn OutlineNote() -> Element {
        let note = use_memo(|| true);
        let blocks = use_memo(|| {
            let mut blocks = Vec::new();
            for line in [0, 10, 20] {
                blocks.push(NoteBlock {
                    start_line: line,
                    end_line: line + 1,
                    source: format!("# Section {line}"),
                    block: MdBlock::Heading {
                        level: 1,
                        inlines: vec![MdInline::Text(format!("Section {line}"))],
                    },
                });
            }
            blocks
        });
        let rows = use_memo(|| {
            [0, 10, 20]
                .into_iter()
                .map(|line| OutlineRow {
                    name: format!("Section {line}"),
                    kind: 12,
                    line,
                    end_line: if line == 20 {
                        OutlineRow::OPEN_END
                    } else {
                        line + 9
                    },
                    depth: 0,
                })
                .collect()
        });
        let path = use_memo(|| "/note.md".to_string());
        let navigation = OutlineNavigation::use_state(note, blocks, rows, path);
        *use_context::<NavigationProbe>().0.borrow_mut() = Some(navigation);
        rsx! {
            div { id: SCROLL_ID, onscroll: move |_| navigation.measure(),
                button { onclick: move |_| navigation.select(10), "Section 10" }
            }
        }
    }

    struct NoteHarness;

    impl NoteHarness {
        fn settle(dom: &mut VirtualDom) {
            for _ in 0..8 {
                let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                let _ = std::pin::pin!(dom.wait_for_work()).poll(&mut context);
                dom.render_immediate_to_vec();
            }
        }

        fn location(dom: &mut VirtualDom, host: &NoteHost, fragment: &str) {
            let frame = UiStateFrame::<PageLocationUiState, PageLocationUiState>::Snapshot(
                PageLocationUiState {
                    fragment: fragment.to_string(),
                },
            );
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            for listener in host.listeners.borrow_mut().iter_mut() {
                listener(&bytes);
            }
            Self::settle(dom);
        }
    }

    #[test]
    fn note_outline_follows_scrolling_and_link_echoes_do_not_restart_the_scroll() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let host = Rc::new(NoteHost::default());
        host.section.set(10);
        let _host = HostScope::enter(host.clone());
        let probe = NavigationProbe::default();
        let mut dom = VirtualDom::new(OutlineNote);
        dom.provide_root_context(probe.clone());
        let initial = dom.rebuild_to_vec();
        let mut scroll = None;
        let mut click = None;
        for edit in initial.edits {
            if let Mutation::NewEventListener { name, id } = edit {
                if name == "scroll" {
                    scroll = Some(id);
                } else if name == "click" {
                    click = Some(id);
                }
            }
        }
        NoteHarness::settle(&mut dom);
        assert_eq!(probe.0.borrow().unwrap().current_line(), Some(10));
        host.locations.borrow_mut().clear();
        host.section.set(20);
        let event = EventData::Scroll(SerializedScrollData {
            scroll_top: 600.0,
            scroll_left: 0.0,
            scroll_width: 800,
            scroll_height: 1200,
            client_width: 800,
            client_height: 400,
        });
        dom.runtime().handle_event(
            "scroll",
            Event::new(event.into_any(), false),
            scroll.unwrap(),
        );
        NoteHarness::settle(&mut dom);
        assert_eq!(probe.0.borrow().unwrap().current_line(), Some(20));
        assert_eq!(host.locations.borrow().as_slice(), ["L21"]);
        NoteHarness::location(&mut dom, &host, "L21");
        assert!(host.scrolls.borrow().is_empty());
        for _ in 0..2 {
            let event = EventData::Mouse(SerializedMouseData::default());
            dom.runtime()
                .handle_event("click", Event::new(event.into_any(), true), click.unwrap());
            NoteHarness::settle(&mut dom);
        }
        assert_eq!(
            host.scrolls.borrow().as_slice(),
            [
                ("note-block-1".to_string(), ScrollBehavior::Smooth),
                ("note-block-1".to_string(), ScrollBehavior::Smooth),
            ]
        );
        NoteHarness::location(&mut dom, &host, "L11");
        assert_eq!(host.scrolls.borrow().len(), 2);
        NoteHarness::location(&mut dom, &host, "L1");
        assert_eq!(
            host.scrolls.borrow().last(),
            Some(&("note-block-0".to_string(), ScrollBehavior::Instant))
        );
    }
}
