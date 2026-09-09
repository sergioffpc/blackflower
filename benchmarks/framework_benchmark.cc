#include <benchmark/benchmark.h>

namespace blackflower {
namespace {

// Checks benchmark integration; timings do not represent a product workload.
void FrameworkSmoke(benchmark::State& state) {
  for (auto iteration : state) {
    (void)iteration;
    benchmark::DoNotOptimize(state.iterations());
  }
}

BENCHMARK(FrameworkSmoke);

}  // namespace
}  // namespace blackflower
