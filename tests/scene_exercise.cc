#include "scene_exercise.h"

#include <array>
#include <cmath>
#include <cstddef>
#include <expected>
#include <iostream>
#include <limits>

#include "content.h"
#include "scene.h"

namespace {
namespace runtime = blackflower::scene;

bool Check(bool condition, const char* description) {
  if (!condition) {
    std::cerr << "scene check failed: " << description << '\n';
  }
  return condition;
}

template <typename T>
bool Error(const std::expected<T, runtime::SceneError>& result,
           runtime::SceneError expected) {
  return !result && result.error() == expected;
}

struct Instances {
  runtime::SceneInstance first;
  runtime::SceneInstance second;
  runtime::Entity first_box;
  runtime::Entity second_box;
};

template <std::size_t N>
bool Near(const std::array<double, N>& actual,
          const std::array<double, N>& expected, double tolerance) {
  for (std::size_t i = 0; i < N; ++i) {
    if (std::abs(actual[i] - expected[i]) > tolerance) {
      return false;
    }
  }
  return true;
}

bool CheckWorldGeometry(runtime::SceneWorld& world,
                        const Instances& instances) {
  const auto first = world.WorldColliders(instances.first_box);
  const auto second = world.WorldColliders(instances.second_box);
  if (!first || !second || first->size() != 2 || second->size() != 2) {
    return false;
  }
  return Check(
      Near((*first)[1].center_m, {2, 1, 1}, 1e-9) &&
          Near((*first)[0].dimensions_m, {2, 2, 2}, 1e-9) &&
          Near((*first)[0].rotation_xyzw,
               {0, 0.7071067811865476, 0, 0.7071067811865476}, 1e-12) &&
          Near((*second)[0].rotation_xyzw, {0, 1, 0, 0}, 1e-12),
      "analytical world geometry and composed orientation");
}

bool CheckFailedPreparationCache(runtime::SceneWorld& world,
                                 runtime::ResourceManager& resources,
                                 runtime::SceneHandle source) {
  const auto initial = world.Instantiate(source);
  if (!initial) {
    return false;
  }
  const auto entity = world.Find(*initial, "box-01");
  if (!entity) {
    return false;
  }
  const auto state = world.Read(*entity);
  if (!state || !world.Unload(*initial)) {
    return false;
  }
  const auto rejected = world.Instantiate(source, {.scale = 1e307});
  return Check(Error(rejected, runtime::SceneError::kInvalidTransform) &&
                   resources.Resolve(state->collider.resource).has_value() &&
                   resources.Resolve(source).has_value() &&
                   world.entity_count() == 0,
               "failed preparation preserves previously cached resources");
}

bool CheckGeometry(runtime::SceneWorld& world, const Instances& instances) {
  const auto first = world.Read(instances.first_box);
  const auto second = world.Read(instances.second_box);
  const auto boxes = world.WorldColliders(instances.second_box);
  if (!Check(first && second && boxes && boxes->size() == 2,
             "complete live geometry")) {
    return false;
  }
  std::cout << "{\"cap_center\":[" << (*boxes)[1].center_m[0] << ','
            << (*boxes)[1].center_m[1] << ',' << (*boxes)[1].center_m[2]
            << "],\"body_dimensions\":[" << (*boxes)[0].dimensions_m[0] << ','
            << (*boxes)[0].dimensions_m[1] << ',' << (*boxes)[0].dimensions_m[2]
            << ']';
  return Check(first->collider.resource == second->collider.resource &&
                   first->prototype == second->prototype &&
                   first->instance != second->instance &&
                   first->local.position_m == std::array<double, 3>{2, 1, 3},
               "shared collider and independent identity namespaces");
}

bool CheckIndependence(runtime::SceneWorld& world, const Instances& instances) {
  const auto changed =
      world.SetTransform(instances.first_box, {.position_m = {30, 4, 5}});
  const auto second = world.Read(instances.second_box);
  if (!Check(changed && second &&
                 std::abs(second->world.value.position_m[0] - 10) < 1e-9,
             "changing one entity preserves the other")) {
    return false;
  }
  const auto invalid = world.SetTransform(
      instances.second_box, {.scale = std::numeric_limits<double>::infinity()});
  const auto preserved = world.Read(instances.second_box);
  return Check(Error(invalid, runtime::SceneError::kInvalidTransform) &&
                   preserved && preserved->local.scale == 4,
               "invalid update preserves previous live transform");
}

bool CheckRejectedActivation(runtime::SceneWorld& world,
                             runtime::SceneHandle source,
                             const Instances& instances) {
  const auto invalid = world.Instantiate(source, {.scale = 1e307});
  const auto zero = world.Instantiate(source, {.scale = 0});
  const auto stale = world.Instantiate({});
  const auto first = world.Read(instances.first_box);
  const auto second = world.Read(instances.second_box);
  return Check(Error(invalid, runtime::SceneError::kInvalidTransform) &&
                   Error(zero, runtime::SceneError::kInvalidTransform) &&
                   Error(stale, runtime::SceneError::kStaleResource) && first &&
                   second && world.entity_count() == 4,
               "failed activation publishes no partial entities");
}

bool CheckWorldIsolation(runtime::ResourceManager& resources,
                         const Instances& instances,
                         runtime::SceneHandle source) {
  runtime::SceneWorld other(resources);
  const auto independent = other.Instantiate(source);
  return Check(independent &&
                   Error(other.Read(instances.first_box),
                         runtime::SceneError::kStaleEntity) &&
                   Error(other.Unload(instances.first),
                         runtime::SceneError::kStaleInstance) &&
                   other.Unload(*independent),
               "runtime handles cannot cross world namespaces");
}

bool CheckTransfer(runtime::SceneWorld& world, const Instances& instances) {
  const auto collision = world.Transfer(instances.first_box, instances.second);
  const auto destroy = world.Destroy(instances.second_box);
  const auto transfer = world.Transfer(instances.first_box, instances.second);
  const auto unload = world.Unload(instances.first);
  const auto survivor = world.Read(instances.first_box);
  const auto old_lookup = world.Find(instances.first, "box-01");
  const auto new_lookup = world.Find(instances.second, "box-01");
  return Check(Error(collision, runtime::SceneError::kDuplicatePrototype) &&
                   destroy && transfer && unload && survivor && new_lookup &&
                   *new_lookup == instances.first_box &&
                   survivor->instance == instances.second &&
                   survivor->local.position_m[0] == 30 &&
                   Error(old_lookup, runtime::SceneError::kStaleInstance) &&
                   Error(world.Read(instances.second_box),
                         runtime::SceneError::kStaleEntity) &&
                   world.entity_count() == 2,
               "transferred membership survives its original instance");
}

bool CheckUnload(runtime::SceneWorld& world,
                 runtime::ResourceManager& resources,
                 const Instances& instances, runtime::ColliderHandle collider) {
  const auto pinned = resources.Resolve(collider);
  const auto unloaded = world.Unload(instances.second);
  const auto again = world.Unload(instances.second);
  resources.CollectUnused();
  return Check(
      pinned && unloaded && Error(again, runtime::SceneError::kStaleInstance) &&
          Error(world.Read(instances.first_box),
                runtime::SceneError::kStaleEntity) &&
          resources.Resolve(collider).has_value() && world.entity_count() == 0,
      "full unload preserves an external resource lease");
}

bool CheckReload(runtime::SceneWorld& world,
                 runtime::ResourceManager& resources,
                 const blackflower::content::VerifiedPack& pack,
                 runtime::ColliderHandle old_collider,
                 runtime::Entity old_entity) {
  resources.CollectUnused();
  if (!Check(Error(resources.Resolve(old_collider),
                   runtime::SceneError::kStaleResource),
             "resource eviction")) {
    return false;
  }
  const auto source = resources.Load(pack);
  if (!source) {
    return false;
  }
  const auto instance = world.Instantiate(*source);
  if (!instance) {
    return false;
  }
  const auto entity = world.Find(*instance, "box-01");
  if (!entity) {
    return false;
  }
  const auto state = world.Read(*entity);
  return Check(
      state && state->collider.resource.slot == old_collider.slot &&
          state->collider.resource.generation != old_collider.generation &&
          Error(world.Read(old_entity), runtime::SceneError::kStaleEntity) &&
          world.Unload(*instance),
      "entity and resource generations reject stale handles");
}

bool CheckLifecycle(runtime::SceneWorld& world,
                    runtime::ResourceManager& resources,
                    const blackflower::content::VerifiedPack& pack,
                    runtime::SceneHandle source, const Instances& instances) {
  const auto state = world.Read(instances.first_box);
  return state && CheckWorldGeometry(world, instances) &&
         CheckGeometry(world, instances) &&
         CheckIndependence(world, instances) &&
         CheckRejectedActivation(world, source, instances) &&
         CheckWorldIsolation(resources, instances, source) &&
         CheckTransfer(world, instances) &&
         CheckUnload(world, resources, instances, state->collider.resource) &&
         Error(resources.Resolve(source),
               runtime::SceneError::kStaleResource) &&
         CheckReload(world, resources, pack, state->collider.resource,
                     instances.first_box);
}
}  // namespace

bool ExerciseScene(const blackflower::content::VerifiedPack& pack) {
  runtime::ResourceManager resources;
  runtime::SceneWorld world(resources);
  const auto source = resources.Load(pack);
  if (!source || !CheckFailedPreparationCache(world, resources, *source)) {
    return false;
  }
  const auto first = world.Instantiate(*source);
  const auto second = world.Instantiate(
      *source, {.position_m = {4, 3, -2},
                .rotation_xyzw = {0, std::sqrt(0.5), 0, std::sqrt(0.5)},
                .scale = 2});
  if (!first || !second) {
    return false;
  }
  const auto first_box = world.Find(*first, "box-01");
  const auto second_box = world.Find(*second, "box-01");
  if (!first_box || !second_box) {
    return false;
  }
  const Instances instances{.first = *first,
                            .second = *second,
                            .first_box = *first_box,
                            .second_box = *second_box};
  if (!CheckLifecycle(world, resources, pack, *source, instances)) {
    return false;
  }
  std::cout << ",\"lifecycle_verified\":true}\n";
  return true;
}
