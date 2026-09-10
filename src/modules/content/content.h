#ifndef BLACKFLOWER_CONTENT_CONTENT_H_
#define BLACKFLOWER_CONTENT_CONTENT_H_

#include <array>
#include <compare>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <filesystem>
#include <memory>
#include <span>
#include <string>
#include <string_view>
#include <variant>
#include <vector>

namespace blackflower::content {

// Pack authentication and scene preparation. Format contracts are maintained in
// schemas/pack/v1.md and schemas/scene/v1.md.

// SHA-256 identity and raw Ed25519 verification key, respectively.
using Digest = std::array<unsigned char, 32>;
using PublicKey = std::array<unsigned char, 32>;
// Failure conditions exposed by pack ingestion, authentication and validation.
enum class PackError : std::uint8_t {
  // Scene length or collection counts violate the concrete scene's layout.
  kInvalidSceneLength,
  // Entity IDs violate their ASCII grammar or strictly increasing order.
  kInvalidEntity,
  // Nonfinite geometry, nonpositive dimensions/scale or a non-unit quaternion.
  kInvalidTransform,
  // A collision shape record uses a kind unsupported by this schema.
  kUnsupportedCollider,
  // Input bytes are too short to contain the pack header and signature.
  kInvalidLength,
  // The cryptographic backend could not initialize for verification.
  kCryptoInitializationFailed,
  // The pack magic or format version is not supported.
  kUnsupportedFormat,
  // The declared resource count is unsupported by the pack format.
  kUnsupportedResourceCount,
  // Declared total, manifest and payload lengths do not partition the input
  // into a complete pack with the required manifest structure.
  kInvalidLayout,
  // The signing-key identifier matches none of the caller's trusted keys.
  kUnknownSigningKey,
  // The signature does not authenticate the header and manifest with the
  // selected trusted key.
  kInvalidSignature,
  // Provenance length, field encoding or text violates the manifest contract.
  kInvalidProvenance,
  // A resource's identity, type, schema, reserved field or payload range
  // violates the pack format.
  kInvalidResource,
  // The payload hash differs from the authenticated resource digest, or the
  // resource record does not consume the complete manifest.
  kDigestMismatch,
  // Opening or mapping the file failed inside the Boost adapter.
  kCannotMapFile,
  // Allocation failed inside the mapping adapter; other allocation failures
  // outside that exception boundary remain unrecoverable.
  kMappingAllocationFailed,

};

// Returns static diagnostic text. Callers classify failures by the enum value;
// message wording is not part of the error contract.
std::string_view PackErrorMessage(PackError error);

// Independently authored entity-local box. Dimensions are full extents in
// metres; rotation is a unit XYZW quaternion.
struct ColliderBox {
  std::array<double, 3> center_m{};
  std::array<double, 3> dimensions_m{};
  std::array<double, 4> rotation_xyzw{};
  bool operator==(const ColliderBox&) const = default;
};

// Content-addressed compiled asset identity; all-zero is reserved as invalid.
struct AssetId {
  Digest bytes{};
  auto operator<=>(const AssetId&) const = default;
};

// Scene-local authored identity, independent of USD prim paths and runtime IDs.
struct PrototypeId {
  std::string value;
  auto operator<=>(const PrototypeId&) const = default;
};

// Persistent ASCII identity, placement and optional local colliders. Position
// is metres, rotation is a unit XYZW quaternion, scale is positive and uniform.
struct Prototype {
  PrototypeId id;
  std::array<double, 3> position_m{};
  std::array<double, 4> rotation_xyzw{};
  double scale = 1;
  std::vector<ColliderBox> colliders;
  AssetId collider_asset_id;
};

// Each role currently receives the same entity content. Future role-specific
// resources can evolve independently under these concrete scene types.
struct ServerScene {
  std::vector<Prototype> entities;
};

struct AgentScene {
  std::vector<Prototype> entities;
};

struct ClientScene {
  std::vector<Prototype> entities;
};

// The authenticated file magic selects the alternative; collections may be
// empty.
using Scene = std::variant<ServerScene, AgentScene, ClientScene>;

// Owns authenticated bytes and the structurally validated scene decoded from
// them. Accessor references borrow this object's storage; do not
// retain them across destruction, assignment, or moving the pack.
class VerifiedPack {
 public:
  // Authenticates a complete artifact and checks scene encoding, without
  // evaluating geometry or gameplay rules. The application supplies the
  // trusted keys independently of the artifact; keys are not
  // retained. After ownership transfer, callers must not mutate the bytes
  // through retained aliases. Performs preparation outside ECS execution;
  // creates no runtime SDK resources.
  static std::expected<VerifiedPack, PackError> Load(
      std::vector<unsigned char> bytes,
      std::span<const PublicKey> trusted_keys);

  [[nodiscard]] const Scene& scene() const { return scene_; }

  [[nodiscard]] AssetId asset_id() const { return asset_id_; }

  // Authenticated identity of the source, settings and cooked resources.
  [[nodiscard]] const Digest& content_build_id() const {
    return content_build_id_;
  }

 private:
  VerifiedPack() = default;
  static std::expected<VerifiedPack, PackError> LoadStorage(
      std::shared_ptr<const unsigned char> data, std::size_t size,
      std::span<const PublicKey> trusted_keys);
  friend std::expected<VerifiedPack, PackError> LoadFile(
      const std::filesystem::path& path,
      std::span<const PublicKey> trusted_keys);

  std::shared_ptr<const unsigned char> data_;
  Scene scene_;
  Digest content_build_id_{};
  AssetId asset_id_;
};

// Maps a file read-only and verifies it without copying the complete artifact.
// The file must remain unchanged while any pack copy owns the mapping. Windows
// denies writes and deletion; Linux requires the publisher to enforce this.
// Hash verification touches all payload bytes; decoded scene values use
// separate allocations. Call outside ECS execution.
std::expected<VerifiedPack, PackError> LoadFile(
    const std::filesystem::path& path, std::span<const PublicKey> trusted_keys);

}  // namespace blackflower::content
#endif  // BLACKFLOWER_CONTENT_CONTENT_H_
