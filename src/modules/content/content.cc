#include "content.h"

#include <sodium/core.h>
#include <sodium/crypto_hash_sha256.h>
#include <sodium/crypto_sign.h>

#include <algorithm>
#include <array>
#include <bit>
#include <cmath>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <filesystem>
#include <iterator>
#include <memory>
#include <span>
#include <string>
#include <string_view>
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
  // Entities with independently authored bounds.
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
             std::same_as<T, float> || std::same_as<T, double>)
  std::expected<T, PackError> Read() {
    if (!valid_ || bytes_.size() < sizeof(T)) {
      valid_ = false;
      return std::unexpected(PackError::kInvalidSceneLength);
    }
    if constexpr (std::same_as<T, double>) {
      return std::bit_cast<T>(U64());
    } else {
      return std::bit_cast<T>(U32());
    }
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

bool ValidIdentity(std::span<const unsigned char> bytes) {
  const auto alnum = [](unsigned char c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
           (c >= '0' && c <= '9');
  };
  return !bytes.empty() && alnum(bytes.front()) &&
         std::ranges::all_of(bytes, [&](unsigned char c) {
           return alnum(c) || c == '_' || c == '.' || c == ':' || c == '-';
         });
}

bool ValidTransform(std::span<const double> position,
                    std::span<const double> rotation,
                    std::span<const double> dimensions) {
  const auto finite = [](double v) { return std::isfinite(v); };
  double norm = 0;
  for (const auto value : rotation) {
    norm += value * value;
  }
  return std::ranges::all_of(position, finite) &&
         std::ranges::all_of(rotation, finite) &&
         std::ranges::all_of(
             dimensions, [](double v) { return std::isfinite(v) && v > 0; }) &&
         std::abs(norm - 1) <= 1e-12;
}

std::expected<Bound, PackError> DecodeBound(Reader& reader) {
  const auto kind = reader.Read<std::uint32_t>();
  if (!kind) {
    return std::unexpected(kind.error());
  }
  if (*kind != 1) {
    return std::unexpected(PackError::kUnsupportedBound);
  }
  const auto values = reader.ReadArray<double, 10>();
  if (!values) {
    return std::unexpected(values.error());
  }
  const auto& v = *values;
  Bound box{.center_m = {v[0], v[1], v[2]},
            .dimensions_m = {v[3], v[4], v[5]},
            .rotation_xyzw = {v[6], v[7], v[8], v[9]}};
  if (!ValidTransform(box.center_m, box.rotation_xyzw, box.dimensions_m)) {
    return std::unexpected(PackError::kInvalidTransform);
  }
  return box;
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

std::expected<Entity, PackError> DecodeEntity(Reader& reader) {
  const auto size = reader.Read<std::uint32_t>();
  if (!size) {
    return std::unexpected(size.error());
  }
  const auto text = reader.Take(*size);
  if (text.size() != *size) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (!ValidIdentity(text)) {
    return std::unexpected(PackError::kInvalidEntity);
  }
  const auto values = reader.ReadArray<double, 8>();
  if (!values) {
    return std::unexpected(values.error());
  }
  const auto& v = *values;
  Entity entity{.id = std::string(text.begin(), text.end()),
                .position_m = {v[0], v[1], v[2]},
                .rotation_xyzw = {v[3], v[4], v[5], v[6]},
                .scale = v[7],
                .bounds = {}};
  if (!ValidTransform(entity.position_m, entity.rotation_xyzw,
                      std::span(&entity.scale, 1))) {
    return std::unexpected(PackError::kInvalidTransform);
  }
  const auto count = reader.Read<std::uint32_t>();
  if (!count) {
    return std::unexpected(count.error());
  }
  auto bounds = DecodeCollection<Bound>(reader, *count, DecodeBound);
  if (!bounds) {
    return std::unexpected(bounds.error());
  }
  entity.bounds = std::move(*bounds);
  return entity;
}

// Decode complete entities before exposing scene values.
std::expected<std::vector<Entity>, PackError> DecodeScene(
    std::span<const unsigned char> bytes) {
  Reader reader(bytes);
  const auto count = reader.Read<std::uint32_t>();
  if (!count) {
    return std::unexpected(count.error());
  }
  auto entities = DecodeCollection<Entity>(reader, *count, DecodeEntity);
  if (!entities) {
    return std::unexpected(entities.error());
  }
  if (!reader.done()) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (std::ranges::adjacent_find(*entities, [](const auto& a, const auto& b) {
        return a.id >= b.id;
      }) != entities->end()) {
    return std::unexpected(PackError::kInvalidEntity);
  }
  return entities;
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

// File magic selects the concrete role scene.
std::expected<Scene, PackError> MakeScene(std::vector<Entity> entities,
                                          SceneKind kind) {
  switch (kind) {
    case SceneKind::kServer:
      return ServerScene{.entities = std::move(entities)};
    case SceneKind::kAgent:
      return AgentScene{.entities = std::move(entities)};
    case SceneKind::kClient:
      return ClientScene{.entities = std::move(entities)};
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
    case PackError::kInvalidEntity:
      return "invalid entity identity";
    case PackError::kInvalidTransform:
      return "invalid scene transform";
    case PackError::kInvalidSceneLength:
      return "invalid scene length";
    case PackError::kUnsupportedBound:
      return "unsupported bound kind";
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
