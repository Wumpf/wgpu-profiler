# Change Log

## 0.24.0
* Update to wgpu 26.0.0, by @waywardmonkeys in [#95](https://github.com/Wumpf/wgpu-profiler/pull/95)

## 0.23.0
* Update to wgpu 25.0.0, by @waywardmonkeys in [#95](https://github.com/Wumpf/wgpu-profiler/pull/95)

## 0.22.0
* Device no longer needs to be passed for scope/start-query/end-query, in [#93](https://github.com/Wumpf/wgpu-profiler/pull/93)

## 0.21.1
* Add accessor for settings.

## 0.21.0
* Integration with puffin, by @gonkalbell in [#81](https://github.com/Wumpf/wgpu-profiler/pull/81)

## 0.20.0
* Update to tracy-client 0.18.0, update to thiserror 2, in [#91](https://github.com/Wumpf/wgpu-profiler/pull/91)
* Update to wgpu 24.0.0, by @songhuaixu in [#90](https://github.com/Wumpf/wgpu-profiler/pull/90)

## 0.19.0
* Update to wgpu 23.0.0, by @waywardmonkeys in [#88](https://github.com/Wumpf/wgpu-profiler/pull/88)

## 0.18.2
* Fix `resolve_queries` calls per frame causing invalid buffer copy operations, in [#83](https://github.com/Wumpf/wgpu-profiler/pull/83)

## 0.18.1
* Fix issues with multiple calls to `GpuProfiler::resolve_queries` per frame, in [#79](https://github.com/Wumpf/wgpu-profiler/pull/79)

## 0.18.0
* Update to wgpu 22.1.0, by @waywardmonkeys in [#75](https://github.com/Wumpf/wgpu-profiler/pull/75)

## 0.17.0
* Update to wgpu 0.20
* `GpuTimerQueryResult` are now also produced when timing is disabled for that scope
  * `GpuTimerQueryResult::time` is an `Option` now
* Update tracy client to 0.17.0

## 0.16.2

* Updating to wgpu 0.19.3 thus removing the need for pinned web-sys, by @xStrom in [#65](https://github.com/Wumpf/wgpu-profiler/pull/65)

## 0.16.1

* Fix building for wasm, by @davidster in [#62](https://github.com/Wumpf/wgpu-profiler/pull/62)

## 0.16

* update to wgpu 0.19
* ⚠️ Includes many major breaking changes! ⚠️
  * `GpuProfiler` can now be used with several command buffers interleaved or in parallel!
    * `Scope`/`OwningScope`/`ManualScope`/ are now all top-level in the `gpu_profiler` module. `GpuProfiler` has utilities to create them directly.
  * `GpuProfiler::begin_query` returns a query and `GpuProfiler::end_query` consumes it again
  * nesting of profiling scopes is no longer done automatically: To manually associate a `GpuProfilerQuery` with a parent, use `GpuProfilerQuery::with_parent`
  * removed profiling macro (doesn't work well with the new nesting model)
  * `GpuProfiler` can now directly create scope structs using `GpuProfiler::scope`/`owning_scope`

## 0.15

* update to wgpu 0.18, by @Zoxc in [#50](https://github.com/Wumpf/wgpu-profiler/pull/50)
* sample & doc fixes, by @waywardmonkeys in [#41](https://github.com/Wumpf/wgpu-profiler/pull/41), [#44](https://github.com/Wumpf/wgpu-profiler/pull/44)
* various methods return `thiserror` errors instead of internal unwrap/except on user errors, by @Wumpf in [#45](https://github.com/Wumpf/wgpu-profiler/pull/45) and following PRs
* overhauled `GpuProfiler` creation & configuration:
  * takes settings object that can be changed after the fact (allows disabling on the fly!)
  * adapter/queue/device no longer needed on creation unless tracy client is required.
  * separate creation method for tracy support

## 0.14.2

* Fix pointing to wrong tracy version, by @waywardmonkeys in [#36](https://github.com/Wumpf/wgpu-profiler/pull/35)
* Doc fixes, by @waywardmonkeys in [#38](https://github.com/Wumpf/wgpu-profiler/pull/35)

## 0.14.1

* Tracy integration, by @cwfitzgerald in [#35](https://github.com/Wumpf/wgpu-profiler/pull/35)

## 0.13.0

* Upgrade to wgpu 0.17, by @waywardmonkeys in [#31](https://github.com/Wumpf/wgpu-profiler/pull/31)

## 0.12.1

* Fix wgpu validation error due to mapping of query resolve buffer, by @Davidster [#28](https://github.com/Wumpf/wgpu-profiler/pull/28)

## 0.12.0

* Upgrade to wgpu 0.16, by @davidster in [#26](https://github.com/Wumpf/wgpu-profiler/pull/26)

## 0.11.0

* Upgrade to wgpu 0.15

## 0.10.0

* Upgrade to wgpu 0.14 and switch to rust 2021 edition, by @Imberflur in [#23](https://github.com/Wumpf/wgpu-profiler/pull/23)

## 0.9.1

* Better docs [#21](https://github.com/Wumpf/wgpu-profiler/pull/21)
* Fix crash on dropped frame [#20](https://github.com/Wumpf/wgpu-profiler/pull/20), reported by @JCapucho in [#19](https://github.com/Wumpf/wgpu-profiler/pull/19)
* Fix enable_pass_timer/enable_encoder_timer checking wrong features
