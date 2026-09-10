#ifndef BLACKFLOWER_SCENE_SCENE_H_
#define BLACKFLOWER_SCENE_SCENE_H_

#include <array>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <memory>
#include <string_view>
#include <vector>

#include "content.h"

namespace blackflower::scene {

// Scene-local authored placement identity. Runtime lookups also require an
// instance, so identical IDs in different instances never alias.
using PrototypeId = content::PrototypeId;

// Shared spatial definition, used independently by each ECS world. Metres,
// right-handed Y-up, unit XYZW rotation, positive uniform scale.
struct LocalTransform {
  std::array<double, 3> position_m{};
  std::array<double, 4> rotation_xyzw{0, 0, 0, 1};
  double scale = 1;
};

// Derived spatial definition; never a second mutable transform authority.
struct WorldTransform {
  LocalTransform value;
};

// Immutable compiled collision definition, shared by Simulation and Prediction.
struct ColliderAsset {
  content::AssetId id;
  std::vector<content::ColliderBox> boxes;
};

// Immutable recipe and authenticated backing storage. ResourceManager publishes
// only const access. No live entity state is stored here.
struct SceneAsset {
  content::VerifiedPack pack;
};

class ResourceManager;

// Opaque borrowed identity, valid only in its originating manager's lifetime.
// Default construction produces an invalid handle. Copying and comparison do
// not keep resources alive; only ResourceManager can issue valid handles.
template <typename Asset>
class ResourceHandle {
 public:
  ResourceHandle() = default;
  bool operator==(const ResourceHandle&) const = default;

 private:
  friend class ResourceManager;

  struct Identity {
    const ResourceManager* owner = nullptr;
    std::size_t slot = 0;
    std::uint64_t generation = 0;
    bool operator==(const Identity&) const = default;
  };

  explicit ResourceHandle(Identity identity) : identity_(identity) {}

  Identity identity_;
};

using SceneHandle = ResourceHandle<SceneAsset>;
using ColliderHandle = ResourceHandle<ColliderAsset>;

// ECS Simulation/Prediction collision reference. Geometry stays in the manager.
struct Collider {
  ColliderHandle resource;
};

enum class SceneError : std::uint8_t {
  // Missing, evicted, wrong-manager or generation-mismatched resource.
  kStaleResource,
  // Missing, unloaded or wrong-world scene instance.
  kStaleInstance,
  // Destroyed, wrong-world or generation-mismatched entity.
  kStaleEntity,
  // No live member with the requested authored identity in this instance.
  kUnknownPrototype,
  // Transfer would duplicate an authored identity in the destination instance.
  kDuplicatePrototype,
  // Nonfinite placement/derived geometry, nonpositive scale, nonunit rotation.
  kInvalidTransform,
  // Identical AssetIds name unequal compiled geometry/recipes.
  kIdentityCollision,
  // Monotonic runtime identity space is exhausted; no identity is wrapped.
  kIdentityExhausted,
};

// Single-threaded immutable resource publication, outside world progression.
// Resolve returns an owning lease; CollectUnused evicts only manager-only
// resources. Keep this manager alive until all its worlds have been destroyed.
class ResourceManager {
 public:
  ResourceManager();
  ~ResourceManager();
  ResourceManager(const ResourceManager&) = delete;
  ResourceManager& operator=(const ResourceManager&) = delete;
  ResourceManager(ResourceManager&&) = delete;
  ResourceManager& operator=(ResourceManager&&) = delete;

  std::expected<SceneHandle, SceneError> Load(content::VerifiedPack pack);
  [[nodiscard]] std::expected<std::shared_ptr<const SceneAsset>, SceneError>
  Resolve(SceneHandle handle) const;
  [[nodiscard]] std::expected<std::shared_ptr<const ColliderAsset>, SceneError>
  Resolve(ColliderHandle handle) const;
  void CollectUnused();

 private:
  friend class SceneWorld;
  std::expected<ColliderHandle, SceneError> Acquire(
      const content::Prototype& recipe);
  class Impl;
  std::unique_ptr<Impl> impl_;
};

// Process-unique generation namespaces; zero is invalid. Never serialized.
struct Entity {
  std::uint64_t id = 0;
  std::uint64_t generation = 0;
  bool operator==(const Entity&) const = default;
};

struct SceneInstance {
  std::uint64_t generation = 0;
  bool operator==(const SceneInstance&) const = default;
};

// Read-only observation copied from live ECS components, not authoritative
// state.
struct EntityState {
  PrototypeId prototype;
  SceneInstance instance;
  LocalTransform local;
  WorldTransform world;
  Collider collider;
};

// Owns a real headless Flecs world. All calls are synchronous at exclusive
// world synchronization points, never from ECS callbacks or concurrent threads.
// Instantiation validates every transformed box before publishing any entity.
// Recoverable errors leave existing state intact. Allocation/SDK aborts are
// process failures, not recoverable SceneErrors. No external SDKs execute here.
class SceneWorld {
 public:
  explicit SceneWorld(ResourceManager& resources);
  ~SceneWorld();
  SceneWorld(const SceneWorld&) = delete;
  SceneWorld& operator=(const SceneWorld&) = delete;
  SceneWorld(SceneWorld&&) = delete;
  SceneWorld& operator=(SceneWorld&&) = delete;

  std::expected<SceneInstance, SceneError> Instantiate(
      SceneHandle source, LocalTransform root = {});
  // Repeated unload returns kStaleInstance. Transferred members survive.
  std::expected<void, SceneError> Unload(SceneInstance instance);
  [[nodiscard]] std::expected<Entity, SceneError> Find(
      SceneInstance instance, std::string_view prototype) const;
  [[nodiscard]] std::expected<EntityState, SceneError> Read(
      Entity entity) const;
  // Root entities store authoritative world placement in LocalTransform.
  std::expected<void, SceneError> SetTransform(Entity entity,
                                               LocalTransform transform);
  [[nodiscard]] std::expected<std::vector<content::ColliderBox>, SceneError>
  WorldColliders(Entity entity) const;
  // Transfer preserves current world placement and the member's resource lease.
  std::expected<void, SceneError> Transfer(Entity entity,
                                           SceneInstance destination);
  std::expected<void, SceneError> Destroy(Entity entity);
  [[nodiscard]] std::size_t entity_count() const;

 private:
  class Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace blackflower::scene
#endif  // BLACKFLOWER_SCENE_SCENE_H_
