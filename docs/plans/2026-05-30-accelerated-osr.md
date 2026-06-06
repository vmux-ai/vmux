# Accelerated OSR (GPU shared-texture) — Plan B

**Goal:** Replace CPU-bitmap CEF OSR with GPU shared-texture OSR so webview CPU drops to Chromium-browser parity (scroll ~51% → near-idle). Prerequisite migration (Bevy 0.19-rc.2 + CEF 148, wgpu 29) landed in `12bb431`.

**Status:** macOS first (this branch). Windows (D3D12) + Linux (Vulkan/dmabuf) follow on real-GPU boxes.

## Key constraint (drives the design)

CEF's `AcceleratedPaintInfo` is valid **only during the `on_accelerated_paint` callback**; it returns to CEF's pool when the callback returns. And `cef::osr_texture_import`:
- `SharedTextureHandle::new(&AcceleratedPaintInfo)` is the **only** external constructor (the per-platform importers are `pub(crate)`).
- `SharedTextureHandle` is **not `Send`, not `Clone`**; `import_texture(self, &wgpu::Device)` **consumes** it.

So the plan's earlier "clone the handle through the 3-reader Bevy `Message` broadcast" does NOT work. Corrected flow uses **move semantics + a retained keep-alive**.

## Data flow (macOS)

1. **Callback (`on_accelerated_paint`, main thread):**
   - `let handle = SharedTextureHandle::new(info);`
   - `let keepalive = Arc::new(IoSurfaceKeepAlive::retain(info.shared_texture_io_surface));` (IOSurfaceIncrementUseCount + CFRetain)
   - Send `AcceleratedFrame { webview, handle: SendHandle(handle), keepalive, dirty, width, height }` over a **dedicated `async_channel`** (`accel_sender`). No clone (channel moves). `SendHandle` is a newtype with `unsafe impl Send` (justified: IOSurface is thread-safe and retained).
   - Keep `on_paint` implemented unchanged (CPU fallback).

2. **Main world (one system, material-agnostic):**
   - Drain `accel_receiver`. For each frame, resolve the surface `Handle<Image>` from the webview entity's **`WebviewSurface(Handle<Image>)`** component (set by `ensure_mesh_webview_placeholder` regardless of material type / paint type). If absent yet, drop the frame (releases keep-alive) — next paint will land once the placeholder exists.
   - Push `PendingAcceleratedUpload { image: AssetId<Image>, handle, keepalive, dirty, width, height }` into a main-world resource `WebviewAcceleratedQueue(Mutex<Vec<…>>)`.

3. **Extract → render world (move via Mutex):**
   - `Extract` system locks the `Mutex` and `mem::take`s the Vec into a render-world `ExtractedAcceleratedUploads(Vec<…>)`. (Extract is read-only on the main world; interior mutability lets us move out without `Clone`.)

4. **Render world (system after `RenderSystems::PrepareAssets`):**
   - `let Some(gpu) = gpu_images.get(upload.image) else { continue }`; guard `gpu.texture_descriptor.size` vs upload dims.
   - `let src = match upload.handle.0.import_texture(render_device.wgpu_device()) { Ok(t) => t, Err(_) => { bump failure counter; continue } };`
   - Encode `copy_texture_to_texture` per dirty rect (whole frame if empty) from `src` → `gpu.texture`. Both `Bgra8UnormSrgb`.
   - `let idx = render_queue.submit(once(encoder.finish()));`
   - `render_queue.on_submitted_work_done(move || drop((keepalive_clone, src)));` — releases the IOSurface use-count + the imported texture exactly when the GPU copy completes. (Capture an `Arc` clone of the keep-alive + move `src` in.)

## Keep-alive (`IoSurfaceKeepAlive`, macOS)

- `retain(ptr)`: `IOSurfaceIncrementUseCount(ptr)` + `CFRetain(ptr)`.
- `Drop`: `IOSurfaceDecrementUseCount(ptr)` + `CFRelease(ptr)`.
- `unsafe impl Send + Sync`. extern "C" decls; `#[link(name = "IOSurface", kind = "framework")]` + `#[link(name = "CoreFoundation", kind = "framework")]`.
- Wrapped in `Arc` so the message, the extracted upload, and the `on_submitted_work_done` closure can each hold a reference; the single underlying retain is released when the last `Arc` drops (the GPU-completion closure).

## Auto-fallback

Per-webview consecutive `import_texture` `Err` counter in the render world. On threshold (e.g. 3), send a message to a main-world system that recreates that browser with `shared_texture_enabled = false` → CEF resumes `on_paint` (CPU path). Success resets the counter.

## Files

- `patches/bevy_cef_core-0.5.2/src/browser_process/renderer_handler.rs` — `RenderPaintData`/payload split is unnecessary now (accelerated uses its own channel); add `on_accelerated_paint` + `AcceleratedFrame` + `accel_sender`/`accel_receiver` plumbing; macOS keep-alive may live here or in a sibling.
- `patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs:218` — `shared_texture_enabled: true` (cfg macOS); recreate-without-shared-texture path for fallback.
- `patches/bevy_cef-0.5.2/src/webview/accelerated_upload.rs` (new) — main resolver system, `Mutex` queue, Extract drain, render import+blit+release plugin.
- `patches/bevy_cef-0.5.2/Cargo.toml` + `bevy_cef_core` Cargo.toml — enable cef `accelerated_osr` feature (macOS).

## Verification

- Unit: keep-alive retain/release balance (mock counters); dirty-rect mapping (reuse `webview_dirty_rects`); resolver picks `WebviewSurface` AssetId; fallback counter → recreate.
- Manual (on-device): scroll an animation-heavy page; Activity Monitor scroll CPU vs the 51% baseline and vs Chrome; confirm `on_accelerated_paint` fires / `on_paint` does not; no tearing / colour shift; typing (small dirty rects) correct.
- Gate: workspace fmt + clippy + test, plus the touched CEF crate package checks.

Delete this file once macOS accelerated OSR is merged (Windows/Linux tracked separately).
