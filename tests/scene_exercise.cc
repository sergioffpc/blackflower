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

const runtime::SceneEntityId kBoxId{.value = "box-01"};
const runtime::SceneEntityId kFloorId{.value = "floor-main"};

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

template <std::size_t N>
bool Near(const std::array<float, N>& actual,
          const std::array<float, N>& expected, float tolerance) {
  for (std::size_t i = 0; i < N; ++i) {
    if (std::abs(actual[i] - expected[i]) > tolerance) {
      return false;
    }
  }
  return true;
}

struct SceneEntities {
  runtime::Entity box;
  runtime::Entity floor;
};

bool CheckWorldGeometry(runtime::SceneWorld& world, runtime::Entity box) {
  const auto boxes = world.WorldColliders(box);
  if (!boxes || boxes->size() != 2) {
    return false;
  }
  std::cout << "{\"cap_center\":[" << (*boxes)[1].center_m[0] << ','
            << (*boxes)[1].center_m[1] << ',' << (*boxes)[1].center_m[2]
            << "],\"body_dimensions\":[" << (*boxes)[0].dimensions_m[0] << ','
            << (*boxes)[0].dimensions_m[1] << ',' << (*boxes)[0].dimensions_m[2]
            << ']';
  return Check(Near((*boxes)[1].center_m, {2, 1, 1}, 1e-5F) &&
                   Near((*boxes)[0].dimensions_m, {2, 2, 2}, 1e-5F) &&
                   Near((*boxes)[0].rotation_xyzw,
                        {0, 0.70710677F, 0, 0.70710677F}, 1e-5F),
               "authored placement produces analytical world geometry");
}

bool CheckSingleScene(runtime::SceneWorld& world, runtime::SceneHandle source,
                      const SceneEntities& entities) {
  const auto duplicate = world.Load(source);
  const auto preserved_box = world.GetEntity(kBoxId);
  const auto preserved_floor = world.GetEntity(kFloorId);
  return Check(Error(duplicate, runtime::SceneError::kSceneAlreadyLoaded) &&
                   preserved_box && *preserved_box == entities.box &&
                   preserved_floor && *preserved_floor == entities.floor &&
                   world.entity_count() == 2,
               "one world rejects a second scene without changing its state");
}

bool CheckMutation(runtime::SceneWorld& world, const SceneEntities& entities) {
  const auto floor_before = world.ReadEntityState(entities.floor);
  const auto changed =
      world.SetTransform(entities.box, {.position_m = {30, 4, 5}});
  const auto floor_after = world.ReadEntityState(entities.floor);
  const auto invalid = world.SetTransform(
      entities.box, {.scale = std::numeric_limits<float>::infinity()});
  const auto box_after = world.ReadEntityState(entities.box);
  return Check(
      floor_before && changed && floor_after &&
          floor_after->local.position_m == floor_before->local.position_m &&
          Error(invalid, runtime::SceneError::kInvalidTransform) && box_after &&
          box_after->local.position_m == std::array<float, 3>{30, 4, 5},
      "entity mutation is isolated and invalid updates are atomic");
}

bool CheckWorldIsolation(runtime::SceneWorld& world,
                         runtime::ResourceManager& resources,
                         runtime::SceneHandle source, runtime::Entity box) {
  runtime::SceneWorld other(resources);
  const auto loaded = other.Load(source);
  const auto other_box = other.GetEntity(kBoxId);
  if (!loaded || !other_box) {
    Check(false, "each world loads its own scene");
    return false;
  }
  const auto original_state = world.ReadEntityState(box);
  const auto other_state = other.ReadEntityState(*other_box);
  return Check(
      loaded && other_box && *other_box != box && original_state &&
          other_state &&
          other_state->collider.resource == original_state->collider.resource &&
          Error(other.ReadEntityState(box),
                runtime::SceneError::kStaleEntity) &&
          other.Unload(),
      "worlds isolate entities and share immutable resources");
}

bool CheckDestroy(runtime::SceneWorld& world, runtime::Entity floor) {
  const auto destroyed = world.Destroy(floor);
  const auto missing = world.GetEntity(kFloorId);
  return Check(
      destroyed && !missing &&
          Error(world.Destroy(floor), runtime::SceneError::kStaleEntity) &&
          world.entity_count() == 1,
      "destroy removes the scene member and rejects its stale handle");
}

bool CheckUnloadAndReload(runtime::SceneWorld& world,
                          runtime::ResourceManager& resources,
                          const blackflower::content::VerifiedPack& pack,
                          runtime::SceneHandle old_source,
                          runtime::ColliderHandle old_collider,
                          runtime::Entity old_entity) {
  if (!Check(world.Unload().has_value() &&
                 Error(world.Unload(), runtime::SceneError::kNoScene) &&
                 !world.GetEntity(kBoxId) &&
                 Error(world.ReadEntityState(old_entity),
                       runtime::SceneError::kStaleEntity) &&
                 world.entity_count() == 0,
             "unload empties the world and invalidates its entities")) {
    return false;
  }
  resources.CollectUnused();
  if (!Check(Error(resources.Resolve(old_source),
                   runtime::SceneError::kStaleResource) &&
                 Error(resources.Resolve(old_collider),
                       runtime::SceneError::kStaleResource),
             "unload releases scene and collider resources for collection")) {
    return false;
  }
  const auto source = resources.Load(pack);
  if (!source || !world.Load(*source)) {
    return false;
  }
  const auto box = world.GetEntity(kBoxId);
  if (!box) {
    return false;
  }
  const auto state = world.ReadEntityState(*box);
  return Check(state && *box != old_entity &&
                   state->collider.resource != old_collider &&
                   Error(world.ReadEntityState(old_entity),
                         runtime::SceneError::kStaleEntity) &&
                   world.Unload() &&
                   Error(world.Load({}), runtime::SceneError::kStaleResource),
               "reload advances entity and resource generations");
}
}  // namespace

bool ExerciseScene(const blackflower::content::VerifiedPack& pack) {
  runtime::ResourceManager resources;
  runtime::SceneWorld world(resources);
  const auto source = resources.Load(pack);
  if (!source || !world.Load(*source)) {
    return false;
  }
  const auto box = world.GetEntity(kBoxId);
  const auto floor = world.GetEntity(kFloorId);
  if (!box || !floor) {
    return false;
  }
  const SceneEntities entities{.box = *box, .floor = *floor};
  const auto state = world.ReadEntityState(entities.box);
  if (!state || !CheckWorldGeometry(world, entities.box) ||
      !CheckSingleScene(world, *source, entities) ||
      !CheckMutation(world, entities) ||
      !CheckWorldIsolation(world, resources, *source, entities.box) ||
      !CheckDestroy(world, entities.floor) ||
      !CheckUnloadAndReload(world, resources, pack, *source,
                            state->collider.resource, entities.box)) {
    return false;
  }
  std::cout << ",\"lifecycle_verified\":true}\n";
  return true;
}
