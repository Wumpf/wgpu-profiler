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
* Puffin integration (behind `puffin` feature flag)

## How to use

Create a new profiler object:
```rust
use wgpu_profiler::{wgpu_profiler, GpuProfiler, GpuProfilerSettings};
// ...
let mut profiler = GpuProfiler::new(&device, GpuProfilerSettings::default());
```

Now you can start creating profiler scopes:
```rust
// You can now open profiling scopes on any encoder or pass:
let mut scope = profiler.scope("name of your scope", &mut encoder);

// Scopes can be nested arbitrarily!
let mut nested_scope = scope.scope("nested!");

// Scopes on encoders can be used to easily create profiled passes!
let mut compute_pass = nested_scope.scoped_compute_pass("profiled compute");

// Scopes expose the underlying encoder or pass they wrap:
compute_pass.set_pipeline(&pipeline);
// ...

// Scopes created this way are automatically closed when dropped.
```

`GpuProfiler` reads the device features on first use:

* `wgpu::Features::TIMESTAMP_QUERY` is required to emit any timer queries.
  * Alone, this allows you to use timestamp writes on pass definition as done by `Scope::scoped_compute_pass`/`Scope::scoped_render_pass`
* `wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS` is required to issue queries at any point within encoders.
* `wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES` is required to issue queries at any point within passes.

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
