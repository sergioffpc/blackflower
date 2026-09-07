#include <benchmark/benchmark.h>

namespace blackflower {
namespace {

// Exercise the benchmark harness until product workloads are implemented.
void FrameworkSmoke(benchmark::State& state) {
  for (auto iteration : state) {
    (void)iteration;
    benchmark::DoNotOptimize(state.iterations());
  }
}
BENCHMARK(FrameworkSmoke);

}  // namespace
}  // namespace blackflower
