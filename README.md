# wgpu-profiler
[![Crates.io](https://img.shields.io/crates/v/wgpu-profiler.svg)](https://crates.io/crates/wgpu-profiler)

Simple profiler scopes for wgpu using timer queries

## Features

* Easy to use profiler scopes
  * Allows nesting!
  * Can be disabled by runtime flag
  * Additionally generates debug markers 
* Internally creates pools of timer queries automatically
  * Does not need to know in advance how many queries/profiling scopes are needed
  * Caches up profiler-frames until results are available
    * No stalling of the device at any time!
* Many profiler instances can live side by side
* chrome trace flamegraph json export

TODO:
* Better error messages
* Disable via feature flag

## How to use

Create a new profiler object:
```rust
use wgpu_profiler::{wgpu_profiler, GpuProfiler};
// ...
let mut profiler = GpuProfiler::new(4, adapter.get_timestamp_period()); // buffer up to 4 frames
```

Using scopes is easiest with the macro:
```rust
wgpu_profiler!("name of your scope", &mut profiler, &mut encoder, &device, {
  // wgpu commands go here
});
```
Unless you disable timer scoping (`wgpu_profile` will still emit debug scopes), your wgpu device needs `wgpu::Features::TIMESTAMP_QUERY` enabled.

Wgpu-profiler needs to insert buffer copy commands, so when you're done with an encoder and won't do any more profiling scopes on it, you need to resolve the queries:
```rust
profiler.resolve_queries(&mut encoder);
```

And finally, to end a profiling frame, call `end_frame`. This does a few checks and will let you know of something is off!
```rust
profiler.end_frame().unwrap();
```

Retrieving the oldest available frame and writing it out to a chrome trace file (don't do that every frame 😉).
```rust
if let Some(profiling_data) = profiler.process_finished_frame() {
    wgpu_profiler::chrometrace::write_chrometrace(Path::new("mytrace.json"), profiling_data);
}
```


To get a look of it in action, check out the example project!

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