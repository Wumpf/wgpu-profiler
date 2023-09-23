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
* Tracy integration (behind `tracy` feature flag)

TODO:
* Disable via feature flag

## How to use

Create a new profiler object:
```rust
use wgpu_profiler::{wgpu_profiler, GpuProfiler};
// ...
let mut profiler = GpuProfiler::new(&adapter, &device, &queue, 4); // buffer up to 4 frames
```

Using scopes is easiest with the macro:
```rust
wgpu_profiler!("name of your scope", &mut profiler, &mut encoder, &device, {
  // wgpu commands go here
});
```
Note that `GpuProfiler` reads the device features - if your wgpu device doesn't have `wgpu::Features::TIMESTAMP_QUERY` enabled, it will automatically not attempt to emit any timer queries.
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
if let Some(profiling_data) = profiler.process_finished_frame() {
    // You usually want to write to disk only under some condition, e.g. press of a key or button
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
  * sample & doc fixes, by @waywardmonkeys in [#41](https://github.com/Wumpf/wgpu-profiler/pull/41), [#44](https://github.com/Wumpf/wgpu-profiler/pull/44)
  * `end_scope` & `end_frame` return `thiserror` errors instead of internal unwrap/except on user errors, by @Wumpf in [#45](https://github.com/Wumpf/wgpu-profiler/pull/45)
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
