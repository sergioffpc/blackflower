#include "content.h"

#include <gtest/gtest.h>

#ifdef _WIN32
#include <stdlib.h>  // _dupenv_s is a Microsoft CRT extension.
#endif

#include <algorithm>
#include <array>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <ios>
#include <iterator>
#include <vector>

#ifdef _WIN32
#include <cstddef>
#include <memory>
#endif

namespace blackflower::content {
namespace {
TEST(Content, OwnsVerifiedBytesAfterCallerStorageChanges) {
#ifdef _WIN32
  char* allocated = nullptr;
  std::size_t length = 0;
  ASSERT_EQ(_dupenv_s(&allocated, &length, "BLACKFLOWER_CONTENT_FIXTURES"), 0);
  const std::unique_ptr<char, decltype(&std::free)> owned(allocated,
                                                          &std::free);
  const char* directory = owned.get();
#else
  // Tests read this process environment before starting any worker threads.
  // NOLINTNEXTLINE(concurrency-mt-unsafe)
  const char* directory = std::getenv("BLACKFLOWER_CONTENT_FIXTURES");
#endif
  ASSERT_NE(directory, nullptr);
  const std::filesystem::path fixtures(directory);
  std::ifstream file(fixtures / "reference.bfclient", std::ios::binary);
  ASSERT_TRUE(file);
  std::vector<unsigned char> input((std::istreambuf_iterator<char>(file)), {});
  ASSERT_FALSE(input.empty());
  const auto original = input;
  std::array<PublicKey, 1> keys{};
  std::ifstream public_key(fixtures / "reference.pub", std::ios::binary);
  public_key.read(reinterpret_cast<char*>(keys[0].data()), keys[0].size());
  ASSERT_TRUE(public_key);
  const auto loaded = VerifiedPack::Load(input, Role::kClient,
                                         Profile::kWindowsPrimitives, keys);
  ASSERT_TRUE(loaded) << PackErrorMessage(loaded.error());
  std::ranges::fill(input, 0);
  keys[0].fill(0);
  EXPECT_TRUE(std::ranges::equal(loaded->bytes(), original));
  EXPECT_EQ(loaded->scene().capsule_mm[0], 1800U);
  EXPECT_EQ(loaded->scene().spawns[0].foot_mm[0], -8000);
}
}  // namespace
}  // namespace blackflower::content
