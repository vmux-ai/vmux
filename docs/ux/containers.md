# Manage a VM and Docker

Vmux can run Docker in its own Linux VM on Apple Silicon macOS. Open the VM bookmark beside Simulator. The side sheet follows **VM → Docker → Containers → group → container**; selecting a level changes the information shown in the main area.

## Start and stop the right thing

The first visit starts the VM once. Startup shows the VM illustration and progress while keeping the side sheet available. A manual stop stays respected when you change pages. Use the VM row's play control to start it again, and its pause-shaped control to stop the entire VM.

A container's play or pause control affects that container. A Compose group's control affects the group. Starting a service also starts its required Compose dependencies and waits for their readiness. The VM control remains available while that work is pending.

The VM page shows its status, CPU, memory and shared folder once ready. Resource settings can be changed while stopped. Containers and their disk data survive VM shutdown. Saved container names remain in the side sheet while stopped; the main area shows the VM Start surface until Docker is ready.

## Make containers appear

Use normal Docker commands against the Vmux daemon. `docker build` creates an image; `docker run`, `docker create`, or `docker compose up` creates containers. Vmux discovers containers from Docker automatically, including stopped containers and Compose groups. No separate registration is needed.

From Terminal, `vmux container start` starts the engine and prints its Docker host and a command using that host. Use that host when running Docker commands. `vmux container status` reports the same connection and runtime logs.

Select a container or group to open Info or Logs. The route is `vmux://vm/docker/containers/.../info` or `.../logs`. Switching selections retains the hosted surface and breadcrumb identity. Background refresh failures keep the last successful inventory and show a diagnostic without locking out recovery controls.

Publish a port with a wildcard guest binding, such as `-p 8080:80`; Vmux forwards it to localhost on the Mac. Explicit guest-loopback bindings such as `-p 127.0.0.1:8080:80` currently need a separate tunnel and are not supported. The nginx demo in `examples/containers` can be used to try normal port forwarding.

Closing a page leaves its work running. Quitting the GUI stops its owned VM; CLI-started engines and other profiles remain independent. Explicitly pausing the VM stops its runtime while preserving the guest disk and containers.
