#ifndef BLACKFLOWER_CONTENT_CONTENT_H_
#define BLACKFLOWER_CONTENT_CONTENT_H_

#include <array>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <filesystem>
#include <memory>
#include <span>
#include <string_view>
#include <variant>
#include <vector>

namespace blackflower::content {

// Pack authentication and scene preparation. Format contracts are maintained in
// schemas/pack/v1.md and schemas/scene/v1.md.

// SHA-256 identity and raw Ed25519 verification key, respectively.
using Digest = std::array<unsigned char, 32>;
using PublicKey = std::array<unsigned char, 32>;
// Selects the content's purpose independently of deployment or host platform.
enum class Role : std::uint8_t {
  // World rules and geometry shared by simulation and prediction.
  kSimulation = 1,
  // Resources used to render and present the scenario.
  kPresentation = 2
};

// Preserve the full encoded discriminator so unknown values cannot truncate.
// NOLINTNEXTLINE(performance-enum-size)
enum class GeometryKind : std::uint32_t {
  // Axis-aligned box with a centre and full extents.
  kBox = 1,
  // Sphere with a centre and radius.
  kSphere = 2
};

// Preserve the full encoded discriminator so unknown values cannot truncate.
// NOLINTNEXTLINE(performance-enum-size)
enum class LightKind : std::uint32_t {
  // Omnidirectional emitter at a position.
  kPoint = 1,
  // Parallel emitter with a scene-space direction of travel.
  kDirectional = 2
};

// Failure conditions exposed by pack ingestion, authentication and validation.
enum class PackError : std::uint8_t {
  // Scene payload length differs from the layout required by its schema.
  kInvalidSceneLength,
  // A geometry record uses a geometry tag unsupported by this schema.
  kUnsupportedGeometry,
  // A light record uses a kind unsupported by this schema.
  kUnsupportedLight,
  // Input bytes are too short to contain the pack header and signature.
  kInvalidLength,
  // The cryptographic backend could not initialize for verification.
  kCryptoInitializationFailed,
  // The pack magic or format version is not supported.
  kUnsupportedFormat,
  // The embedded role is unsupported or differs from the caller's requirement.
  kIncompatibleRole,
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

// Axis-aligned solid with XYZ coordinates and full extents in millimetres.
struct Box {
  std::uint32_t id = 0;
  std::array<std::int32_t, 3> center_mm{};
  std::array<std::uint32_t, 3> size_mm{};
};

// Sphere centre and radius in millimetres.
struct Sphere {
  std::uint32_t id = 0;
  std::array<std::int32_t, 3> center_mm{};
  std::uint32_t radius_mm = 0;
};

using Geometry = std::variant<Box, Sphere>;

// Omnidirectional point emitter. Color is linear RGB; intensity is a
// dimensionless multiplier of that color, independent of a rendering backend.
struct PointLight {
  std::uint32_t id = 0;
  std::array<std::int32_t, 3> position_mm{};
  std::array<float, 3> color{};
  float intensity = 0;
};

// Parallel emitter. Direction is a scene-space vector along light travel;
// consumers normalize it. Color and intensity use PointLight's convention.
struct DirectionalLight {
  std::uint32_t id = 0;
  std::array<float, 3> direction{};
  std::array<float, 3> color{};
  float intensity = 0;
};

using Light = std::variant<PointLight, DirectionalLight>;

// Placement origin in XYZ millimetres; consumers define what is spawned.
struct Spawn {
  std::uint32_t id = 0;
  std::array<std::int32_t, 3> position_mm{};
};

// Scene contents in the coordinate system defined by scene v1. Collections may
// be empty; no enclosure or participant dimensions are implied.
struct Scene {
  std::vector<Geometry> geometries;
  std::vector<Light> lights;
  std::vector<Spawn> spawns;
};

// Owns authenticated bytes and the structurally validated scene decoded from
// them. Accessor references and views borrow this object's storage; do not
// retain them across destruction, assignment, or moving the pack.
class VerifiedPack {
 public:
  // Authenticates a complete artifact and checks scene encoding, without
  // evaluating geometry or gameplay rules. The application supplies the
  // required role and trusted keys independently of the artifact; keys are not
  // retained. After ownership transfer, callers must not mutate the bytes
  // through retained aliases. Performs preparation outside ECS execution;
  // creates no runtime SDK resources.
  static std::expected<VerifiedPack, PackError> Load(
      std::vector<unsigned char> bytes, Role role,
      std::span<const PublicKey> trusted_keys);

  [[nodiscard]] const Scene& scene() const { return scene_; }

  // Identifies this role-specific artifact.
  [[nodiscard]] const Digest& pack_id() const { return pack_id_; }

  // Authenticated identity shared by the matching role packs. Verifying one
  // artifact does not establish that its counterpart is available or valid.
  [[nodiscard]] const Digest& scenario_build_id() const { return build_id_; }

  [[nodiscard]] std::span<const unsigned char> bytes() const {
    return {data_.get(), data_ ? size_ : 0};
  }

 private:
  VerifiedPack() = default;
  static std::expected<VerifiedPack, PackError> LoadStorage(
      std::shared_ptr<const unsigned char> data, std::size_t size, Role role,
      std::span<const PublicKey> trusted_keys);
  friend std::expected<VerifiedPack, PackError> LoadFile(
      const std::filesystem::path& path, Role role,
      std::span<const PublicKey> trusted_keys);

  std::shared_ptr<const unsigned char> data_;
  std::size_t size_ = 0;
  Scene scene_;
  Digest pack_id_{};
  Digest build_id_{};
};

// Maps a file read-only and verifies it without copying the complete artifact.
// The file must remain unchanged while any pack copy owns the mapping. Windows
// denies writes and deletion; Linux requires the publisher to enforce this.
// Hash verification touches all payload bytes; decoded scene values use
// separate allocations. Call outside ECS execution.
std::expected<VerifiedPack, PackError> LoadFile(
    const std::filesystem::path& path, Role role,
    std::span<const PublicKey> trusted_keys);

}  // namespace blackflower::content
#endif  // BLACKFLOWER_CONTENT_CONTENT_H_
