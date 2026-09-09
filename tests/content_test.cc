#include "content.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <filesystem>
#include <fstream>
#include <ios>
#include <iterator>
#include <vector>

namespace blackflower::content {
namespace {
TEST(Content, OwnsVerifiedBytesAfterCallerStorageChanges) {
  const std::filesystem::path fixtures(".");
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
