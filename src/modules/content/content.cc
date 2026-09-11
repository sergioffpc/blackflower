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
#include <optional>
#include <span>
#include <string>
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
constexpr std::uint32_t kCollisionComponent = 1;
constexpr std::uint32_t kVisualComponent = 2;
constexpr std::uint32_t kAudioComponent = 4;
constexpr std::uint32_t kAllComponents =
    kCollisionComponent | kVisualComponent | kAudioComponent;

struct PackErrorDescription {
  PackError error;
  std::string_view message;
};

constexpr std::array<PackErrorDescription, 20> kPackErrorDescriptions{{
    {.error = PackError::kInvalidEntity, .message = "invalid entity identity"},
    {.error = PackError::kInvalidTransform,
     .message = "invalid scene transform"},
    {.error = PackError::kInvalidSceneLength,
     .message = "invalid scene length"},
    {.error = PackError::kUnsupportedCollider,
     .message = "unsupported collider kind"},
    {.error = PackError::kUnsupportedCollisionDomain,
     .message = "unsupported collision domain"},
    {.error = PackError::kInvalidComponents,
     .message = "invalid scene components"},
    {.error = PackError::kInvalidReference,
     .message = "invalid logical reference"},
    {.error = PackError::kInvalidLength, .message = "invalid pack length"},
    {.error = PackError::kCryptoInitializationFailed,
     .message = "cryptographic initialization failed"},
    {.error = PackError::kUnsupportedFormat,
     .message = "unsupported pack format"},
    {.error = PackError::kUnexpectedRole, .message = "unexpected pack role"},
    {.error = PackError::kUnsupportedResourceCount,
     .message = "unsupported resource count"},
    {.error = PackError::kInvalidLayout, .message = "invalid pack layout"},
    {.error = PackError::kUnknownSigningKey, .message = "unknown signing key"},
    {.error = PackError::kInvalidSignature,
     .message = "invalid pack signature"},
    {.error = PackError::kInvalidProvenance,
     .message = "invalid provenance layout"},
    {.error = PackError::kInvalidResource,
     .message = "invalid resource identity, schema or range"},
    {.error = PackError::kDigestMismatch,
     .message = "resource digest mismatch"},
    {.error = PackError::kCannotMapFile, .message = "pack mapping failed"},
    {.error = PackError::kMappingAllocationFailed,
     .message = "mapping allocation failed"},
}};

enum class ResourceType : std::uint8_t {
  // Role-specific sparse scene-entity descriptions.
  kScene = 1
};

enum class ReaderError : std::uint8_t {
  // The requested value or byte range exceeds the unread input.
  kUnexpectedEnd,
};

template <typename T>
concept ReadableScalar =
    std::same_as<T, std::remove_cv_t<T>> &&
    ((std::integral<T> && !std::same_as<T, bool>) || std::floating_point<T>) &&
    (sizeof(T) == 1 || sizeof(T) == 2 || sizeof(T) == 4 || sizeof(T) == 8);

template <std::size_t Size>
using UnsignedInteger = std::conditional_t<
    Size == 1, std::uint8_t,
    std::conditional_t<
        Size == 2, std::uint16_t,
        std::conditional_t<Size == 4, std::uint32_t, std::uint64_t>>>;

// Borrows little-endian encoded storage and reports representation-level
// failures without assigning them domain meaning.
class Reader {
 public:
  explicit Reader(std::span<const unsigned char> bytes) : bytes_(bytes) {}

  std::expected<std::span<const unsigned char>, ReaderError> Take(
      std::size_t size) {
    if (size > bytes_.size()) {
      return std::unexpected(ReaderError::kUnexpectedEnd);
    }
    const auto result = bytes_.first(size);
    bytes_ = bytes_.subspan(size);
    return result;
  }

  template <ReadableScalar T>
  std::expected<T, ReaderError> Read() {
    const auto bytes = Take(sizeof(T));
    if (!bytes) {
      return std::unexpected(bytes.error());
    }
    return std::bit_cast<T>(
        DecodeLittleEndian<UnsignedInteger<sizeof(T)>>(*bytes));
  }

  template <ReadableScalar T, std::size_t N>
  std::expected<std::array<T, N>, ReaderError> ReadArray() {
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

  [[nodiscard]] std::span<const unsigned char> remaining() const {
    return bytes_;
  }

  [[nodiscard]] bool done() const { return bytes_.empty(); }

 private:
  template <typename T>
  static T DecodeLittleEndian(std::span<const unsigned char> bytes) {
    T result = 0;
    for (std::size_t i = 0; i < bytes.size(); ++i) {
      result |= static_cast<T>(bytes[i]) << (8 * i);
    }
    return result;
  }

  std::span<const unsigned char> bytes_;
};

// Views borrow the input artifact; ranges have been checked against its size.
struct PackLayout {
  PackRole role;
  std::span<const unsigned char> manifest;
  std::span<const unsigned char> payload;
  std::span<const unsigned char> key_id;
  std::span<const unsigned char> content_build_id;
  std::span<const unsigned char> signed_bytes;
  std::span<const unsigned char> signature;
};

std::expected<PackRole, PackError> DecodeMagic(
    std::span<const unsigned char> magic) {
  if (std::ranges::equal(magic, kServerMagic)) {
    return PackRole::kServer;
  }
  if (std::ranges::equal(magic, kAgentMagic)) {
    return PackRole::kAgent;
  }
  if (std::ranges::equal(magic, kClientMagic)) {
    return PackRole::kClient;
  }
  return std::unexpected(PackError::kUnsupportedFormat);
}

// Consumes the header prefix and identifies the scene encoding.
std::expected<PackRole, PackError> ValidateHeader(Reader& header) {
  const auto magic = header.Take(kServerMagic.size());
  if (!magic) {
    return std::unexpected(PackError::kUnsupportedFormat);
  }
  const auto kind = DecodeMagic(*magic);
  if (!kind) {
    return std::unexpected(kind.error());
  }
  const auto version = header.Read<std::uint32_t>();
  if (!version || *version != 1) {
    return std::unexpected(PackError::kUnsupportedFormat);
  }
  const auto resource_count = header.Read<std::uint32_t>();
  if (!resource_count || *resource_count != 1) {
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
  const auto total = header.Read<std::uint64_t>();
  const auto manifest_size = header.Read<std::uint64_t>();
  const auto payload_size = header.Read<std::uint64_t>();
  if (!total || !manifest_size || !payload_size) {
    return std::unexpected(PackError::kInvalidLayout);
  }
  // Check actual storage before subtraction; summing untrusted sizes can wrap.
  const auto body_size = bytes.size() - kHeaderSize - kSignatureSize;
  if (*total != bytes.size() || *manifest_size < kManifestFixedSize ||
      *manifest_size > body_size ||
      *payload_size != body_size - *manifest_size) {
    return std::unexpected(PackError::kInvalidLayout);
  }
  const auto key_id = header.Take(kDigestSize);
  const auto content_build_id = header.Take(kDigestSize);
  if (!key_id || !content_build_id) {
    return std::unexpected(PackError::kInvalidLayout);
  }
  const auto payload_start =
      kHeaderSize + static_cast<std::size_t>(*manifest_size);
  return PackLayout{.role = *compatible,
                    .manifest = bytes.subspan(kHeaderSize, *manifest_size),
                    .payload = bytes.subspan(payload_start, *payload_size),
                    .key_id = *key_id,
                    .content_build_id = *content_build_id,
                    .signed_bytes = bytes.first(payload_start),
                    .signature = bytes.last(kSignatureSize)};
}

// Computes SHA-256 over the exact bytes, without canonicalizing the input.
Digest Hash(std::span<const unsigned char> bytes) {
  Digest digest{};
  crypto_hash_sha256(digest.data(), bytes.data(), bytes.size());
  return digest;
}

// Domain-separated identities use verified canonical description bytes.
AssetId AssetIdentity(std::string_view domain,
                      std::span<const unsigned char> bytes) {
  crypto_hash_sha256_state state{};
  crypto_hash_sha256_init(&state);
  crypto_hash_sha256_update(
      &state, reinterpret_cast<const unsigned char*>(domain.data()),
      domain.size());
  crypto_hash_sha256_update(&state, bytes.data(), bytes.size());
  AssetId result;
  crypto_hash_sha256_final(&state, result.bytes.data());
  return result;
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

std::expected<ColliderBox, PackError> DecodeCollider(Reader& reader) {
  const auto kind = reader.Read<std::uint32_t>();
  if (!kind) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (*kind != 1) {
    return std::unexpected(PackError::kUnsupportedCollider);
  }
  const auto values = reader.ReadArray<double, 10>();
  if (!values) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  const auto& v = *values;
  ColliderBox box{.center_m = {v[0], v[1], v[2]},
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

std::expected<SceneEntityId, PackError> DecodeSceneEntityId(Reader& reader) {
  const auto size = reader.Read<std::uint32_t>();
  if (!size) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  const auto text = reader.Take(*size);
  if (!text) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (!ValidIdentity(*text)) {
    return std::unexpected(PackError::kInvalidEntity);
  }
  return SceneEntityId{.value = std::string(text->begin(), text->end())};
}

std::expected<std::string, PackError> DecodeReference(Reader& reader) {
  const auto size = reader.Read<std::uint32_t>();
  if (!size) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  const auto text = reader.Take(*size);
  if (!text) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (!ValidIdentity(*text)) {
    return std::unexpected(PackError::kInvalidReference);
  }
  return std::string(text->begin(), text->end());
}

std::expected<void, PackError> DecodeCollision(Reader& reader, PackRole role,
                                               SceneEntityDescription& entity) {
  const auto domain = reader.Read<std::uint32_t>();
  if (!domain) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (*domain != static_cast<std::uint32_t>(CollisionDomain::kSessionStatic) &&
      *domain !=
          static_cast<std::uint32_t>(CollisionDomain::kAuthoritativeDynamic)) {
    return std::unexpected(PackError::kUnsupportedCollisionDomain);
  }
  const auto collision_domain = static_cast<CollisionDomain>(*domain);
  if (role != PackRole::kServer &&
      collision_domain == CollisionDomain::kAuthoritativeDynamic) {
    return std::unexpected(PackError::kInvalidComponents);
  }
  const auto collider_bytes = reader.remaining();
  const auto count = reader.Read<std::uint32_t>();
  if (!count) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (*count == 0) {
    return std::unexpected(PackError::kInvalidComponents);
  }
  auto colliders =
      DecodeCollection<ColliderBox>(reader, *count, DecodeCollider);
  if (!colliders) {
    return std::unexpected(colliders.error());
  }
  entity.collision = CollisionDescription{
      .domain = collision_domain,
      .boxes = std::move(*colliders),
      .asset_id =
          AssetIdentity("Blackflower.Collider.v1",
                        collider_bytes.first(collider_bytes.size() -
                                             reader.remaining().size()))};
  return {};
}

std::expected<void, PackError> DecodePresentation(
    Reader& reader, std::uint32_t components, SceneEntityDescription& entity) {
  if ((components & kVisualComponent) != 0) {
    auto reference = DecodeReference(reader);
    if (!reference) {
      return std::unexpected(reference.error());
    }
    entity.visual_reference = VisualReference{.value = std::move(*reference)};
  }
  if ((components & kAudioComponent) != 0) {
    auto reference = DecodeReference(reader);
    if (!reference) {
      return std::unexpected(reference.error());
    }
    entity.audio_reference = AudioReference{.value = std::move(*reference)};
  }
  return {};
}

bool ValidComponents(std::uint32_t components, PackRole role) {
  if (components == 0 || (components & ~kAllComponents) != 0) {
    return false;
  }
  return role == PackRole::kClient || components == kCollisionComponent;
}

std::expected<void, PackError> DecodeComponents(
    Reader& reader, PackRole role, SceneEntityDescription& entity) {
  const auto components = reader.Read<std::uint32_t>();
  if (!components) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  if (!ValidComponents(*components, role)) {
    return std::unexpected(PackError::kInvalidComponents);
  }
  if ((*components & kCollisionComponent) != 0) {
    const auto collision = DecodeCollision(reader, role, entity);
    if (!collision) {
      return std::unexpected(collision.error());
    }
  }
  return DecodePresentation(reader, *components, entity);
}

std::expected<SceneEntityDescription, PackError> DecodeSceneEntityDescription(
    Reader& reader, PackRole role) {
  auto identity = DecodeSceneEntityId(reader);
  if (!identity) {
    return std::unexpected(identity.error());
  }
  const auto values = reader.ReadArray<double, 8>();
  if (!values) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  const auto& v = *values;
  SceneEntityDescription entity{.id = std::move(*identity),
                                .position_m = {v[0], v[1], v[2]},
                                .rotation_xyzw = {v[3], v[4], v[5], v[6]},
                                .scale = v[7],
                                .collision = std::nullopt,
                                .visual_reference = std::nullopt,
                                .audio_reference = std::nullopt};
  if (!ValidTransform(entity.position_m, entity.rotation_xyzw,
                      std::span(&entity.scale, 1))) {
    return std::unexpected(PackError::kInvalidTransform);
  }
  const auto components = DecodeComponents(reader, role, entity);
  if (!components) {
    return std::unexpected(components.error());
  }
  return entity;
}

// Decode complete entities before exposing scene values.
std::expected<std::vector<SceneEntityDescription>, PackError> DecodeScene(
    std::span<const unsigned char> bytes, PackRole role) {
  Reader reader(bytes);
  const auto count = reader.Read<std::uint32_t>();
  if (!count) {
    return std::unexpected(PackError::kInvalidSceneLength);
  }
  auto entities = DecodeCollection<SceneEntityDescription>(
      reader, *count, [role](Reader& source) {
        return DecodeSceneEntityDescription(source, role);
      });
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
  if (!reader.Take(2 * kDigestSize)) {
    return false;
  }
  for (int i = 0; i < 5; ++i) {
    const auto size = reader.Read<std::uint32_t>();
    if (!size || *size == 0) {
      return false;
    }
    const auto text = reader.Take(*size);
    if (!text || !std::ranges::all_of(*text, [](unsigned char c) {
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
  const auto resource_id = manifest.Read<std::uint32_t>();
  const auto resource_type = manifest.Read<std::uint32_t>();
  const auto schema_version = manifest.Read<std::uint32_t>();
  const auto reserved = manifest.Read<std::uint32_t>();
  const auto payload_offset = manifest.Read<std::uint64_t>();
  const auto resource_size = manifest.Read<std::uint64_t>();
  if (!resource_id || !resource_type || !schema_version || !reserved ||
      !payload_offset || !resource_size) {
    return std::unexpected(PackError::kInvalidResource);
  }
  switch (*resource_type) {
    case static_cast<std::uint32_t>(ResourceType::kScene):
      if (*schema_version != kSceneSchemaVersion) {
        return std::unexpected(PackError::kInvalidResource);
      }
      break;
    default:
      return std::unexpected(PackError::kInvalidResource);
  }
  if (*resource_id != kSceneResourceId || *reserved != kReservedResourceValue ||
      *payload_offset != kSingleResourceOffset ||
      *resource_size != payload_size) {
    return std::unexpected(PackError::kInvalidResource);
  }
  return {};
}

// File magic selects the concrete role scene.
std::expected<Scene, PackError> MakeScene(
    std::vector<SceneEntityDescription> entities, PackRole role) {
  switch (role) {
    case PackRole::kServer:
      return ServerScene{.entities = std::move(entities)};
    case PackRole::kAgent:
      return AgentScene{.entities = std::move(entities)};
    case PackRole::kClient:
      return ClientScene{.entities = std::move(entities)};
  }
  return std::unexpected(PackError::kUnsupportedFormat);
}

// Requires authenticated metadata. Checks the resource record and payload
// before decoding scene values, preserving the distinction between failure
// categories.
std::expected<Scene, PackError> DecodeResource(const PackLayout& layout) {
  Reader manifest(layout.manifest);
  const auto provenance_size = manifest.Read<std::uint32_t>();
  if (!provenance_size ||
      static_cast<std::uint64_t>(*provenance_size) + kManifestFixedSize !=
          layout.manifest.size()) {
    return std::unexpected(PackError::kInvalidProvenance);
  }
  const auto provenance = manifest.Take(*provenance_size);
  if (!provenance || !ValidProvenance(*provenance)) {
    return std::unexpected(PackError::kInvalidProvenance);
  }
  const auto valid_record =
      ValidateResourceRecord(manifest, layout.payload.size());
  if (!valid_record) {
    return std::unexpected(valid_record.error());
  }
  const auto digest = manifest.Take(kDigestSize);
  if (!digest || !manifest.done() ||
      !std::ranges::equal(Hash(layout.payload), *digest)) {
    return std::unexpected(PackError::kDigestMismatch);
  }
  auto decoded = DecodeScene(layout.payload, layout.role);
  if (!decoded) {
    return std::unexpected(decoded.error());
  }
  return MakeScene(std::move(*decoded), layout.role);
}

}  // namespace

std::expected<VerifiedPack, PackError> VerifiedPack::LoadStorage(
    std::shared_ptr<const unsigned char> data, std::size_t size,
    std::span<const PublicKey> trusted_keys,
    std::optional<PackRole> expected_role) {
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
  if (expected_role && *expected_role != layout->role) {
    return std::unexpected(PackError::kUnexpectedRole);
  }
  VerifiedPack result;
  result.scene_ = std::move(*scene);
  result.asset_id_ = AssetIdentity("Blackflower.Scene.v1", layout->payload);
  std::ranges::copy(layout->content_build_id, result.content_build_id_.begin());
  result.data_ = std::move(data);
  return result;
}

std::expected<VerifiedPack, PackError> VerifiedPack::Load(
    std::vector<unsigned char> bytes, std::span<const PublicKey> trusted_keys,
    std::optional<PackRole> expected_role) {
  auto storage =
      std::make_shared<const std::vector<unsigned char>>(std::move(bytes));
  const auto size = storage->size();
  const auto* data = storage->data();
  return LoadStorage(
      std::shared_ptr<const unsigned char>(std::move(storage), data), size,
      trusted_keys, expected_role);
}

std::expected<VerifiedPack, PackError> LoadFile(
    const std::filesystem::path& path, std::span<const PublicKey> trusted_keys,
    std::optional<PackRole> expected_role) {
  auto mapped = internal::MapFile(path);
  if (!mapped) {
    return std::unexpected(mapped.error());
  }
  return VerifiedPack::LoadStorage(std::move(mapped->data), mapped->size,
                                   trusted_keys, expected_role);
}

std::string_view PackErrorMessage(PackError error) {
  for (const auto& description : kPackErrorDescriptions) {
    if (description.error == error) {
      return description.message;
    }
  }
  return "unknown pack error";
}

}  // namespace blackflower::content
