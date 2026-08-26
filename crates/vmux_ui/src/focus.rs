#[derive(Clone)]
pub struct FocusClaim {
    element_id: std::borrow::Cow<'static, str>,
    caret: Caret,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Caret {
    AsIs,
    ToEnd,
}

impl FocusClaim {
    pub fn new(element_id: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self {
            element_id: element_id.into(),
            caret: Caret::AsIs,
        }
    }

    pub fn caret_at_end(mut self) -> Self {
        self.caret = Caret::ToEnd;
        self
    }
}

impl FocusClaim {
    pub fn request(self) {
        crate::transport::Host::focus_element(&self.element_id);
        if self.caret == Caret::ToEnd {
            crate::transport::Host::caret_to_end(&self.element_id);
        }
    }
}
