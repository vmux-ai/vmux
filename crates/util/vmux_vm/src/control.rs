use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::{IoTaskPool, Task, futures_lite::future};

use crate::Settings;
use crate::engine::{Engine, EngineSet, Failure, Phase};
use crate::process::Lease;

pub(crate) struct ControlPlugin;

impl Plugin for ControlPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (listen, receive).chain().in_set(EngineSet::Control));
    }
}

#[derive(Component)]
struct Listener(UnixListener);

impl Listener {
    fn bind(path: &Path) -> io::Result<Self> {
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self(listener))
    }

    fn accept(&self) -> io::Result<Option<UnixStream>> {
        match self.0.accept() {
            Ok((stream, _)) => Ok(Some(stream)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[derive(Component)]
struct Request(Task<io::Result<UnixStream>>);

impl Request {
    fn read(mut stream: UnixStream) -> io::Result<UnixStream> {
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut bytes = [0; 5];
        stream.read_exact(&mut bytes)?;
        if &bytes != b"STOP\n" {
            let _ = stream.write_all(b"INVALID\n");
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid VM control request",
            ));
        }
        Ok(stream)
    }
}

pub(crate) struct GuestControl;

impl GuestControl {
    pub(crate) fn stop(settings: &Settings) -> io::Result<()> {
        let mut stream = UnixStream::connect(settings.guest_control())?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        stream.write_all(b"STOP\n")?;
        let mut reply = [0; 9];
        stream.read_exact(&mut reply)?;
        if &reply != b"STOPPING\n" {
            return Err(io::Error::other("guest refused shutdown"));
        }
        Ok(())
    }
}

type UnboundListeners<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Settings, &'static mut Phase),
    (With<Engine>, With<Lease>, Without<Listener>),
>;

fn listen(mut engines: UnboundListeners, mut commands: Commands) {
    for (entity, settings, mut phase) in &mut engines {
        if !matches!(
            *phase,
            Phase::NetworkStarting | Phase::DockerStarting | Phase::Ready
        ) {
            continue;
        }
        match Listener::bind(&settings.control_socket()) {
            Ok(listener) => {
                commands.entity(entity).insert(listener);
            }
            Err(error) => {
                *phase = Phase::Stopping;
                commands.entity(entity).insert(Failure(error.to_string()));
            }
        }
    }
}

fn receive(
    mut engines: Query<(Entity, &Listener, &mut Phase), With<Engine>>,
    mut requests: Query<(Entity, &ChildOf, &mut Request)>,
    mut commands: Commands,
) {
    for (entity, owner, mut request) in &mut requests {
        let Some(result) = future::block_on(future::poll_once(&mut request.0)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let Ok(mut stream) = result else {
            continue;
        };
        let Ok((_, _, mut phase)) = engines.get_mut(owner.parent()) else {
            continue;
        };
        if !matches!(*phase, Phase::Stopped | Phase::Failed) {
            *phase = Phase::Stopping;
        }
        let _ = stream.write_all(b"STOPPING\n");
    }
    let mut pending = requests.iter().count();
    for (entity, listener, phase) in &engines {
        if matches!(phase, Phase::Stopped | Phase::Failed) {
            commands.entity(entity).remove::<Listener>();
            continue;
        }
        while pending < 16 {
            let stream = match listener.accept() {
                Ok(Some(stream)) => stream,
                Ok(None) | Err(_) => break,
            };
            commands.spawn((
                Name::new("VM stop request"),
                ChildOf(entity),
                Request(IoTaskPool::get().spawn(async move { Request::read(stream) })),
            ));
            pending += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fragmented_stop_frame_is_read_after_accepting_a_nonblocking_socket() {
        let (stream, mut client) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            client.write_all(b"ST").unwrap();
            std::thread::sleep(Duration::from_millis(10));
            client.write_all(b"OP\n").unwrap();
        });
        Request::read(stream).unwrap();
        worker.join().unwrap();
    }
}
