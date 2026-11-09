use dioxus::prelude::*;

use crate::components::manager::ManagerView;

#[derive(PartialEq)]
pub struct CollectionController<F: 'static> {
    query: Signal<String>,
    view: Signal<ManagerView>,
    filters: Signal<Vec<F>>,
}

impl<F: 'static> Clone for CollectionController<F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<F: 'static> Copy for CollectionController<F> {}

pub fn use_collection_controller<F: 'static>(initial_view: ManagerView) -> CollectionController<F> {
    CollectionController {
        query: use_signal(String::new),
        view: use_signal(|| initial_view),
        filters: use_signal(Vec::new),
    }
}

impl<F: Clone + 'static> CollectionController<F> {
    pub fn query(self) -> String {
        (self.query)()
    }

    pub fn view(self) -> ManagerView {
        (self.view)()
    }

    pub fn filters(self) -> Vec<F> {
        (self.filters)()
    }

    pub fn set_query(mut self, query: String) {
        self.query.set(query);
    }

    pub fn set_view(mut self, view: ManagerView) {
        self.view.set(view);
    }

    pub fn update_filters(mut self, update: impl FnOnce(&mut Vec<F>)) {
        update(&mut self.filters.write());
    }

    pub fn replace_filters(mut self, filters: Vec<F>) {
        self.filters.set(filters);
    }

    pub fn add_filter(mut self, filter: F) {
        self.filters.write().push(filter);
    }

    pub fn remove_filter(mut self, index: usize) {
        let mut filters = self.filters.write();
        if index < filters.len() {
            filters.remove(index);
        }
    }

    pub fn pop_filter(mut self) {
        self.filters.write().pop();
    }

    pub fn dismiss_filter(mut self) {
        self.query.set(String::new());
    }

    pub fn toggle_filter(mut self) {
        if (self.query)().starts_with('#') {
            self.query.set(String::new());
        } else {
            self.query.set("#".to_string());
        }
    }
}
