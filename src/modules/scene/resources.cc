#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <limits>
#include <memory>
#include <utility>
#include <variant>
#include <vector>

#include "content.h"
#include "scene.h"

namespace blackflower::scene {
namespace {
template <typename Asset>
struct Slot {
  std::uint64_t generation = 1;
  std::shared_ptr<const Asset> asset;
};

template <typename Asset>
void EvictUnused(std::vector<Slot<Asset>>& slots) {
  for (auto& slot : slots) {
    if (slot.asset && slot.asset.use_count() == 1) {
      slot.asset.reset();
      slot.generation =
          slot.generation == std::numeric_limits<std::uint64_t>::max()
              ? 0
              : slot.generation + 1;
    }
  }
}

// Exact value comparison catches digest collisions without treating differences
// in authenticated provenance as differences in compiled scene identity.
bool SameSceneEntityDescriptions(const content::VerifiedPack& a,
                                 const content::VerifiedPack& b) {
  const auto descriptions = [](const auto& pack) -> const auto& {
    return std::visit(
        [](const auto& scene) -> const auto& { return scene.entities; },
        pack.scene());
  };
  return descriptions(a) == descriptions(b);
}
}  // namespace

class ResourceManager::Impl {
 public:
  template <typename Asset>
  static std::expected<std::shared_ptr<const Asset>, SceneError> ResolveSlot(
      const std::vector<Slot<Asset>>& slots, ResourceHandle<Asset> handle,
      const ResourceManager* owner) {
    if (handle.identity_.owner != owner || handle.identity_.generation == 0 ||
        handle.identity_.slot >= slots.size()) {
      return std::unexpected(SceneError::kStaleResource);
    }
    const auto& slot = slots[handle.identity_.slot];
    if (slot.generation != handle.identity_.generation || !slot.asset) {
      return std::unexpected(SceneError::kStaleResource);
    }
    return slot.asset;
  }

  template <typename Asset>
  static ResourceHandle<Asset> Publish(std::vector<Slot<Asset>>& slots,
                                       std::shared_ptr<const Asset> asset,
                                       const ResourceManager* owner) {
    auto slot = std::ranges::find_if(slots, [](const auto& candidate) {
      return !candidate.asset && candidate.generation != 0;
    });
    if (slot == slots.end()) {
      slots.emplace_back();
      slot = slots.end() - 1;
    }
    slot->asset = std::move(asset);
    return ResourceHandle<Asset>(
        {.owner = owner,
         .slot = static_cast<std::size_t>(slot - slots.begin()),
         .generation = slot->generation});
  }

  std::vector<Slot<SceneAsset>> scenes;
  std::vector<Slot<ColliderAsset>> colliders;
};

ResourceManager::ResourceManager() : impl_(std::make_unique<Impl>()) {}

ResourceManager::~ResourceManager() = default;

std::expected<SceneHandle, SceneError> ResourceManager::Load(
    content::VerifiedPack pack) {
  for (std::size_t i = 0; i < impl_->scenes.size(); ++i) {
    const auto& slot = impl_->scenes[i];
    if (slot.asset && slot.asset->pack.asset_id() == pack.asset_id()) {
      if (!SameSceneEntityDescriptions(slot.asset->pack, pack)) {
        return std::unexpected(SceneError::kIdentityCollision);
      }
      return SceneHandle(
          {.owner = this, .slot = i, .generation = slot.generation});
    }
  }
  return Impl::Publish(
      impl_->scenes,
      std::make_shared<const SceneAsset>(SceneAsset{.pack = std::move(pack)}),
      this);
}

std::expected<ColliderHandle, SceneError> ResourceManager::Acquire(
    const content::SceneEntityDescription& description) {
  if (!description.collision) {
    return std::unexpected(SceneError::kMissingCollider);
  }
  const auto& collision = *description.collision;
  for (std::size_t i = 0; i < impl_->colliders.size(); ++i) {
    const auto& slot = impl_->colliders[i];
    if (slot.asset && slot.asset->id == collision.asset_id) {
      if (slot.asset->boxes != collision.boxes) {
        return std::unexpected(SceneError::kIdentityCollision);
      }
      return ColliderHandle(
          {.owner = this, .slot = i, .generation = slot.generation});
    }
  }
  return Impl::Publish(impl_->colliders,
                       std::make_shared<const ColliderAsset>(ColliderAsset{
                           .id = collision.asset_id, .boxes = collision.boxes}),
                       this);
}

std::expected<std::shared_ptr<const SceneAsset>, SceneError>
ResourceManager::Resolve(SceneHandle handle) const {
  return Impl::ResolveSlot(impl_->scenes, handle, this);
}

std::expected<std::shared_ptr<const ColliderAsset>, SceneError>
ResourceManager::Resolve(ColliderHandle handle) const {
  return Impl::ResolveSlot(impl_->colliders, handle, this);
}

void ResourceManager::CollectUnused() {
  EvictUnused(impl_->scenes);
  EvictUnused(impl_->colliders);
}
}  // namespace blackflower::scene
