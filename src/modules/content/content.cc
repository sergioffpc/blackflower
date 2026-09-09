#include "content.h"

#include <sodium/core.h>
#include <sodium/crypto_hash_sha256.h>
#include <sodium/crypto_sign.h>

#include <algorithm>
#include <array>
#include <bit>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <expected>
#include <filesystem>
#include <fstream>
#include <ios>
#include <iterator>
#include <span>
#include <string_view>
#include <utility>
#include <vector>

namespace blackflower::content {
namespace {
constexpr std::size_t kHeaderSize = 112;
constexpr std::array<unsigned char, 8> kMagic{'B', 'F', 'P', 'A',
                                              'C', 'K', '1', 0};
constexpr char kPackDomain[] = "Blackflower.Pack.v1";

class Reader {
 public:
  explicit Reader(std::span<const unsigned char> bytes) : bytes_(bytes) {}

  std::span<const unsigned char> Take(std::size_t size) {
    if (size > bytes_.size()) {
      valid_ = false;
      return {};
    }
    const auto result = bytes_.first(size);
    bytes_ = bytes_.subspan(size);
    return result;
  }

  std::uint32_t U32() {
    const auto bytes = Take(4);
    std::uint32_t result = 0;
    for (std::size_t i = 0; i < bytes.size(); ++i) {
      result |= static_cast<std::uint32_t>(bytes[i]) << (8 * i);
    }
    return result;
  }

  std::uint64_t U64() {
    const auto low = U32();
    const auto high = U32();
    return low | (static_cast<std::uint64_t>(high) << 32);
  }

  std::int32_t I32() { return std::bit_cast<std::int32_t>(U32()); }

  [[nodiscard]] bool done() const { return valid_ && bytes_.empty(); }

 private:
  std::span<const unsigned char> bytes_;
  bool valid_ = true;
};

Digest Hash(std::span<const unsigned char> bytes) {
  Digest digest{};
  crypto_hash_sha256(digest.data(), bytes.data(), bytes.size());
  return digest;
}

bool ValidProvenance(std::span<const unsigned char> bytes) {
  Reader reader(bytes);
  reader.Take(64);  // Source/settings digests are opaque authenticated values.
  for (int i = 0; i < 5; ++i) {
    const auto size = reader.U32();
    if (size == 0) {
      return false;
    }
    const auto text = reader.Take(size);
    if (text.size() != size || !std::ranges::all_of(text, [](unsigned char c) {
          return c >= 32 && c <= 126;
        })) {
      return false;
    }
  }
  return reader.done();
}

bool ValidScene(const Scene& scene) {
  if (scene.interior_mm != std::array<std::uint32_t, 2>{20000, 20000} ||
      scene.capsule_mm != std::array<std::uint32_t, 2>{1800, 600} ||
      scene.wall_mm[0] < 1800 || scene.wall_mm[0] > 20000 ||
      scene.wall_mm[1] == 0 || scene.wall_mm[1] > 20000) {
    return false;
  }
  std::uint32_t previous = 0;
  for (const auto& box : scene.boxes) {
    if (box.id <= previous) {
      return false;
    }
    previous = box.id;
    for (std::size_t i = 0; i < 3; ++i) {
      if (box.size_mm[i] == 0 || box.size_mm[i] > 20000 ||
          box.size_mm[i] % 2 != 0 ||
          std::abs(static_cast<std::int64_t>(box.center_mm[i])) > 20000) {
        return false;
      }
    }
    if (static_cast<std::int64_t>(box.center_mm[1]) * 2 != box.size_mm[1]) {
      return false;
    }
    for (const auto axis : {0U, 2U}) {
      if (std::abs(static_cast<std::int64_t>(box.center_mm[axis])) +
              box.size_mm[axis] / 2 >
          10000) {
        return false;
      }
    }
  }
  bool overlap = true;
  for (std::size_t i = 0; i < 3; ++i) {
    const auto& a = scene.boxes[0];
    const auto& b = scene.boxes[1];
    if (std::abs(static_cast<std::int64_t>(a.center_mm[i]) - b.center_mm[i]) *
            2 >=
        a.size_mm[i] + b.size_mm[i]) {
      overlap = false;
    }
  }
  if (overlap) {
    return false;
  }
  previous = 0;
  for (std::size_t index = 0; index < scene.spawns.size(); ++index) {
    const auto& spawn = scene.spawns[index];
    if (spawn.id <= previous || spawn.foot_mm[1] != 0) {
      return false;
    }
    previous = spawn.id;
    for (const auto axis : {0U, 2U}) {
      if (std::abs(static_cast<std::int64_t>(spawn.foot_mm[axis])) + 300 >=
          10000) {
        return false;
      }
    }
    for (const auto& box : scene.boxes) {
      std::int64_t squared_distance = 0;
      for (const auto axis : {0U, 2U}) {
        const auto distance = std::max<std::int64_t>(
            std::abs(static_cast<std::int64_t>(spawn.foot_mm[axis]) -
                     box.center_mm[axis]) -
                box.size_mm[axis] / 2,
            0);
        squared_distance += distance * distance;
      }
      if (squared_distance <= 90000) {
        return false;
      }
    }
    for (std::size_t other = 0; other < index; ++other) {
      std::int64_t squared_distance = 0;
      for (const auto axis : {0U, 2U}) {
        const auto distance = static_cast<std::int64_t>(spawn.foot_mm[axis]) -
                              scene.spawns[other].foot_mm[axis];
        squared_distance += distance * distance;
      }
      if (squared_distance <= 360000) {
        return false;
      }
    }
  }
  return true;
}

std::expected<Scene, PackError> DecodeScene(
    std::span<const unsigned char> bytes) {
  if (bytes.size() != 152) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  Reader reader(bytes);
  Scene scene;
  for (auto& n : scene.interior_mm) {
    n = reader.U32();
  }
  for (auto& n : scene.wall_mm) {
    n = reader.U32();
  }
  for (auto& n : scene.capsule_mm) {
    n = reader.U32();
  }
  if (reader.U32() != 2 || reader.U32() != 4) {
    return std::unexpected(PackError::kInvalidPrimitiveCounts);
  }
  for (auto& box : scene.boxes) {
    box.id = reader.U32();
    for (auto& n : box.center_mm) {
      n = reader.I32();
    }
    for (auto& n : box.size_mm) {
      n = reader.U32();
    }
  }
  for (auto& spawn : scene.spawns) {
    spawn.id = reader.U32();
    for (auto& n : spawn.foot_mm) {
      n = reader.I32();
    }
  }
  if (!reader.done() || !ValidScene(scene)) {
    return std::unexpected(PackError::kInvalidScene);
  }
  return scene;
}
}  // namespace

std::string_view PackErrorMessage(PackError error) {
  switch (error) {
    case PackError::kInvalidSceneLength:
      return "invalid scene length";
    case PackError::kInvalidPrimitiveCounts:
      return "invalid primitive counts";
    case PackError::kInvalidScene:
      return "invalid scenario dimensions or references";
    case PackError::kInvalidLength:
      return "invalid pack length";
    case PackError::kCryptoInitializationFailed:
      return "cryptographic initialization failed";
    case PackError::kUnsupportedFormat:
      return "unsupported pack format";
    case PackError::kIncompatibleRoleOrProfile:
      return "wrong pack role or incompatible profile";
    case PackError::kUnsupportedResourceCount:
      return "unsupported resource count";
    case PackError::kInvalidLayout:
      return "invalid pack layout";
    case PackError::kUnknownSigningKey:
      return "unknown signing key";
    case PackError::kInvalidSignature:
      return "invalid pack signature";
    case PackError::kInvalidProvenance:
      return "invalid provenance layout";
    case PackError::kInvalidResource:
      return "invalid resource identity, schema or range";
    case PackError::kDigestMismatch:
      return "resource digest mismatch";
    case PackError::kCannotOpenFile:
      return "cannot open pack";
    case PackError::kInvalidFileLength:
      return "invalid pack file length";
    case PackError::kReadFailed:
      return "pack changed or read failed";
    case PackError::kSizeNotRepresentable:
      return "pack size is not representable by this platform";
  }
  return "unknown pack error";
}

std::expected<VerifiedPack, PackError> VerifiedPack::Load(
    std::vector<unsigned char> bytes, Role role, Profile profile,
    std::span<const PublicKey> trusted_keys) {
  if (bytes.size() < kHeaderSize + 64) {
    return std::unexpected(PackError::kInvalidLength);
  }
  if (sodium_init() < 0) {
    return std::unexpected(PackError::kCryptoInitializationFailed);
  }
  Reader header(std::span<const unsigned char>(bytes).first(kHeaderSize));
  if (!std::ranges::equal(header.Take(8), kMagic) || header.U32() != 1) {
    return std::unexpected(PackError::kUnsupportedFormat);
  }
  const auto actual_role = header.U32();
  const auto actual_profile = header.U32();
  if ((actual_role != 1 && actual_role != 2) || actual_profile != actual_role ||
      actual_role != static_cast<std::uint32_t>(role) ||
      actual_profile != static_cast<std::uint32_t>(profile)) {
    return std::unexpected(PackError::kIncompatibleRoleOrProfile);
  }
  if (header.U32() != 1) {
    return std::unexpected(PackError::kUnsupportedResourceCount);
  }
  const auto total = header.U64();
  const auto manifest_size = header.U64();
  const auto payload_size = header.U64();
  // Subtract only from verified available storage; untrusted u64 lengths
  // must not overflow before they are checked against the actual file.
  const auto body_size = bytes.size() - kHeaderSize - 64;
  if (total != bytes.size() || manifest_size < 68 ||
      manifest_size > body_size || payload_size != body_size - manifest_size) {
    return std::unexpected(PackError::kInvalidLayout);
  }
  const auto key_id = header.Take(32);
  const auto build_id = header.Take(32);
  const auto key =
      std::ranges::find_if(trusted_keys, [key_id](const auto& candidate) {
        return std::ranges::equal(Hash(candidate), key_id);
      });
  if (key == trusted_keys.end()) {
    return std::unexpected(PackError::kUnknownSigningKey);
  }
  const auto payload_start =
      kHeaderSize + static_cast<std::size_t>(manifest_size);
  std::vector<unsigned char> transcript(std::begin(kPackDomain),
                                        std::end(kPackDomain));
  transcript.insert(transcript.end(), bytes.begin(),
                    bytes.begin() + static_cast<std::ptrdiff_t>(payload_start));
  if (crypto_sign_verify_detached(bytes.data() + bytes.size() - 64,
                                  transcript.data(), transcript.size(),
                                  key->data()) != 0) {
    return std::unexpected(PackError::kInvalidSignature);
  }
  Reader manifest(std::span<const unsigned char>(bytes).subspan(kHeaderSize,
                                                                manifest_size));
  const auto provenance_size = manifest.U32();
  if (static_cast<std::uint64_t>(provenance_size) + 68 != manifest_size ||
      !ValidProvenance(manifest.Take(provenance_size))) {
    return std::unexpected(PackError::kInvalidProvenance);
  }
  if (manifest.U32() != 1 || manifest.U32() != 1 || manifest.U32() != 1 ||
      manifest.U32() != 0 || manifest.U64() != 0 ||
      manifest.U64() != payload_size) {
    return std::unexpected(PackError::kInvalidResource);
  }
  const auto digest = manifest.Take(32);
  const auto payload = std::span<const unsigned char>(bytes).subspan(
      payload_start, payload_size);
  if (!manifest.done() || !std::ranges::equal(Hash(payload), digest)) {
    return std::unexpected(PackError::kDigestMismatch);
  }
  auto scene = DecodeScene(payload);
  if (!scene) {
    return std::unexpected(scene.error());
  }
  VerifiedPack result;
  result.scene_ = *scene;
  result.pack_id_ = Hash(transcript);
  std::ranges::copy(build_id, result.build_id_.begin());
  result.bytes_ = std::move(bytes);
  return result;
}

std::expected<VerifiedPack, PackError> LoadFile(
    const std::filesystem::path& path, Role role, Profile profile,
    std::span<const PublicKey> trusted_keys) {
  std::ifstream file(path, std::ios::binary | std::ios::ate);
  if (!file) {
    return std::unexpected(PackError::kCannotOpenFile);
  }
  const auto size = static_cast<std::streamoff>(file.tellg());
  if (size < 0) {
    return std::unexpected(PackError::kInvalidFileLength);
  }
  std::vector<unsigned char> bytes;
  if (!std::in_range<std::size_t>(size) ||
      !std::in_range<std::streamsize>(size) ||
      static_cast<std::size_t>(size) > bytes.max_size()) {
    return std::unexpected(PackError::kSizeNotRepresentable);
  }
  bytes.resize(static_cast<std::size_t>(size));
  file.seekg(0);
  // Binary byte transfer: char is the stream's byte representation.
  file.read(reinterpret_cast<char*>(bytes.data()),
            static_cast<std::streamsize>(bytes.size()));
  if (!file || file.peek() != std::ifstream::traits_type::eof()) {
    return std::unexpected(PackError::kReadFailed);
  }
  return VerifiedPack::Load(std::move(bytes), role, profile, trusted_keys);
}
}  // namespace blackflower::content
