# iOS memory strategy

Companion to `docs/IOS_FEASIBILITY_REPORT.md` and `docs/IOS_IMPLEMENTATION_ROADMAP.md`. Evidence tags as in the report: **[COMPILED]**, **[SOURCE]**, **[DOC]**, **[ASSUMPTION]**, **[OPEN]**.

**Rule for this document:** no memory limit, footprint or performance figure for an iPhone is stated as a fact or promised. Where a threshold is needed it is defined as *a quantity to be set from device measurements* (milestone M8). The only numbers quoted are the ones the repository itself measured, on other hardware, with their source.

## 1. What "memory" means on an iPhone

- iOS may terminate an app (jetsam) when the system is under pressure. The relevant quantity is the process's **physical footprint**, which on a unified-memory device includes GPU allocations. It is not resident set size, and it is not what the repo reads today. [ASSUMPTION: confirm against current Apple documentation and by observing jetsam reports on the device.]
- The memory an app may use is dynamic. Query it at runtime instead of assuming a cap (`os_proc_available_memory()`), and read the footprint with `task_info(TASK_VM_INFO)`. [ASSUMPTION]
- iPhone 12: 4 GB shared by iOS, other apps, the game's CPU heap and GPU buffers/textures. [DOC: your brief.]

What the repository measures today:
- Windows: working set. Linux: `/proc/self/statm`. Apple: `proc_pidinfo(PROC_PIDTASKINFO).pti_resident_size` (`horizon-threads/src/process_memory.rs:66`). This does not compile for iOS [COMPILED] and is the wrong metric there.
- The benchmark harness samples RSS from *outside* the process with `ps` (`docs/benchmark.md`); on Apple Silicon "no per-process [GPU memory] figure is available" [DOC]. On iOS the app must measure itself.
- Project-measured reference (desktop, not a prediction): steady working set 1.1 to 1.7 GB at render distance 8 to 32, versus 1.9 to 2.9 GB for vanilla, on Windows 11, Ryzen 7 9800X3D, RTX 5080, Horizon v0.2.97 (README, `benchmarks/perf/windows-9800x3d-5080-2026-10-29.md`) [DOC].

## 2. Audit of the existing implementation

Status key: **Read** = code read, behaviour understood from source. **Partial** = some code read. **Not read** = still to inspect. None of this has been measured on iOS.

| Area | What the code does | Evidence | Unknown / risk |
|---|---|---|---|
| **Chunk residency and eviction** | Columns live in `FxHashMap<(i32,i32), Arc<Chunk>>` (`chunk_map.rs`). Residency is **server-driven**: the client forgets columns when the server unloads them (`streaming.rs:1226 forget_columns`, "often thousands at once"). Pending sections for forgotten columns are dropped (`forget_pending_sections`). The integrated server's own chunk tickets/levels are separate (`ticket_levels.rs`, `tick_chunk_membership.rs`) | Partial [SOURCE] | Per-chunk CPU bytes (blocks, light, biomes, block entities) and the resident count at a given render + simulation distance are unmeasured. Whether the client keeps columns outside the view distance is not yet established |
| **Meshing queues and temporaries** | One shared worker pool for all background work (`horizon-threads/src/jobs.rs`): classes Urgent/Near/Far, jobs ordered by distance, sources can re-rank or drop stale jobs, results carry sequence numbers so stale work is discarded (`streaming.rs` header). Per-worker `MeshScratch` "about 150 KiB per worker" (`mesher.rs:1178`). Idle workers give back generator scratch after 5 s (`IDLE_RELEASE`) | Read [SOURCE] | Queue **depth** limits (backpressure) were not found as explicit caps; they are bounded only by the distance window. Measure queue depth during fast flight and teleports |
| **CPU/GPU duplication** | `SectionMesh` holds `solid`, `translucent`, a `packed` compact copy (`[u32;6]` per quad, "when the renderer reads it and it is smaller") and temporary `solid_tints` "dropped once that is made" (`mesher.rs:343-359`). Freed meshes are returned to a worker (`Job::Free`) so the render thread does not pay for deallocation. GUI atlas pixels are LZ4-compressed after upload (`hud.rs pack_pixels`, `lz4_flex`) | Partial [SOURCE] | Whether a section's CPU vectors are released right after upload to the arena, or retained for re-upload/sorting (translucent sorting keeps data), is not established. This is the first thing to measure for duplication |
| **GPU memory for terrain** | One vertex arena, pages up to 256 MiB, 256-vertex grain, first-fit free list with merging (`gfx/arena.rs`). `MemoryHints::MemoryUsage` was chosen to avoid whole-block waste (comment at `gfx.rs:2320`: "over half the video memory reserved was unused (1,344 MiB for 500)" on Vulkan/DX12) | Read [SOURCE] | Page size vs Metal `maxBufferLength`; how many pages are allocated at the mobile preset; fragmentation inside pages |
| **Texture and mesh caching** | Colour textures `Rgba8Unorm` (uncompressed), depth `Depth32Float` in many passes, no MSAA at the sites read. Animated-sprite mips are benchmarked (`docs/animated-sprite-mips-benchmark.md`) | Partial [SOURCE] | Total texture bytes (atlases + mips + render targets + shadow/post targets) at phone resolution. No ASTC/compressed path exists |
| **Asset loading and decoding** | Resource decoding in `horizon-assets`; resource packs in `packs*`; PNG decode with the `png` crate; sounds decoded by `symphonia` on a decoder thread into an LRU cache capped at 64 MiB (`audio.rs:151 CACHE_BYTES`); music streams (`audio.rs` header) | Partial [SOURCE] | Decoded-image retention after atlas upload; resource-pack caches; model library memory (`model_library.rs`); startup peak while loading ~23k resource files |
| **World generation and persistence** | Generation/decoration/light/meshing run as pool jobs (`job_stats::KINDS`: load, generate, decorate, finish, light, mesh, other). Saves go through `save.rs` with helper threads sized from `worker_count()` (`save.rs:2148`). Distant terrain caches reduced columns on disk (`server-terrain/<server>/<seed>`, `docs/graphics.md`) | Partial [SOURCE/DOC] | Save buffer bounds; entity/block-entity storage size (`benchmarks/rust-block-entity-storage-*`, `save-batch-entities.md` exist as project studies); LOD memory when enabled |
| **Thread counts and workloads** | `worker_count()` = available parallelism (or `HORIZON_WORKER_THREADS`); `default_workers()` = performance cores minus one on Apple Silicon via `hw.perflevel0.physicalcpu`; LAN I/O pool = half the cores; far-decoration pool = a third (`vanillagen/lod.rs:75`); the render thread and the integrated-server thread run alongside (`favour_current` raises QoS only when more than 4 cores) | Read [SOURCE] | Whether the sysctl exists on A14; real thread count in a world (the project's own comment says earlier designs reached ~100 threads on a 32-thread CPU before the shared pool). Each thread has stack and allocator-heap overhead |
| **Allocator and fragmentation** | `mimalloc` v3 as global allocator with `purge_delay = 0` ("with mimalloc's delay the game held a quarter more memory than with the system heap"), `mi_collect(true)` plus `malloc_zone_pressure_relief` on Apple in `release_free_memory()` (`horizon-threads/src/lib.rs`). `alloc-census` feature tags heap bytes by startup step and thread; `system-allocator` feature swaps in the system heap for profilers | Read [SOURCE] | Behaviour of mimalloc v3 on iOS (C build unverified), purge overhead vs footprint, and how much `mi_process_info` commit differs from `phys_footprint`. Compare with the system allocator on device |

Not yet inspected, to do in M3/M8: `SectionMesh` retention after upload, texture totals, entity model caches, resource-pack caches, save buffers, `LOD` structures, block-entity renderers, particle buffers.

## 3. Measurement design (physical iPhone only)

Simulators do not model device memory pressure or Metal memory. All numbers below come from a physical device (iPhone 12 first, a newer device second).

### 3.1 In-process telemetry (new, M3)

A 4 Hz sampler writes a JSON report (reusing the `horizon-bench` report format where it fits) and an optional overlay:

| Field | Source |
|---|---|
| `phys_footprint`, `resident`, `compressed`, `internal`/`external` where available | `task_info(TASK_VM_INFO)` |
| `os_proc_available_memory` | system call, sampled with the footprint |
| CPU heap: allocator commit/peak | `horizon_threads::allocator_commit()` (mimalloc) |
| Heap by startup step/thread | `alloc-census` build (not for timing runs: +16 bytes per block) |
| GPU: allocated bytes by category (arena pages, textures, render targets, other buffers) | wgpu allocator report plus Horizon's own counters in `gfx.rs`; Metal's current allocated size where reachable [ASSUMPTION] |
| Counts: columns resident, columns visible, sections meshing/queued/uploaded/freed | existing `streaming.rs`/`chunk_map.rs` counters, extended |
| Worker and queue depth per class (Urgent/Near/Far) | `jobs.rs` |
| Frame time mean/p50/p95/p99, dropped frames | existing `spans.rs`/`telemetry.rs` |
| Thermal state, battery level/state, low-power mode | `ProcessInfo.thermalState`, `UIDevice` |
| Events: memory warnings, background/foreground, device-lost | lifecycle hooks (M5) |

Measured OS footprint and GPU estimates are reported **separately**; GPU figures are estimates and must not be summed into a fake "total".

### 3.2 External tools

- Xcode memory gauge and Memory Graph; Instruments: Allocations, VM Tracker, Leaks, Metal System Trace, Game Performance, Time Profiler, System Trace (thread scheduling on 2+4 cores). `os_signpost` around startup phases, world join, teleport, render-distance change, save.
- Xcode's "Simulate Memory Warning"; device diagnostics for `JetsamEvent` reports after a kill.
- Metal HUD and GPU frame capture for per-frame resource lists.
- Same world and path for every comparison: fresh world seed 1234 and fixed camera paths (the project's `tools/graphics_bench.py` uses seed 1234 and four fixed 192-block paths; reproduce that route in an in-app script).

### 3.3 Standard scenarios

1. Cold launch to title; warm launch.
2. Resource load peak (title screen) and steady idle.
3. New world to first playable frame.
4. Fixed-route flight at 3 or more render distances.
5. Rapid direction changes and teleports (queue spikes, mass forgetting).
6. Render distance up and down while flying.
7. Save and reload; save during background.
8. Entity-heavy and particle-heavy scenes.
9. 20 to 30 minute soak (footprint slope, thermal state, battery).
10. 100 background/foreground cycles; injected memory warnings.

Record for every run: device model, iOS version, build id, upstream commit, game version, seed, settings, render and simulation distance, worker count, power mode, plugged/unplugged, thermal state at start.

## 4. Targets (defined as measurable acceptance rules)

Absolute thresholds are filled in after M8 from observed data; the *form* of each target is fixed now.

| Metric | Acceptance rule | How measured |
|---|---|---|
| Jetsam terminations in the standard soak | **0** across N runs (N set with the team, at least 5) | device diagnostics |
| Footprint plateau | Slope of `phys_footprint` over the last 10 minutes of a flight is indistinguishable from zero (define a tolerance from run-to-run variance) | telemetry |
| Peak footprint | Peak during the standard scenarios stays at or below a fraction of `os_proc_available_memory` measured at launch; the fraction is chosen from M8 data and written into an ADR | telemetry |
| Startup peak | Peak during resource load is below the steady-state peak of the same session, or the difference is understood and bounded | telemetry |
| Lifecycle | 100 cycles: footprint after cycle 100 within the run-to-run tolerance of cycle 1 | telemetry |
| Memory warning response | After an injected warning, footprint falls by a measured amount within a bounded time and gameplay continues | telemetry |
| Frame pacing (a product goal, not a memory limit) | p99 frame time at or below one frame at the chosen cap (for example 33.3 ms at 30 fps) in the standard flight | telemetry |
| Thermal | In the 30-minute soak, thermal state does not reach "serious" at the default preset, or the preset lowers load automatically | telemetry |
| Per-chunk cost | Resident bytes per column (CPU) and per section (GPU), reported, then reduced by measured experiments | counters |

## 5. Optimisation hypotheses (each must be validated before it ships)

Ranked by expected leverage, **all unproven**. One change per experiment; before/after captures on the same device and route; keep what moves a metric, drop what does not.

1. **Mobile preset first.** Smaller render and simulation distance, no LOD, no shader packs, no HDR, lower internal resolution. Cheapest lever and affects every other number.
2. **Worker count and queue depth.** Fix the worker count from measurement (the default may over-subscribe the 4 efficiency cores); add explicit caps on queued meshing/light/generation and on uploaded-but-unrendered sections (backpressure), instead of relying on the distance window alone.
3. **Mesh CPU copies.** Release `solid`/`translucent` vectors after upload where sorting and re-upload do not need them; confirm the `packed` layout is used on Metal; confirm `solid_tints` is dropped on every path.
4. **Arena pages.** Reduce page size on iOS to fit Metal's buffer limit and lower waste; measure free-list fragmentation after a long flight; defragmentation only if the numbers justify it.
5. **Render targets.** Count and size the `Depth32Float` and colour targets at device resolution; reuse targets across passes; lower the internal render scale; no MSAA.
6. **Textures.** Measure atlas and mip bytes. A compressed format (ASTC) would cut GPU bytes but changes pixels; it needs a visual-parity decision and must not be applied silently.
7. **Allocator.** Compare mimalloc v3 (current) with the system allocator (feature `system-allocator`) on device, including the purge setting. The desktop finding (immediate purge costs a few percent speed, saves about a quarter in memory) may not transfer.
8. **Sound.** Reduce the 64 MiB decoded-sound cache on iOS if measurement shows it matters; stream long sounds.
9. **Distant terrain.** Keep off on iOS until the budget is known; if enabled, bound its disk and memory use.
10. **Entities and block entities.** Measure storage per entity/block entity; compact only what shows up in profiles.
11. **Persistence.** Bound serialization buffers; stream region reads; never load a whole world.
12. **Memory-warning tiers.** Tier 1: drop caches and scratch (`release_free_memory`, sound cache, mesh pools); tier 2: reduce render distance temporarily; tier 3: save and return to menu. Each tier needs a measured effect.
13. **Pipelines and bind groups.** Count pipelines created at startup; avoid per-frame resource creation; measure shader compile cost (time, and memory while compiling).

## 6. Fragmentation and allocator checks

- Log mimalloc `commit`/`peak_commit` beside `phys_footprint`; the gap is allocator slack plus non-heap memory (graphics driver, thread stacks, mapped files).
- Run the `alloc-census` build once per milestone to attribute live heap to startup steps and threads; use Instruments Allocations for call stacks.
- After a 30-minute flight and a return to the spawn area, footprint should come back near its starting plateau. A permanent rise means fragmentation or an unbounded cache.
- Do not trade safety for memory: no new `unsafe` in gameplay crates (the game crate forbids it); allocator or FFI work lives in small dedicated crates.

## 7. Experiment template

```
id: MEM-0007            # one change per id
hypothesis: release SectionMesh.solid after upload lowers steady footprint
device/os/build: iPhone 12 / iOS x.y / <commit>
scenario: flight A, render distance R, seed 1234, 20 min, plugged in
baseline:  footprint plateau, p99 frame, GPU estimate, queue depth max
change:    <diff reference>
result:    same metrics, N>=5 runs each, mean and spread
decision:  keep / drop / needs follow-up    (ledger entry updated)
parity:    desktop harnesses touched by the change still pass
```

## 8. Open measurement questions

- Prepared-resource size on disk and the decoded/uploaded sizes at the title screen and in-world.
- Resident column count and bytes per column at several render and simulation distances.
- Bytes in `SectionMesh` after upload; translucent sorting retention.
- Texture totals at phone resolution; number and size of render targets.
- Thread count and stack/heap overhead in a running world on iOS.
- Does `os_proc_available_memory` at launch vary across boots and with other apps open? (It decides how a fraction-based peak target is stated.)
- Does mimalloc v3 behave well under iOS memory pressure and warnings?
