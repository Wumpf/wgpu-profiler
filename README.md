# wgpu-profiler
Simple profiler scopes for wgpu using timer queries

## Features

* Easy to use profiler scopes
  * Allows nesting!
  * Can be disabled via feature or runtime flag
  * Additionally generates debug markers 
* Internally creates pools of timer queries automatically
  * Does not need to know in advance how many queries/profiling scopes are needed
  * Caches up profiler-frames until results are available
    * No stalling of the device at any time!
* Many profiler instances can live side by side
* TODO: chrome trace flamegraph json export

### Credits / Licenses

Author: Andreas Reich

wgpu-profiler is under MIT OR Apache-2.0 license.