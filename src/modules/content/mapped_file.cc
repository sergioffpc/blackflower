#include "mapped_file.h"

#include <boost/filesystem/path.hpp>
#include <boost/iostreams/device/mapped_file.hpp>
#include <expected>
#include <filesystem>
#include <memory>
#include <new>
#include <utility>

#include "content.h"

namespace blackflower::content::internal {
std::expected<MappedFile, PackError> MapFile(
    const std::filesystem::path& path) noexcept {
  // This adapter contains the dependency's exceptions; none cross the boundary.
  try {
    auto mapping = std::make_shared<boost::iostreams::mapped_file_source>();
    // Boost's path preserves native wide Windows paths at this boundary.
    mapping->open(boost::filesystem::path(path.native()));
    const auto size = mapping->size();
    const auto* data = reinterpret_cast<const unsigned char*>(mapping->data());
    return MappedFile{
        .data = std::shared_ptr<const unsigned char>(std::move(mapping), data),
        .size = size};
  } catch (const std::bad_alloc&) {
    return std::unexpected(PackError::kMappingAllocationFailed);
  } catch (...) {
    return std::unexpected(PackError::kCannotMapFile);
  }
}
}  // namespace blackflower::content::internal
