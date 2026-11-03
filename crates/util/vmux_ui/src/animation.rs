pub struct Height;

pub struct Movement;

impl Movement {
    pub fn transition(element: &str) {
        crate::transport::Host::animate_layout(element, 180);
    }
}

impl Height {
    pub fn transition(element: &str) {
        crate::transport::Host::animate_height(element, 160);
    }
}
