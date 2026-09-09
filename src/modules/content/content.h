#ifndef BLACKFLOWER_CONTENT_CONTENT_H_
#define BLACKFLOWER_CONTENT_CONTENT_H_

#include <array>
#include <cstdint>
#include <expected>
#include <filesystem>
#include <span>
#include <string_view>
#include <vector>

namespace blackflower::content {

// Pack authentication and scene preparation. Format contracts are maintained in
// schemas/pack/v1.md and schemas/scene/v1.md.

// SHA-256 identity and raw Ed25519 verification key, respectively.
using Digest = std::array<unsigned char, 32>;
using PublicKey = std::array<unsigned char, 32>;
enum class Role : std::uint8_t { kClient = 1, kServer = 2 };
// Identifies the cooked representation, not the host running the verifier.
enum class Profile : std::uint8_t {
  kWindowsPrimitives = 1,
  kLinuxPrimitives = 2
};

// Failure conditions exposed by pack ingestion, authentication and validation.
enum class PackError : std::uint8_t {
  // Scene payload length differs from the layout required by its schema.
  kInvalidSceneLength,
  // Declared box or spawn counts do not match the scene schema.
  kInvalidPrimitiveCounts,
  // Decoded geometry or object identities violate the scene contract.
  kInvalidScene,
  // Input bytes are too short to contain the pack header and signature.
  kInvalidLength,
  // The cryptographic backend could not initialize for verification.
  kCryptoInitializationFailed,
  // The pack magic or format version is not supported.
  kUnsupportedFormat,
  // The embedded role/profile pair is unsupported or differs from the pair
  // required by the caller.
  kIncompatibleRoleOrProfile,
  // The declared resource count is unsupported by the pack profile.
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
  // violates the selected profile.
  kInvalidResource,
  // The payload hash differs from the authenticated resource digest, or the
  // resource record does not consume the complete manifest.
  kDigestMismatch,
  // The file could not be opened and positioned for reading.
  kCannotOpenFile,
  // The stream could not report a nonnegative file length.
  kInvalidFileLength,
  // Reading failed or the file length changed during ingestion.
  kReadFailed,
  // The file length cannot be represented by the byte container or stream.
  // This does not report allocation failure or a policy size cap.
  kSizeNotRepresentable,
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

// Participant capsule placement, measured at the feet in XYZ coordinates.
struct Spawn {
  std::uint32_t id = 0;
  std::array<std::int32_t, 3> foot_mm{};
};

// Prepared primitive geometry in the coordinate system defined by scene v1.
struct Scene {
  // Width and depth of the interior.
  std::array<std::uint32_t, 2> interior_mm{};
  // Height and thickness of the enclosing walls.
  std::array<std::uint32_t, 2> wall_mm{};
  // Total height, including both hemispheres, and diameter.
  std::array<std::uint32_t, 2> capsule_mm{};
  std::array<Box, 2> boxes{};
  std::array<Spawn, 4> spawns{};
};

// Owns authenticated bytes and the validated scene decoded from them. Accessor
// references and views borrow this object's storage; do not retain them across
// destruction, assignment, or moving the pack.
class VerifiedPack {
 public:
  // Authenticates a complete artifact and validates its scene before exposing
  // content. The application supplies the required role, profile and trusted
  // keys independently of the artifact; keys are not retained. After ownership
  // transfer, callers must not mutate the bytes through retained aliases.
  // Performs preparation outside ECS execution; creates no runtime SDK
  // resources.
  static std::expected<VerifiedPack, PackError> Load(
      std::vector<unsigned char> bytes, Role role, Profile profile,
      std::span<const PublicKey> trusted_keys);

  [[nodiscard]] const Scene& scene() const { return scene_; }

  // Identifies this role-specific artifact.
  [[nodiscard]] const Digest& pack_id() const { return pack_id_; }

  // Authenticated identity shared by the matching role packs. Verifying one
  // artifact does not establish that its counterpart is available or valid.
  [[nodiscard]] const Digest& scenario_build_id() const { return build_id_; }

  [[nodiscard]] std::span<const unsigned char> bytes() const { return bytes_; }

 private:
  VerifiedPack() = default;
  std::vector<unsigned char> bytes_;
  Scene scene_;
  Digest pack_id_{};
  Digest build_id_{};
};

// Opens the file once and applies VerifiedPack::Load's trust and validation
// contract to the bytes read. Ingestion uses memory proportional to file size;
// it does not stream resources. Call outside ECS execution.
std::expected<VerifiedPack, PackError> LoadFile(
    const std::filesystem::path& path, Role role, Profile profile,
    std::span<const PublicKey> trusted_keys);

}  // namespace blackflower::content
#endif  // BLACKFLOWER_CONTENT_CONTENT_H_
