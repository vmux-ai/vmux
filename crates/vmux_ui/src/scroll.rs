pub struct ScrollIntoView;

impl ScrollIntoView {
    pub fn nearest(element_id: &str) -> bool {
        crate::transport::Host::scroll_item_into_view(element_id)
    }

    pub fn element_to(element_id: &str, top: f64) {
        crate::transport::Host::scroll_element_to(element_id, top.max(0.0));
    }
    pub fn first_rendered(element_ids: &[&str]) {
        crate::transport::Host::reveal_first_rendered(element_ids, false);
    }

    pub fn first_rendered_centered(element_ids: &[&str]) {
        crate::transport::Host::reveal_first_rendered(element_ids, true);
    }
}
