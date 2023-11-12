# wgpu-profiler
[![Crates.io](https://img.shields.io/crates/v/wgpu-profiler.svg)](https://crates.io/crates/wgpu-profiler)

Simple profiler scopes for wgpu using timer queries

## Features

* Easy to use profiler scopes
  * Allows nesting!
  * Can be disabled by runtime flag
  * Additionally generates debug markers 
  * Thread-safe - can profile several command encoder/buffers in parallel
* Internally creates pools of timer queries automatically
  * Does not need to know in advance how many queries/profiling scopes are needed
  * Caches up profiler-frames until results are available
    * No stalling of the device at any time!
* Many profiler instances can live side by side
* chrome trace flamegraph json export
* Tracy integration (behind `tracy` feature flag)

## How to use

Create a new profiler object:
```rust
use wgpu_profiler::{wgpu_profiler, GpuProfiler, GpuProfilerSettings};
// ...
let mut profiler = GpuProfiler::new(GpuProfilerSettings::default());
```

Now you can start creating profiler scopes:
```rust
// You can now open profiling scopes on any encoder or pass:
let mut scope = profiler.scope("name of your scope", &mut encoder, &device);

// Scopes can be nested arbitrarily!
let mut nested_scope = scope.scope("nested!", &device);

// Scopes on encoders can be used to easily create profiled passes!
let mut compute_pass = nested_scope.scoped_compute_pass("profiled compute", &device, &Default::default());

// Scopes expose the underlying encoder or pass they wrap:
compute_pass.set_pipeline(&pipeline);
// ...

// Scopes created this way are automatically closed when dropped.
```

`GpuProfiler` reads the device features on first use:
if your wgpu device doesn't have `wgpu::Features::TIMESTAMP_QUERY` enabled, it won't attempt to emit any timer queries.
Similarly, if `wgpu::Features::WRITE_TIMESTAMP_INSIDE_PASSES` is not present, no queries will be issued from inside passes.

Wgpu-profiler needs to insert buffer copy commands, so when you're done with an encoder and won't do any more profiling scopes on it, you need to resolve the queries:
```rust
profiler.resolve_queries(&mut encoder);
```

And finally, to end a profiling frame, call `end_frame`. This does a few checks and will let you know if something is off!
```rust
profiler.end_frame().unwrap();
```

Retrieving the oldest available frame and writing it out to a chrome trace file.
```rust
if let Some(profiling_data) = profiler.process_finished_frame(queue.get_timestamp_period()) {
    wgpu_profiler::chrometrace::write_chrometrace(std::path::Path::new("mytrace.json"), &profiling_data);
}
```


To get a look of it in action, check out the [example](./examples/demo.rs)  project!

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Changelog
* unreleased
  * ⚠️ Includes many major breaking changes! ⚠️
  * `GpuProfiler` can now be with several command buffers interleaved or in parallel!
  * `GpuProfiler::begin_scope` returns a scope and `GpuProfiler::end_scope` consumes it again
  * `Scope`/`OwningScope`/`ManualScope`/ are now all top-level in the `gpu_profiler` module
  * nesting of profiling scopes is no longer done automatically: `GpuProfiler::begin_scope` now takes an optional reference to a parent scope
  * removed profiling macro (doesn't work well with the new nesting model)
  * `GpuProfiler` can now directly create scope structs using `GpuProfiler::scope`/`owning_scope`
* 0.15
  * update to wgpu 0.18, by @Zoxc in [#50](https://github.com/Wumpf/wgpu-profiler/pull/50)
  * sample & doc fixes, by @waywardmonkeys in [#41](https://github.com/Wumpf/wgpu-profiler/pull/41), [#44](https://github.com/Wumpf/wgpu-profiler/pull/44)
  * various methods return `thiserror` errors instead of internal unwrap/except on user errors, by @Wumpf in [#45](https://github.com/Wumpf/wgpu-profiler/pull/45) and following PRs
  * overhauled `GpuProfiler` creation & configuration:
    * takes settings object that can be changed after the fact (allows disabling on the fly!)
    * adapter/queue/device no longer needed on creation unless tracy client is required.
    * separate creation method for tracy support
* 0.14.2
  * Fix pointing to wrong tracy version, by @waywardmonkeys in [#36](https://github.com/Wumpf/wgpu-profiler/pull/35)
  * Doc fixes, by @waywardmonkeys in [#38](https://github.com/Wumpf/wgpu-profiler/pull/35)
* 0.14.1
  * Tracy integration, by @cwfitzgerald in [#35](https://github.com/Wumpf/wgpu-profiler/pull/35)
* 0.13.0
  * Upgrade to wgpu 0.17, by @waywardmonkeys in [#31](https://github.com/Wumpf/wgpu-profiler/pull/31)
* 0.12.1
  * Fix wgpu validation error due to mapping of query resolve buffer, by @Davidster [#28](https://github.com/Wumpf/wgpu-profiler/pull/28)
* 0.12.0
  * Upgrade to wgpu 0.16, by @davidster in [#26](https://github.com/Wumpf/wgpu-profiler/pull/26)
* 0.11.0
  * Upgrade to wgpu 0.15
* 0.10.0
  * Upgrade to wgpu 0.14 and switch to rust 2021 edition, by @Imberflur in [#23](https://github.com/Wumpf/wgpu-profiler/pull/23)
* 0.9.1
  * Better docs [#21](https://github.com/Wumpf/wgpu-profiler/pull/21)
  * Fix crash on dropped frame [#20](https://github.com/Wumpf/wgpu-profiler/pull/20), reported by @JCapucho in [#19](https://github.com/Wumpf/wgpu-profiler/pull/19)
  * Fix enable_pass_timer/enable_encoder_timer checking wrong features
