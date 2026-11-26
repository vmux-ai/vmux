use std::time::{Duration, Instant};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{Operation, OperationCompleted, OperationState};
use vmux_app::job::{Job, JobPlugin, JobRequest, JobResult, JobSet};

#[derive(Clone)]
struct WordCount(String);

impl Job for WordCount {
    type Output = usize;

    fn run(self) -> Self::Output {
        self.0.split_whitespace().count()
    }
}

fn apply_results(
    results: Query<(Entity, &JobResult<WordCount>, &OperationState)>,
    mut commands: Commands,
) {
    for (entity, result, state) in &results {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        match &result.0 {
            Ok(count) => {
                println!("word count job finished: {count}");
                commands.trigger(OperationCompleted {
                    entity,
                    result: Ok(()),
                });
            }
            Err(error) => {
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error.clone()),
                });
            }
        }
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        VmuxPlugin::builder().jobs(true).build(),
        JobPlugin::<WordCount>::default(),
    ))
    .add_systems(Update, apply_results.in_set(JobSet::Apply));

    let op = app
        .world_mut()
        .spawn((
            Operation::bundle("text.word_count").unwrap(),
            JobRequest(WordCount("vmux framework runs blocking jobs in ecs".into())),
        ))
        .id();

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.update();
        if app.world().get::<OperationState>(op) == Some(&OperationState::Succeeded) {
            println!("operation state: {:?}", OperationState::Succeeded);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
