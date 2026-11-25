use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::channel::{
    AsyncChannel, AsyncChannelPlugin, ChannelClosed, ChannelReceived, MessageSender, SpawnSender,
};

#[derive(Message, Clone, Debug)]
struct LogLine(String);

#[derive(Component, Clone, Debug)]
struct IncomingJob(u32);

struct ExampleChannelPlugin;

impl Plugin for ExampleChannelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            AsyncChannelPlugin::message::<LogLine>().bounded(8),
            AsyncChannelPlugin::spawn::<IncomingJob>(),
            AsyncChannelPlugin::<String>::default(),
        ))
        .add_systems(Update, (print_logs, print_jobs))
        .add_observer(print_chunks);
    }
}

fn print_logs(mut reader: MessageReader<LogLine>) {
    for line in reader.read() {
        println!("message: {}", line.0);
    }
}

fn print_jobs(jobs: Query<&IncomingJob, Added<IncomingJob>>) {
    for job in &jobs {
        println!("spawned job: {}", job.0);
    }
}

fn print_chunks(mut trigger: On<ChannelReceived<String>>) {
    let entity = trigger.event().entity;
    if let Some(chunk) = trigger.event_mut().take() {
        println!("stream ({entity:?}): {chunk}");
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(ExampleChannelPlugin);
    app.update();

    let logs = app.world().resource::<MessageSender<LogLine>>().clone();
    let spawns = app.world().resource::<SpawnSender<IncomingJob>>().clone();
    let (stream_tx, stream_rx) = AsyncChannel::<String>::unbounded();
    let worker = app.world_mut().spawn(stream_rx).id();
    app.update();

    let producer = std::thread::spawn(move || {
        logs.send(LogLine("worker started".into())).unwrap();
        spawns.send(IncomingJob(7)).unwrap();
        stream_tx.send("chunk-1".into()).unwrap();
        stream_tx.send("chunk-2".into()).unwrap();
    });
    producer.join().unwrap();

    app.update();
    println!(
        "stream_closed={}",
        app.world().get::<ChannelClosed<String>>(worker).is_some(),
    );
}
