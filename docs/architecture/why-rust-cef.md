# Why Rust + CEF: Custom Desktop Architecture in the Agentic Era

> Part of the [Vmux Architecture](../architecture.md) overview.

Desktop frameworks like Electron were built to solve a 2010s problem: *How do we let developers build cross-platform, pixel-perfect user interfaces for human users?* Electron assumes the web page *is* the application. It wraps Chromium around your code, forces everything through a JavaScript/Node.js runtime, and locks down native capabilities behind a strict security sandbox.

In the agentic era, the calculus changes completely. We are no longer constrained by the traditional trade-offs of team bandwidth and boilerplate management. Two shifts make owning the stack affordable: modern agentic tooling absorbs the heavy lifting of software execution and system boilerplate, and a mature Rust crate ecosystem already supplies the low-level building blocks — windowing, GPU, async, FFI bindings — battle-tested and off the shelf. Between the two, we can build and maintain our own custom infrastructure from the ground up, instead of leaning on the generic sandbox of a heavy framework just to get things done.

By taking direct control of our entire tech stack, we eliminate the artificial ceilings of the web container and unleash the full potential of the underlying hardware. Vmux discards the framework layer entirely. Instead, a native **Rust host** sits directly on the metal, treating the **Chromium Embedded Framework (CEF)** purely as an unprivileged guest rendering surface.

---

## Unlocking the Metal: Structural Capabilities of a Custom Infrastructure

By owning our custom infrastructure rather than inheriting Electron's abstractions, we instantly unlock capabilities that a generic web container physically cannot support:

### 1. Deep Programmatic DevTools & Native Automation

* **The Electron Limit:** While you can open a DevTools window in Electron, you cannot programmatically intercept or drive its internal communication layer. You are blocked from filtering or modifying the protocol commands passing between the page and the browser engine.
* **The Vmux Advantage:** Our Rust host has direct access to the **Chrome DevTools Protocol (CDP)** at the native library layer. We can execute low-level headless automation, listen to raw console events, manipulate the DOM *beneath* the JavaScript layer, and simulate hardware inputs directly into the browser's engine with zero reliance on external automation bridges like Puppeteer.

### 2. Off-Screen Rendering (OSR) via GPU Textures

* **The Electron Limit:** Electron treats web views (`WebContentsView`) rigidly as OS-level child windows tied to exact screen coordinates. You cannot pass a web page directly into a custom render pipeline or composite it fluidly inside a custom application canvas.
* **The Vmux Advantage:** We can initialize CEF in windowless, GPU-accelerated off-screen rendering mode (`windowless_rendering_enabled`), so CEF hands back each rendered frame as a shared GPU texture (`OnAcceleratedPaint`) instead of allocating an OS window. Our Rust app drops that texture straight into our Bevy render graph—letting us treat a live web page as an interactive 3D asset to tile, stack, rotate, or shade with custom fragment shaders. (The everyday flat workspace uses native windowed views; this OSR path powers the spatial 3D mode.)

### 3. Multi-Session Profile Isolation

* **The Electron Limit:** Running isolated multi-tenant contexts (separate cookie jars, proxy configurations, local storage pools) side-by-side inside an Electron workspace forces you to spawn independent partition instances, each carrying heavy internal process overhead.
* **The Vmux Advantage:** We programmatically instantiate unique `CefRequestContext` handles directly in Rust memory. This gives us granular, code-level control over the lifecycle, network proxy credentials, and authentication stores per view—allowing us to scale isolated guest profiles cleanly under a single host controller.

---

> ### Thesis
>
> Traditional desktop frameworks were engineered to sandbox a human-facing web page because building native, multi-process desktop stacks was historically too expensive. In the agentic era, we can afford to manage our own tech stack. We do not need an application framework. We need a **native systems controller** that treats the browser engine as a programmable utility, giving us the low-level primitive control necessary to turn web pages into lightweight components within a native graphics pipeline.
