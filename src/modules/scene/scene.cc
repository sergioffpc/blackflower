#include "scene.h"

#include <flecs.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <functional>
#include <glm/ext/quaternion_float.hpp>
#include <glm/ext/vector_float3.hpp>
#include <limits>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "content.h"

namespace blackflower::scene {
namespace {
struct Identity {
  std::uint64_t generation;
};

struct ColliderLease {
  std::shared_ptr<const ColliderAsset> asset;
};

std::expected<std::uint64_t, SceneError> NewGeneration() {
  static std::atomic<std::uint64_t> next{1};
  auto value = next.load(std::memory_order_relaxed);
  do {
    if (value == std::numeric_limits<std::uint64_t>::max()) {
      return std::unexpected(SceneError::kIdentityExhausted);
    }
  } while (
      !next.compare_exchange_weak(value, value + 1, std::memory_order_relaxed));
  return value;
}

bool Valid(const LocalTransform& value) {
  float norm = 0;
  for (const auto v : value.rotation_xyzw) {
    norm += v * v;
  }
  const auto finite = [](float v) { return std::isfinite(v); };
  return std::ranges::all_of(value.position_m, finite) &&
         std::ranges::all_of(value.rotation_xyzw, finite) &&
         std::abs(norm - 1) <= 1e-5F && std::isfinite(value.scale) &&
         value.scale > 0;
}

glm::quat Quaternion(const std::array<float, 4>& value) {
  // GLM constructor takes WXYZ; the portable schema uses XYZW.
  return glm::quat(value[3], value[0], value[1], value[2]);
}

LocalTransform Compose(const LocalTransform& parent,
                       const LocalTransform& local) {
  const glm::vec3 position(local.position_m[0] * parent.scale,
                           local.position_m[1] * parent.scale,
                           local.position_m[2] * parent.scale);
  const auto rotated = Quaternion(parent.rotation_xyzw) * position;
  const auto rotation =
      Quaternion(parent.rotation_xyzw) * Quaternion(local.rotation_xyzw);
  return {.position_m = {parent.position_m[0] + rotated.x,
                         parent.position_m[1] + rotated.y,
                         parent.position_m[2] + rotated.z},
          .rotation_xyzw = {rotation.x, rotation.y, rotation.z, rotation.w},
          .scale = parent.scale * local.scale};
}

std::expected<std::vector<content::ColliderBox>, SceneError> TransformBoxes(
    const LocalTransform& transform, const ColliderAsset& asset) {
  if (!Valid(transform)) {
    return std::unexpected(SceneError::kInvalidTransform);
  }
  std::vector<content::ColliderBox> boxes;
  for (const auto& box : asset.boxes) {
    const auto pose = Compose(transform, {.position_m = box.center_m,
                                          .rotation_xyzw = box.rotation_xyzw});
    auto dimensions = box.dimensions_m;
    for (auto& value : dimensions) {
      value *= transform.scale;
    }
    if (!Valid(pose) || !std::ranges::all_of(dimensions, [](float v) {
          return std::isfinite(v) && v > 0;
        })) {
      return std::unexpected(SceneError::kInvalidTransform);
    }
    boxes.push_back({.center_m = pose.position_m,
                     .dimensions_m = dimensions,
                     .rotation_xyzw = pose.rotation_xyzw});
  }
  return boxes;
}

struct SceneRecord {
  std::shared_ptr<const SceneAsset> source;
  std::map<std::string, Entity, std::less<>> members;
};

struct PreparedEntity {
  const content::SceneEntityDescription* description;
  LocalTransform transform;
  Collider collider;
  ColliderLease lease;
  std::uint64_t generation;
};
}  // namespace

class SceneWorld::Impl {
 public:
  explicit Impl(ResourceManager& manager) : resources(manager) {
    // Register before publication. The minimal world never imports timers,
    // networking, logging, REST, statistics or other effectful Flecs addons.
    world.component<SceneEntityId>();
    world.component<Identity>();
    world.component<LocalTransform>();
    world.component<WorldTransform>();
    world.component<Collider>();
    world.component<ColliderLease>();
  }

  [[nodiscard]] bool Alive(Entity entity) const {
    if (entity.id == 0 || entity.generation == 0 ||
        !world.is_alive(entity.id)) {
      return false;
    }
    const auto* identity = world.entity(entity.id).try_get<Identity>();
    return identity && identity->generation == entity.generation;
  }

  std::expected<PreparedEntity, SceneError> Prepare(
      const content::SceneEntityDescription& description) {
    const auto collider = resources.Acquire(description);
    if (!collider) {
      return std::unexpected(collider.error());
    }
    const auto lease = resources.Resolve(*collider);
    if (!lease) {
      return std::unexpected(lease.error());
    }
    const LocalTransform transform{.position_m = description.position_m,
                                   .rotation_xyzw = description.rotation_xyzw,
                                   .scale = description.scale};
    const auto geometry = TransformBoxes(transform, **lease);
    const auto generation = NewGeneration();
    if (!geometry || !generation) {
      return std::unexpected(!geometry ? geometry.error() : generation.error());
    }
    return PreparedEntity{.description = &description,
                          .transform = transform,
                          .collider = {.resource = *collider},
                          .lease = {.asset = *lease},
                          .generation = *generation};
  }

  Entity Publish(const PreparedEntity& prepared) {
    auto entity = world.entity();
    entity.set(prepared.description->id);
    entity.set(Identity{.generation = prepared.generation});
    entity.set(prepared.transform);
    entity.set(WorldTransform{.value = prepared.transform});
    entity.set(prepared.collider);
    entity.set(prepared.lease);
    return {.id = entity.id(), .generation = prepared.generation};
  }

  ResourceManager& resources;
  // The C owner retains its initial reference; the C++ wrapper borrows it.
  std::unique_ptr<ecs_world_t, decltype(&ecs_fini)> storage{ecs_mini(),
                                                            ecs_fini};
  flecs::world world{storage.get()};
  std::optional<SceneRecord> scene;
};

SceneWorld::SceneWorld(ResourceManager& resources)
    : impl_(std::make_unique<Impl>(resources)) {}

SceneWorld::~SceneWorld() = default;

std::expected<void, SceneError> SceneWorld::Load(SceneHandle source) {
  if (impl_->scene) {
    return std::unexpected(SceneError::kSceneAlreadyLoaded);
  }
  const auto asset = impl_->resources.Resolve(source);
  if (!asset) {
    return std::unexpected(asset.error());
  }
  const auto& descriptions = std::visit(
      [](const auto& scene) -> const auto& { return scene.entities; },
      (*asset)->pack.scene());
  std::vector<PreparedEntity> prepared;
  for (const auto& description : descriptions) {
    auto entity = impl_->Prepare(description);
    if (!entity) {
      return std::unexpected(entity.error());
    }
    prepared.push_back(std::move(*entity));
  }
  SceneRecord record{.source = *asset, .members = {}};
  for (const auto& entity : prepared) {
    record.members.emplace(entity.description->id.value,
                           impl_->Publish(entity));
  }
  impl_->scene.emplace(std::move(record));
  return {};
}

std::expected<void, SceneError> SceneWorld::Unload() {
  if (!impl_->scene) {
    return std::unexpected(SceneError::kNoScene);
  }
  for (const auto& [scene_entity_id, entity] : impl_->scene->members) {
    if (impl_->Alive(entity)) {
      impl_->world.entity(entity.id).destruct();
    }
  }
  impl_->scene.reset();
  return {};
}

std::optional<Entity> SceneWorld::GetEntity(
    const SceneEntityId& scene_entity_id) const {
  if (!impl_->scene) {
    return std::nullopt;
  }
  const auto member = impl_->scene->members.find(scene_entity_id.value);
  if (member == impl_->scene->members.end() || !impl_->Alive(member->second)) {
    return std::nullopt;
  }
  return member->second;
}

std::expected<EntityState, SceneError> SceneWorld::ReadEntityState(
    Entity entity) const {
  if (!impl_->Alive(entity)) {
    return std::unexpected(SceneError::kStaleEntity);
  }
  const auto current = impl_->world.entity(entity.id);
  return EntityState{.scene_entity_id = current.get<SceneEntityId>(),
                     .local = current.get<LocalTransform>(),
                     .world = current.get<WorldTransform>(),
                     .collider = current.get<Collider>()};
}

std::expected<void, SceneError> SceneWorld::SetTransform(
    Entity entity, LocalTransform transform) {
  if (!impl_->Alive(entity)) {
    return std::unexpected(SceneError::kStaleEntity);
  }
  const auto current = impl_->world.entity(entity.id);
  const auto geometry =
      TransformBoxes(transform, *current.get<ColliderLease>().asset);
  if (!geometry) {
    return std::unexpected(geometry.error());
  }
  current.set(transform);
  current.set(WorldTransform{.value = transform});
  return {};
}

std::expected<std::vector<content::ColliderBox>, SceneError>
SceneWorld::WorldColliders(Entity entity) const {
  if (!impl_->Alive(entity)) {
    return std::unexpected(SceneError::kStaleEntity);
  }
  const auto current = impl_->world.entity(entity.id);
  return TransformBoxes(current.get<WorldTransform>().value,
                        *current.get<ColliderLease>().asset);
}

std::expected<void, SceneError> SceneWorld::Destroy(Entity entity) {
  if (!impl_->scene || !impl_->Alive(entity)) {
    return std::unexpected(SceneError::kStaleEntity);
  }
  const auto current = impl_->world.entity(entity.id);
  impl_->scene->members.erase(current.get<SceneEntityId>().value);
  current.destruct();
  return {};
}

std::size_t SceneWorld::entity_count() const {
  return static_cast<std::size_t>(impl_->world.count<Identity>());
}
}  // namespace blackflower::scene
