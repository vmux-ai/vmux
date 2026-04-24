# Voxel World Design

A Minecraft-like voxel world for vmux's Player Mode. When the user toggles into Player Mode, a procedurally generated landscape spawns around the scene. The browser tile plane becomes a "big keynote screen" floating above a terrain the user can walk on — with gravity, block collision, jumping, and step-up over 1-block ledges — while their tabs remain visible as a cinema-sized display in the sky. Toggling back to User Mode despawns the world.

The goal is a playground that exercises real voxel-engine techniques (chunking, meshing, LOD, collision) via established crates, without reinventing them.

## Scope

**In scope (v1):**
- Procedural noise terrain (deterministic via fixed seed), spawned only in Player Mode
- LOD-tiered rendering out to ~1km render distance
- Minecraft-scale player: 1.8m tall, 4.3 m/s walk, 5.6 m/s sprint, 1.25m jump
- Full physics: gravity, capsule collider, block collision, step-up over 1-block ledges
- Browser tiles repositioned visually as a "floating keynote screen" above the terrain
- Clear zone around spawn so terrain never blocks the screen

**Out of scope (v1):**
- Block breaking/placing (data path exists via `set_voxel`; input wiring deferred)
- Biomes, caves, water
- Save/load of modified voxels
- Multiplayer
- Any gameplay loop (no inventory, no mobs, no day/night cycle)

## Architecture

### New crate: `vmux_voxel`

Follows vmux's per-subsystem crate pattern. Exports one plugin:

```rust
vmux_voxel::VoxelPlugin
```

`vmux_desktop` adds `VoxelPlugin` to the Bevy app alongside `ScenePlugin`. The voxel plugin reads `InteractionMode` (defined in `vmux_desktop::scene`) to gate its activity.

### Dependencies (workspace)

| Crate | Version | Purpose |
|---|---|---|
| `bevy_voxel_world` | `^0.15` (with `noise` feature) | Chunking, multithreaded meshing, spawning/despawning, raycast, LOD hooks |
| `avian3d` | `^0.6` | Physics engine (ECS-native) |
| `bevy-tnua` | `^0.31` | Floating character controller (step-up, jumps, slopes) |
| `bevy-tnua-avian3d` | `^0.11` | Avian backend for tnua |
| `noise` | `^0.9` | Perlin noise for terrain |

All verified Bevy 0.18 compatible (2026-04-24).

### File layout

Per the project rule (no `mod.rs`, filename-based modules):

```
crates/vmux_voxel/
├── Cargo.toml
└── src/
    ├── lib.rs          // VoxelPlugin, registers sub-plugins, re-exports
    ├── config.rs       // VmuxVoxelWorld config: terrain, LOD, materials
    ├── lifecycle.rs    // spawn/despawn world + player on InteractionMode change
    ├── player.rs       // Player entity: capsule + tnua controller + camera child
    ├── input.rs        // WASD + mouse-look → TnuaBuiltinWalk / TnuaBuiltinJump
    └── collider.rs     // Chunk mesh → avian trimesh collider (bubble around player)
```

Public surface: only `VoxelPlugin`. All other types are private.

## Terrain generation

A `VoxelWorldConfig` impl named `VmuxVoxelWorld` provides the noise-based lookup delegate.

### Height function

```
height(x, z) = perlin(x * 0.02, z * 0.02, SEED) * 8.0 - 4.0
```

- Baseline surface near `y = -4`, variation ±8 blocks → surface range roughly `y ∈ [-12, +4]`.
- Fixed seed (`SEED: u32 = 0x564D5558` — "VMUX" in ASCII) for reproducibility.
- Scale `0.02` gives gentle hills over ~50m wavelength.

### Material layering

```
if y > height(x, z):          Air
else if y == floor(height):   Grass  (u8 = 0)
else if y > height - 4.0:     Dirt   (u8 = 1)
else:                         Stone  (u8 = 2)
```

### Spawn clear zone

Within a 24-block horizontal radius of the world origin (x² + z² ≤ 576), override the noise:

- `y > -4` → Air (the "screen arena" is always open)
- `y == -4` → Grass (flat floor at fixed height)
- `y < -4` → Dirt/Stone per default

This guarantees the browser "screen" at `(0, 0..10.8, 0)` is never obscured by procedural hills, and provides a ~48m-wide flat arena that contains both the screen and the player spawn position at `(0, -2.4, 20)` — the player lands on the flat floor, not on noise terrain.

### Materials / textures

Three materials: Grass, Dirt, Stone. For v1, a 3-layer array texture `assets/voxel_atlas.png` (16×48px, three 16×16 solid-colored tiles) ships with the crate. `texture_index_mapper` returns:

| Material | `[top, side, bottom]` |
|---|---|
| Grass (0) | `[0, 0, 1]` (green top, green sides, brown bottom) |
| Dirt (1)  | `[1, 1, 1]` (brown all sides) |
| Stone (2) | `[2, 2, 2]` (gray all sides) |

Solid-color placeholder art; visual polish deferred.

### LOD tiers

Per-chunk LOD determined by distance from camera (`chunk_lod`):

| Chunk distance | LOD | Mesh shape |
|---|---|---|
| 0–8 chunks | 0 | Full 32³ resolution |
| 9–16 chunks | 1 | 16³ (2× block size) |
| 17–32 chunks | 2 | 8³ (4× block size) |

- Spawning distance: **32 chunks** (~1km radius at 32-block chunks).
- Seam handling uses `bevy_voxel_world`'s built-in `WorldVoxel::Unset` padded-border mechanism (standard pattern from the `noise_terrain_lod` example).

## Player entity & movement

Spawned only while in Player Mode. Entity hierarchy:

```
Player (root)
├── RigidBody::Dynamic
├── Collider::capsule(radius=0.3, height=1.8)
├── TnuaController
├── TnuaAvian3dSensorShape(Collider::cylinder(radius=0.29, height=0.0))
├── LockedAxes::ROTATION_LOCKED
├── Transform (spawn at ground surface)
│
└── MainCamera (reparented child)
    ├── Transform::from_xyz(0.0, 0.72, 0.0)  // eye 1.62m above feet
    └── (Camera3d + PerspectiveProjection stay)
```

### Movement tuning (Minecraft-accurate)

| Parameter | Value | Source |
|---|---|---|
| Capsule radius | 0.3 | Minecraft player width ~0.6 |
| Capsule height | 1.8 | Minecraft player height |
| Eye offset | +0.72 above capsule center | 1.62m from feet |
| Walk speed | 4.3 m/s | Minecraft walk |
| Sprint speed | 5.6 m/s | Minecraft sprint (Shift) |
| Jump height | 1.25 m | Minecraft jump |
| Float height | 0.9 m | Half capsule (tnua parameter) |
| Acceleration | 60.0 m/s² | Responsive feel |

### Input (`input.rs`)

Runs only when in Player Mode AND no browser pane has keyboard focus (mirrors the existing `suppress_free_camera_when_pane_active` pattern in `scene.rs`):

| Input | Action |
|---|---|
| W/A/S/D | Horizontal movement via `TnuaBuiltinWalk::desired_velocity` |
| Shift (held) | Sprint modifier on desired velocity |
| Space | `TnuaBuiltinJump` action |
| Mouse motion | Yaw on Player body, pitch on MainCamera (clamped ±89°) |

The existing vmux `FreeCamera` component is removed from `MainCamera` on entering Player Mode and re-added on exiting.

### Spawn positioning

Computed once when `EnterPlayer` transition starts. Spawn is inside the clear zone, so ground height is the flat floor (`y = -4`), not the noise value:

```
spawn_xz = (0, 20)                             // 20m back from screen, z+ (inside clear zone)
ground_y = -4                                  // flat floor in the clear zone
spawn_pos = (0, ground_y + 0.9, 20) = (0, -3.1, 20)   // capsule center at +0.9 from feet
look_at = (0, 5.4, 0)                          // screen center (m.y/2 for 1080p)
```

The player stands 20m from the browser "screen" on flat ground, looking at its center — an audience-member viewing angle.

## Player Mode integration & lifecycle

The voxel plugin reads `Res<InteractionMode>` and `Option<Res<ModeTransition>>` from `vmux_desktop::scene`. All transitions are driven by events that already exist.

### State table

| `InteractionMode` | `ModeTransition` | Voxel world state | Player entity |
|---|---|---|---|
| `User` | None | Dormant — plugin registered but inactive | None |
| `User` | `EnterPlayer` (in progress) | Spawning chunks in background, meshing overlaps with fade-in | Spawned at transition start |
| `Player` | None | Active — chunks spawn/despawn around player, colliders track player | Alive, tnua-controlled |
| `Player` | `ExitPlayer` (in progress) | Still rendering during fade-out | Still alive, input disabled |
| `User` (after exit complete) | None | All chunks flagged `NeedsDespawn` | Despawned; MainCamera unparented back to scene root |

### Lifecycle systems (`lifecycle.rs`)

**On `EnterPlayer` transition *start*** (fires once per transition):

1. Compute spawn position (flat floor at `y = -4` within clear zone)
2. Spawn Player entity at spawn position with components listed above
3. Insert `VoxelWorldCamera::<VmuxVoxelWorld>::default()` on MainCamera — `bevy_voxel_world`'s marker that gates chunk spawning around an entity. Chunks begin meshing in background during the fade-in.

MainCamera is **not yet** reparented — it stays at User-Mode framing during the 300ms fade so the view doesn't jump while the player doesn't yet control the camera. Tnua input is not yet active.

**On `EnterPlayer` transition *complete*** (replaces existing behavior where `FreeCameraState.enabled = true`):

1. Reparent MainCamera as child of Player entity
2. Snap MainCamera's local transform to `Transform::from_xyz(0.0, 0.72, 0.0)` — eye offset above capsule center
3. Remove `FreeCamera` component from MainCamera (tnua now drives)
4. Enable tnua input processing

**On `ExitPlayer` transition *start*** (fires once per transition, before the exit animation runs):

1. Unparent MainCamera back to scene root, preserving world transform at the moment of unparent (i.e., it keeps the Player's current eye position in world space). This must happen *before* `scene.rs::setup_exit_camera_animation` runs, because that system animates MainCamera's Transform from its current value to `CameraHome` — and if MainCamera were still parented to Player, it would animate in local space, not world space.
2. Disable tnua input (Player entity stays alive during the animation so physics stays coherent, but no movement input is processed).

**On `ExitPlayer` transition *complete***:

1. Re-add `FreeCamera` component (disabled state); `fit_main_camera` restores User-Mode framing if the animation hasn't already placed it exactly
2. Remove `VoxelWorldCamera` marker from MainCamera — prevents new chunks from spawning
3. Query all `Entity, With<Chunk>` and insert `NeedsDespawn` — `bevy_voxel_world`'s own `despawn_retired_chunks` system cleans them up next frame
4. Despawn Player entity

### Camera ownership during transitions

The fade-in animation (bloom + sunlight ramp over 300ms) and exit animation (camera-returns-to-home via `AnimationClip`) both operate on `MainCamera`'s `Transform`. Resolution:

- **EnterPlayer**: MainCamera stays at User-Mode framing (unchanged transform) through the fade-in — no jump, no interference with the bloom ramp. At `complete_mode_transition` (timer finished), MainCamera reparents into Player and snaps to the eye offset. The camera "jumps" to its new position at the same instant tnua input becomes active — a clean handoff, visually masked by the fact that the user has just pressed the toggle.
- **ExitPlayer**: MainCamera unparents **at transition start** (before the exit animation begins), preserving its world transform. This is required because `scene.rs::setup_exit_camera_animation` animates MainCamera's Transform in whatever space it's currently in — if still parented to Player, the animation would run in Player-local space and go to the wrong place. After unparent, the existing exit animation runs unchanged, animating world-space Transform from current to `CameraHome`. At transition complete, Player entity despawns and chunks are marked for despawn.

### Coexistence with existing scene.rs behavior

| Existing behavior | Change for voxel world |
|---|---|
| `FreeCameraState.enabled = true` after EnterPlayer fade-in completes | No longer set; tnua drives movement instead |
| `suppress_free_camera_when_pane_active` toggles `state.enabled` based on `CefKeyboardTarget` | Replace FreeCamera target with tnua — when a pane has focus, disable tnua input |
| `complete_mode_transition` cleans up bloom/sunlight on ExitPlayer complete | Add voxel cleanup (chunks, player) alongside |
| `CameraHome` resource captures return transform | Unchanged |

## Chunk collider integration

### Strategy

Listen to `bevy_voxel_world`'s chunk events and attach avian trimesh colliders only to chunks within a small bubble around the player — collision radius ≪ render radius.

### Collider bubble

- Collider radius = **4 chunks** (~128m) around the Player entity.
- Render radius = 32 chunks. At 32 chunks × several LOD tiers, that's potentially 1000+ chunks; attaching trimesh colliders to all of them is prohibitively expensive.
- 128m bubble is far more than a player can traverse between frames even at sprint speed (5.6 m/s × 1/60 s = 0.093m), so colliders are always ready ahead of the player.

### Event wiring (`collider.rs`)

```rust
fn attach_chunk_collider_if_close(
    mut events: MessageReader<ChunkWillSpawn<VmuxVoxelWorld>>,
    player: Option<Single<&Transform, With<Player>>>,
    mut commands: Commands,
) {
    let Some(player) = player else { return };
    for ev in events.read() {
        let chunk_center = chunk_position_to_world(ev.chunk_key);
        if chunk_center.distance(player.translation) > 128.0 { continue; }
        commands.entity(ev.entity).insert(ColliderConstructor::TrimeshFromMesh);
    }
}

fn rebuild_chunk_collider(
    mut events: MessageReader<ChunkWillRemesh<VmuxVoxelWorld>>,
    mut commands: Commands,
) {
    for ev in events.read() {
        commands.entity(ev.entity)
            .remove::<Collider>()
            .insert(ColliderConstructor::TrimeshFromMesh);
    }
}

fn maintain_collider_bubble(
    player: Option<Single<&Transform, With<Player>>>,
    chunks: Query<(Entity, &Transform, Option<&Collider>), With<Chunk>>,
    mut commands: Commands,
) {
    // Add colliders to chunks entering the bubble, remove from chunks leaving.
}
```

`ColliderConstructor::TrimeshFromMesh` (avian3d) lazily builds the collider once the mesh asset is available — no timing races.

### Why trimesh, not heightmap or voxel-native

- Heightmap colliders don't handle overhangs or caves (future-proofing).
- Avian lacks a native voxel collider; writing one would defeat the purpose of using avian.
- Trimesh is heavier but correct for arbitrary chunk shapes.

## Error handling & edge cases

| Case | Handling |
|---|---|
| Toggle Player Mode before previous transition finishes | `scene.rs` already ignores commands during `ModeTransition` — inherited |
| Chunk spawn event fires before Player entity exists | Guard: `if player.is_none() { return; }` — chunk is skipped for collider attachment; `maintain_collider_bubble` will pick it up next frame |
| LOD changes mid-traverse | `ChunkWillRemesh` handler triggers collider rebuild via `ColliderConstructor` |
| Player falls out of world (below lowest generated chunk) | v1: no recovery — design assumes terrain extends deep enough (stone down to `y → -∞` in lookup delegate). Follow-up: teleport-to-spawn on fall |
| Browser pane takes keyboard focus while in Player Mode | Existing `CefSuppressKeyboardInput` + new mirror for tnua input: suppress movement input when `CefKeyboardTarget` exists, same as current `FreeCameraState.enabled` logic |

## Testing

### Automated

- **Unit test** (`config.rs`): noise lookup delegate with fixed seed produces expected voxel types at known coordinates. Verifies reproducibility.
- **Unit test** (`config.rs`): clear zone (radius 12 around origin) returns `Air` above `y = -4` and `Grass` at `y = -4`, regardless of noise output.
- **Integration test** (plugin smoke): build a `MinimalPlugins` app with `VoxelPlugin`, `VoxelWorldPlugin::<VmuxVoxelWorld>::minimal()`, and avian's `PhysicsPlugins::default()`. Run 2 update cycles. Assert no panic. Mirrors the pattern in `bevy_voxel_world`'s own `test.rs`.

### Manual (`make run-mac`)

1. Launch: User Mode, no voxel world visible, no perf regression.
2. Toggle to Player Mode: during the 300ms fade, chunks start meshing in the background.
3. Post-fade: camera is at the Player's eye; WASD walks at Minecraft speed, Shift sprints, Space jumps. Movement feels grounded.
4. Walk up to a 1-block ledge — step up happens automatically (tnua built-in).
5. Walk off a cliff — gravity brings you down, you land.
6. Walk to the edge of the 128m collider bubble — chunks further out render but can't be collided with. Player can't reach them (physically far further than visible horizon is close).
7. Look up at the browser tiles: the "keynote screen" floats ~5m above ground at origin. Tabs remain interactive (via existing vmux input routing when a pane is focused).
8. Click a browser pane → keyboard input routes to browser; tnua input suppressed. Click empty space → tnua resumes.
9. Toggle back to User Mode: exit animation plays, chunks despawn, camera returns to framing tiles. No ghost physics, no lingering chunks.

### Out of scope for v1 tests

- Cross-chunk collision correctness (manual playtest: walk across chunk boundaries, verify no catch/stutter).
- Movement feel tuning (subjective, adjust via playtest).
- Rendering correctness at LOD seams (visual).

## Open questions deferred to follow-ups

- **Block breaking/placing**: raycast + click → `voxel_world.set_voxel` is ~30 lines. Layer on after v1.
- **Physics-driven browser**: could the browser plane itself become a physics object (e.g., you could push it)? Probably no — keeps integration simple.
- **Biomes & caves**: bigger lookup delegate. Plugin architecture supports it trivially.
- **Save/load persistent edits**: crate exposes the delta `HashMap` — ron-serialize to disk alongside vmux's other persistence.

## Appendix: why these crate choices

| Choice | Alternative | Why this |
|---|---|---|
| `bevy_voxel_world` (full-stack) | Hand-rolled chunking + `block-mesh` | Explicit Bevy 0.18 support, actively maintained, has LOD + raycast + custom material/meshing hooks; unblocks "all 8 voxel techniques" with minimal code. User goal is integration, not re-implementation. |
| `avian3d` | `bevy_rapier3d` | ECS-native, modern default, simpler API, same backend (parry3d) |
| `bevy-tnua` | Custom voxel AABB controller | Floating controller handles step-up, slopes, coyote time, jump buffer — subtleties that take weeks to implement well |
| Trimesh collider per chunk | Heightmap collider / custom voxel collider | Correct for overhangs/caves, cheap to regen in a small bubble |
