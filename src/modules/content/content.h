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

using Digest = std::array<unsigned char, 32>;
using PublicKey = std::array<unsigned char, 32>;
enum class Role : std::uint8_t { kClient = 1, kServer = 2 };
enum class Profile : std::uint8_t {
  kWindowsPrimitives = 1,
  kLinuxPrimitives = 2
};

// Failure categories are independent of diagnostic text.
enum class PackError : std::uint8_t {
  kInvalidSceneLength,
  kInvalidPrimitiveCounts,
  kInvalidScene,
  kInvalidLength,
  kCryptoInitializationFailed,
  kUnsupportedFormat,
  kIncompatibleRoleOrProfile,
  kUnsupportedResourceCount,
  kInvalidLayout,
  kUnknownSigningKey,
  kInvalidSignature,
  kInvalidProvenance,
  kInvalidResource,
  kDigestMismatch,
  kCannotOpenFile,
  kInvalidFileLength,
  kReadFailed,
  kSizeNotRepresentable,
};

// Diagnostic text for presentation only; branch on PackError values.
std::string_view PackErrorMessage(PackError error);

struct Box {
  std::uint32_t id = 0;
  std::array<std::int32_t, 3> center_mm{};
  std::array<std::uint32_t, 3> size_mm{};
};

struct Spawn {
  std::uint32_t id = 0;
  std::array<std::int32_t, 3> foot_mm{};
};

struct Scene {
  std::array<std::uint32_t, 2> interior_mm{};
  std::array<std::uint32_t, 2> wall_mm{};
  std::array<std::uint32_t, 2> capsule_mm{};
  std::array<Box, 2> boxes{};
  std::array<Spawn, 4> spawns{};
};

// Load only outside ECS. Trust and the required role/profile belong to the
// application, never the pack. Success owns the exact verified bytes and
// exposes immutable project values, with no SDK resources or borrowed storage.
class VerifiedPack {
 public:
  static std::expected<VerifiedPack, PackError> Load(
      std::vector<unsigned char> bytes, Role role, Profile profile,
      std::span<const PublicKey> trusted_keys);

  [[nodiscard]] const Scene& scene() const { return scene_; }

  [[nodiscard]] const Digest& pack_id() const { return pack_id_; }

  [[nodiscard]] const Digest& scenario_build_id() const { return build_id_; }

  [[nodiscard]] std::span<const unsigned char> bytes() const { return bytes_; }

 private:
  VerifiedPack() = default;
  std::vector<unsigned char> bytes_;
  Scene scene_;
  Digest pack_id_{};
  Digest build_id_{};
};

// Single-open file ingestion without a policy size cap. Callers may supply
// bytes directly. Failure never returns partial content.
std::expected<VerifiedPack, PackError> LoadFile(
    const std::filesystem::path& path, Role role, Profile profile,
    std::span<const PublicKey> trusted_keys);

}  // namespace blackflower::content
#endif  // BLACKFLOWER_CONTENT_CONTENT_H_
