use dioxus::prelude::*;
use std::rc::Rc;

pub type Container = Signal<Option<Rc<MountedData>>>;

pub(super) struct Observation {
    pub top: i32,
    pub previous: i32,
    pub distance: i32,
    pub initial: bool,
}

impl Observation {
    pub fn pauses_following(&self) -> bool {
        !self.initial && self.distance > 0 && self.top < self.previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_upward_scroll_stops_following_until_the_actual_bottom() {
        let moved = Observation {
            top: 995,
            previous: 1000,
            distance: 5,
            initial: false,
        };
        assert!(moved.pauses_following());
    }

    #[test]
    fn shrinking_content_at_the_bottom_does_not_cancel_following() {
        for initial in [true, false] {
            let observed = Observation {
                top: 8020,
                previous: 8193,
                distance: 0,
                initial,
            };
            assert!(!observed.pauses_following());
        }
    }

    #[test]
    fn initial_placement_does_not_become_a_manual_upward_scroll() {
        let initial = Observation {
            top: 8000,
            previous: 8193,
            distance: 400,
            initial: true,
        };
        assert!(!initial.pauses_following());
        let manual = Observation {
            top: 8000,
            previous: 8193,
            distance: 400,
            initial: false,
        };
        assert!(manual.pauses_following());
    }
}
