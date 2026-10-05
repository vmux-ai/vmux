use std::time::{Duration, Instant};

use bevy::prelude::Entity;
use vmux_api::command_bar::OpenId;

#[derive(Default)]
pub(super) struct OpenVersion {
    initialized: bool,
    open_id: OpenId,
    generation: RequestGeneration,
}

impl OpenVersion {
    pub(super) fn accept(&mut self, open_id: OpenId) -> Option<bool> {
        if self.initialized && self.open_id == open_id {
            return Some(false);
        }
        if self.initialized && open_id.0 < self.open_id.0 {
            return None;
        }
        self.initialized = true;
        self.open_id = open_id;
        self.generation.advance();
        Some(true)
    }

    pub(super) fn matches(&self, open_id: OpenId) -> bool {
        self.initialized && self.open_id == open_id
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation.current()
    }
}

#[derive(Default)]
pub(super) struct RequestGeneration(u64);

impl RequestGeneration {
    pub(super) fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }

    pub(super) fn current(&self) -> u64 {
        self.0
    }

    pub(super) fn matches(&self, generation: u64) -> bool {
        generation != 0 && self.0 == generation
    }
}

pub(super) struct RequestDelay {
    pub(super) target: Entity,
    pub(super) generation: u64,
    pub(super) query: String,
    due: Instant,
}

impl RequestDelay {
    pub(super) fn new(target: Entity, generation: u64, query: String, delay: Duration) -> Self {
        Self {
            target,
            generation,
            query,
            due: Instant::now() + delay,
        }
    }

    pub(super) fn ready(&self) -> bool {
        Instant::now() >= self.due
    }
}
