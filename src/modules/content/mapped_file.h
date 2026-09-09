#ifndef BLACKFLOWER_CONTENT_MAPPED_FILE_H_
#define BLACKFLOWER_CONTENT_MAPPED_FILE_H_

#include <cstddef>
#include <cstdint>
#include <expected>
#include <filesystem>
#include <memory>

namespace blackflower::content {
enum class PackError : std::uint8_t;

namespace internal {
// Shared ownership keeps the read-only mapping alive across pack copies.
struct MappedFile {
  std::shared_ptr<const unsigned char> data;
  std::size_t size = 0;
};

// Maps one file without copying its contents. The backing file must remain
// unchanged until the last owner releases the mapping. Mapping is not a
// snapshot.
std::expected<MappedFile, PackError> MapFile(
    const std::filesystem::path& path) noexcept;
}  // namespace internal
}  // namespace blackflower::content
#endif  // BLACKFLOWER_CONTENT_MAPPED_FILE_H_
