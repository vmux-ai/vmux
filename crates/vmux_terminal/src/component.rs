use bevy::prelude::*;

#[derive(Component)]
pub struct Terminal;

#[derive(Component)]
pub struct ProcessExited;

pub type PtyExited = ProcessExited;
