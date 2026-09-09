#include "content.h"

#include <sodium/core.h>
#include <sodium/crypto_hash_sha256.h>
#include <sodium/crypto_sign.h>

#include <algorithm>
#include <array>
#include <bit>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <filesystem>
#include <iterator>
#include <memory>
#include <span>
#include <string_view>
#include <type_traits>
#include <utility>
#include <vector>

#include "mapped_file.h"

namespace blackflower::content {
namespace {
constexpr std::array<unsigned char, 8> kServerMagic{'B', 'F', 'S', 'E',
                                                    'R', 'V', '1', 0};
constexpr std::array<unsigned char, 8> kAgentMagic{'B', 'F', 'A', 'G',
                                                   'N', 'T', '1', 0};
constexpr std::array<unsigned char, 8> kClientMagic{'B', 'F', 'C', 'L',
                                                    'N', 'T', '1', 0};
enum class SceneKind : std::uint8_t { kServer, kAgent, kClient };

constexpr std::size_t kDigestSize = crypto_hash_sha256_BYTES;
constexpr std::size_t kSignatureSize = crypto_sign_BYTES;
// Pack v1: magic, version/count, total/manifest/payload sizes, key/build
// IDs.
constexpr std::size_t kHeaderSize = kServerMagic.size() +
                                    2 * sizeof(std::uint32_t) +
                                    3 * sizeof(std::uint64_t) + 2 * kDigestSize;
// Resource record: identity/type/schema/reserved, offset/size, payload digest.
constexpr std::size_t kResourceRecordSize =
    4 * sizeof(std::uint32_t) + 2 * sizeof(std::uint64_t) + kDigestSize;
// The manifest contains a provenance length and one resource record.
constexpr std::size_t kManifestFixedSize =
    sizeof(std::uint32_t) + kResourceRecordSize;
constexpr char kPackDomain[] = "Blackflower.Pack.v1";
constexpr std::uint32_t kSceneResourceId = 1;
constexpr std::uint32_t kSceneSchemaVersion = 1;
constexpr std::uint32_t kReservedResourceValue = 0;
constexpr std::uint64_t kSingleResourceOffset = 0;

enum class ResourceType : std::uint8_t {
  // CollisionShape, light and spawn collections encoded by the scene schema.
  kScene = 1
};

// Borrows little-endian encoded storage. Checked scene reads return typed
// errors; pack metadata readers check completion through done().
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

  // Decodes one scene scalar; a failed read permanently invalidates the reader.
  template <typename T>
    requires(std::same_as<T, std::uint32_t> || std::same_as<T, std::int32_t> ||
             std::same_as<T, float>)
  std::expected<T, PackError> Read() {
    if (!valid_ || bytes_.size() < sizeof(std::uint32_t)) {
      valid_ = false;
      return std::unexpected(PackError::kInvalidSceneLength);
    }
    return std::bit_cast<T>(U32());
  }

  template <typename T, std::size_t N>
  std::expected<std::array<T, N>, PackError> ReadArray() {
    std::array<T, N> values{};
    for (auto& value : values) {
      const auto decoded = Read<T>();
      if (!decoded) {
        return std::unexpected(decoded.error());
      }
      value = *decoded;
    }
    return values;
  }

  [[nodiscard]] bool done() const { return valid_ && bytes_.empty(); }

 private:
  std::span<const unsigned char> bytes_;
  bool valid_ = true;
};

// Temporary decoded collections; the file kind determines the public scene
// type.
struct DecodedScene {
  std::vector<CollisionShape> collision_shapes;
  std::vector<Light> lights;
  std::vector<Spawn> spawns;
};

// Views borrow the input artifact; ranges have been checked against its size.
struct PackLayout {
  SceneKind scene_kind;
  std::span<const unsigned char> manifest;
  std::span<const unsigned char> payload;
  std::span<const unsigned char> key_id;
  std::span<const unsigned char> content_build_id;
  std::span<const unsigned char> signed_bytes;
  std::span<const unsigned char> signature;
};

std::expected<SceneKind, PackError> DecodeMagic(
    std::span<const unsigned char> magic) {
  if (std::ranges::equal(magic, kServerMagic)) {
    return SceneKind::kServer;
  }
  if (std::ranges::equal(magic, kAgentMagic)) {
    return SceneKind::kAgent;
  }
  if (std::ranges::equal(magic, kClientMagic)) {
    return SceneKind::kClient;
  }
  return std::unexpected(PackError::kUnsupportedFormat);
}

// Consumes the header prefix and identifies the scene encoding.
std::expected<SceneKind, PackError> ValidateHeader(Reader& header) {
  const auto kind = DecodeMagic(header.Take(kServerMagic.size()));
  if (!kind) {
    return std::unexpected(kind.error());
  }
  if (header.U32() != 1) {
    return std::unexpected(PackError::kUnsupportedFormat);
  }
  if (header.U32() != 1) {
    return std::unexpected(PackError::kUnsupportedResourceCount);
  }
  return *kind;
}

// Requires space for the fixed header and signature. Returned views borrow
// bytes.
std::expected<PackLayout, PackError> DecodeLayout(
    std::span<const unsigned char> bytes) {
  Reader header(bytes.first(kHeaderSize));
  const auto compatible = ValidateHeader(header);
  if (!compatible) {
    return std::unexpected(compatible.error());
  }
  const auto total = header.U64();
  const auto manifest_size = header.U64();
  const auto payload_size = header.U64();
  // Check actual storage before subtraction; summing untrusted sizes can wrap.
  const auto body_size = bytes.size() - kHeaderSize - kSignatureSize;
  if (total != bytes.size() || manifest_size < kManifestFixedSize ||
      manifest_size > body_size || payload_size != body_size - manifest_size) {
    return std::unexpected(PackError::kInvalidLayout);
  }
  const auto key_id = header.Take(kDigestSize);
  const auto content_build_id = header.Take(kDigestSize);
  const auto payload_start =
      kHeaderSize + static_cast<std::size_t>(manifest_size);
  return PackLayout{.scene_kind = *compatible,
                    .manifest = bytes.subspan(kHeaderSize, manifest_size),
                    .payload = bytes.subspan(payload_start, payload_size),
                    .key_id = key_id,
                    .content_build_id = content_build_id,
                    .signed_bytes = bytes.first(payload_start),
                    .signature = bytes.last(kSignatureSize)};
}

// Computes SHA-256 over the exact bytes, without canonicalizing the input.
Digest Hash(std::span<const unsigned char> bytes) {
  Digest digest{};
  crypto_hash_sha256(digest.data(), bytes.data(), bytes.size());
  return digest;
}

// Verifies the header and manifest signature against a trusted public key.
std::expected<void, PackError> VerifySignature(
    const PackLayout& layout, std::span<const PublicKey> trusted_keys) {
  const auto key =
      std::ranges::find_if(trusted_keys, [&layout](const auto& candidate) {
        return std::ranges::equal(Hash(candidate), layout.key_id);
      });
  if (key == trusted_keys.end()) {
    return std::unexpected(PackError::kUnknownSigningKey);
  }
  // Reserialization could change the message. The terminating zero belongs to
  // the pack v1 signature domain.
  std::vector<unsigned char> transcript(std::begin(kPackDomain),
                                        std::end(kPackDomain));
  transcript.insert(transcript.end(), layout.signed_bytes.begin(),
                    layout.signed_bytes.end());
  if (crypto_sign_verify_detached(layout.signature.data(), transcript.data(),
                                  transcript.size(), key->data()) != 0) {
    return std::unexpected(PackError::kInvalidSignature);
  }
  return {};
}

std::expected<Box, PackError> DecodeBox(Reader& reader) {
  const auto id = reader.Read<std::uint32_t>();
  if (!id) {
    return std::unexpected(id.error());
  }
  const auto center = reader.ReadArray<std::int32_t, 3>();
  if (!center) {
    return std::unexpected(center.error());
  }
  const auto size = reader.ReadArray<std::uint32_t, 3>();
  if (!size) {
    return std::unexpected(size.error());
  }
  return Box{.id = *id, .center_mm = *center, .size_mm = *size};
}

std::expected<Sphere, PackError> DecodeSphere(Reader& reader) {
  const auto id = reader.Read<std::uint32_t>();
  if (!id) {
    return std::unexpected(id.error());
  }
  const auto center = reader.ReadArray<std::int32_t, 3>();
  if (!center) {
    return std::unexpected(center.error());
  }
  const auto radius = reader.Read<std::uint32_t>();
  if (!radius) {
    return std::unexpected(radius.error());
  }
  return Sphere{.id = *id, .center_mm = *center, .radius_mm = *radius};
}

std::expected<CollisionShape, PackError> DecodeCollisionShape(Reader& reader) {
  const auto kind = reader.Read<std::uint32_t>();
  if (!kind) {
    return std::unexpected(kind.error());
  }
  switch (static_cast<CollisionShapeKind>(*kind)) {
    case CollisionShapeKind::kBox:
      return DecodeBox(reader);
    case CollisionShapeKind::kSphere:
      return DecodeSphere(reader);
  }
  return std::unexpected(PackError::kUnsupportedCollisionShape);
}

template <typename T>
std::expected<T, PackError> DecodeLightBody(Reader& reader) {
  const auto id = reader.Read<std::uint32_t>();
  if (!id) {
    return std::unexpected(id.error());
  }
  using Coordinate =
      std::conditional_t<std::same_as<T, PointLight>, std::int32_t, float>;
  const auto coordinates = reader.ReadArray<Coordinate, 3>();
  if (!coordinates) {
    return std::unexpected(coordinates.error());
  }
  const auto color = reader.ReadArray<float, 3>();
  if (!color) {
    return std::unexpected(color.error());
  }
  const auto intensity = reader.Read<float>();
  if (!intensity) {
    return std::unexpected(intensity.error());
  }
  T light;
  light.id = *id;
  if constexpr (std::same_as<T, PointLight>) {
    light.position_mm = *coordinates;
  } else {
    light.direction = *coordinates;
  }
  light.color = *color;
  light.intensity = *intensity;
  return light;
}

std::expected<Light, PackError> DecodeLight(Reader& reader) {
  const auto kind = reader.Read<std::uint32_t>();
  if (!kind) {
    return std::unexpected(kind.error());
  }
  switch (static_cast<LightKind>(*kind)) {
    case LightKind::kPoint:
      return DecodeLightBody<PointLight>(reader);
    case LightKind::kDirectional:
      return DecodeLightBody<DirectionalLight>(reader);
  }
  return std::unexpected(PackError::kUnsupportedLight);
}

std::expected<Spawn, PackError> DecodeSpawn(Reader& reader) {
  const auto id = reader.Read<std::uint32_t>();
  if (!id) {
    return std::unexpected(id.error());
  }
  const auto position = reader.ReadArray<std::int32_t, 3>();
  if (!position) {
    return std::unexpected(position.error());
  }
  return Spawn{.id = *id, .position_mm = *position};
}

// Allocate only after successfully decoding records, not from an unchecked
// count.
template <typename T, typename Decoder>
std::expected<std::vector<T>, PackError> DecodeCollection(Reader& reader,
                                                          std::uint32_t count,
                                                          Decoder decode) {
  std::vector<T> values;
  for (std::uint32_t i = 0; i < count; ++i) {
    auto value = decode(reader);
    if (!value) {
      return std::unexpected(value.error());
    }
    values.push_back(std::move(*value));
  }
  return values;
}

// Reading each field validates structure; geometry and gameplay rules belong
// to consumers. No partial scene escapes on failure.
std::expected<DecodedScene, PackError> DecodeScene(
    std::span<const unsigned char> bytes) {
  Reader reader(bytes);
  const auto counts = reader.ReadArray<std::uint32_t, 3>();
  if (!counts) {
    return std::unexpected(counts.error());
  }
  const auto [collision_shape_count, light_count, spawn_count] = *counts;
  auto collision_shapes = DecodeCollection<CollisionShape>(
      reader, collision_shape_count, DecodeCollisionShape);
  if (!collision_shapes) {
    return std::unexpected(collision_shapes.error());
  }
  auto lights = DecodeCollection<Light>(reader, light_count, DecodeLight);
  if (!lights) {
    return std::unexpected(lights.error());
  }
  auto spawns = DecodeCollection<Spawn>(reader, spawn_count, DecodeSpawn);
  if (!spawns) {
    return std::unexpected(spawns.error());
  }
  if (!reader.done()) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  return DecodedScene{.collision_shapes = std::move(*collision_shapes),
                      .lights = std::move(*lights),
                      .spawns = std::move(*spawns)};
}

// Checks the provenance encoding from pack v1, not the claims made by its text.
bool ValidProvenance(std::span<const unsigned char> bytes) {
  Reader reader(bytes);
  // Source and settings hashes are authenticated opaque values.
  reader.Take(2 * kDigestSize);
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

// Pack v1 contains one scene resource occupying the entire payload.
std::expected<void, PackError> ValidateResourceRecord(
    Reader& manifest, std::size_t payload_size) {
  const auto resource_id = manifest.U32();
  const auto resource_type = manifest.U32();
  const auto schema_version = manifest.U32();
  const auto reserved = manifest.U32();
  const auto payload_offset = manifest.U64();
  const auto resource_size = manifest.U64();
  switch (resource_type) {
    case static_cast<std::uint32_t>(ResourceType::kScene):
      if (schema_version != kSceneSchemaVersion) {
        return std::unexpected(PackError::kInvalidResource);
      }
      break;
    default:
      return std::unexpected(PackError::kInvalidResource);
  }
  if (resource_id != kSceneResourceId || reserved != kReservedResourceValue ||
      payload_offset != kSingleResourceOffset ||
      resource_size != payload_size) {
    return std::unexpected(PackError::kInvalidResource);
  }
  return {};
}

// Rejects collections outside the selected schema before exposing a typed
// scene.
std::expected<Scene, PackError> MakeScene(DecodedScene data, SceneKind kind) {
  switch (kind) {
    case SceneKind::kServer:
      if (!data.lights.empty()) {
        return std::unexpected(PackError::kInvalidSceneLength);
      }
      return ServerScene{.collision_shapes = std::move(data.collision_shapes),
                         .spawns = std::move(data.spawns)};
    case SceneKind::kAgent:
      if (!data.lights.empty() || !data.spawns.empty()) {
        return std::unexpected(PackError::kInvalidSceneLength);
      }
      return AgentScene{.collision_shapes = std::move(data.collision_shapes)};
    case SceneKind::kClient:
      if (!data.spawns.empty()) {
        return std::unexpected(PackError::kInvalidSceneLength);
      }
      return ClientScene{.collision_shapes = std::move(data.collision_shapes),
                         .lights = std::move(data.lights)};
  }
  return std::unexpected(PackError::kUnsupportedFormat);
}

// Requires authenticated metadata. Checks the resource record and payload
// before decoding scene values, preserving the distinction between failure
// categories.
std::expected<Scene, PackError> DecodeResource(const PackLayout& layout) {
  Reader manifest(layout.manifest);
  const auto provenance_size = manifest.U32();
  if (static_cast<std::uint64_t>(provenance_size) + kManifestFixedSize !=
          layout.manifest.size() ||
      !ValidProvenance(manifest.Take(provenance_size))) {
    return std::unexpected(PackError::kInvalidProvenance);
  }
  const auto valid_record =
      ValidateResourceRecord(manifest, layout.payload.size());
  if (!valid_record) {
    return std::unexpected(valid_record.error());
  }
  const auto digest = manifest.Take(kDigestSize);
  if (!manifest.done() || !std::ranges::equal(Hash(layout.payload), digest)) {
    return std::unexpected(PackError::kDigestMismatch);
  }
  auto decoded = DecodeScene(layout.payload);
  if (!decoded) {
    return std::unexpected(decoded.error());
  }
  return MakeScene(std::move(*decoded), layout.scene_kind);
}

}  // namespace

std::expected<VerifiedPack, PackError> VerifiedPack::LoadStorage(
    std::shared_ptr<const unsigned char> data, std::size_t size,
    std::span<const PublicKey> trusted_keys) {
  const std::span<const unsigned char> bytes(data.get(), size);
  if (bytes.size() < kHeaderSize + kSignatureSize) {
    return std::unexpected(PackError::kInvalidLength);
  }
  if (sodium_init() < 0) {
    return std::unexpected(PackError::kCryptoInitializationFailed);
  }
  const auto layout = DecodeLayout(bytes);
  if (!layout) {
    return std::unexpected(layout.error());
  }
  const auto verified = VerifySignature(*layout, trusted_keys);
  if (!verified) {
    return std::unexpected(verified.error());
  }
  auto scene = DecodeResource(*layout);
  if (!scene) {
    return std::unexpected(scene.error());
  }
  VerifiedPack result;
  result.scene_ = std::move(*scene);
  std::ranges::copy(layout->content_build_id, result.content_build_id_.begin());
  result.data_ = std::move(data);
  return result;
}

std::expected<VerifiedPack, PackError> VerifiedPack::Load(
    std::vector<unsigned char> bytes, std::span<const PublicKey> trusted_keys) {
  auto storage =
      std::make_shared<const std::vector<unsigned char>>(std::move(bytes));
  const auto size = storage->size();
  const auto* data = storage->data();
  return LoadStorage(
      std::shared_ptr<const unsigned char>(std::move(storage), data), size,
      trusted_keys);
}

std::expected<VerifiedPack, PackError> LoadFile(
    const std::filesystem::path& path,
    std::span<const PublicKey> trusted_keys) {
  auto mapped = internal::MapFile(path);
  if (!mapped) {
    return std::unexpected(mapped.error());
  }
  return VerifiedPack::LoadStorage(std::move(mapped->data), mapped->size,
                                   trusted_keys);
}

std::string_view PackErrorMessage(PackError error) {
  switch (error) {
    case PackError::kInvalidSceneLength:
      return "invalid scene length";
    case PackError::kUnsupportedCollisionShape:
      return "unsupported collision shape kind";
    case PackError::kUnsupportedLight:
      return "unsupported light kind";
    case PackError::kInvalidLength:
      return "invalid pack length";
    case PackError::kCryptoInitializationFailed:
      return "cryptographic initialization failed";
    case PackError::kUnsupportedFormat:
      return "unsupported pack format";
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
    case PackError::kCannotMapFile:
      return "pack mapping failed";
    case PackError::kMappingAllocationFailed:
      return "mapping allocation failed";
  }
  return "unknown pack error";
}

}  // namespace blackflower::content
