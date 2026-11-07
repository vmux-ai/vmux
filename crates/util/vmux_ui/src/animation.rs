pub struct Height;

pub struct Movement;

impl Movement {
    pub fn transition(element: &str) {
        crate::transport::Host::animate_layout(element, crate::motion::Motion::LAYOUT_MS);
    }
}

impl Height {
    pub fn transition(element: &str) {
        crate::transport::Host::animate_height(element, crate::motion::Motion::LAYOUT_MS);
    }
}
