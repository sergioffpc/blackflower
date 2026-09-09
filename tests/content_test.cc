#include "content.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <filesystem>
#include <fstream>
#include <ios>
#include <iterator>
#include <optional>
#include <utility>
#include <variant>
#include <vector>

namespace blackflower::content {
namespace {
TEST(Content, KeepsSceneAfterCallerStorageChanges) {
  const std::filesystem::path fixtures(".");
  std::ifstream file(fixtures / "reference.bfserver", std::ios::binary);
  ASSERT_TRUE(file);
  std::vector<unsigned char> input((std::istreambuf_iterator<char>(file)), {});
  ASSERT_FALSE(input.empty());
  std::array<PublicKey, 1> keys{};
  std::ifstream public_key(fixtures / "reference.pub", std::ios::binary);
  public_key.read(reinterpret_cast<char*>(keys[0].data()), keys[0].size());
  ASSERT_TRUE(public_key);
  const auto loaded = VerifiedPack::Load(input, keys);
  ASSERT_TRUE(loaded) << PackErrorMessage(loaded.error());
  std::ranges::fill(input, 0);
  keys[0].fill(0);
  const auto* scene = std::get_if<ServerScene>(&loaded->scene());
  ASSERT_NE(scene, nullptr);
  EXPECT_EQ(std::get<Box>(scene->collision_shapes[0]).size_mm[0], 2000U);
  EXPECT_EQ(scene->spawns[0].position_mm[0], -8000);
}

TEST(Content, KeepsSceneAcrossMappedPackCopiesAndMoves) {
  std::array<PublicKey, 1> keys{};
  std::ifstream key_file("reference.pub", std::ios::binary);
  key_file.read(reinterpret_cast<char*>(keys[0].data()), keys[0].size());
  ASSERT_TRUE(key_file);
  std::optional<VerifiedPack> survivor;
  {
    const auto loaded = LoadFile("reference.bfserver", keys);
    ASSERT_TRUE(loaded) << PackErrorMessage(loaded.error());
    survivor.emplace(*loaded);
  }
  const auto moved = std::move(*survivor);
  survivor.reset();
  const auto* scene = std::get_if<ServerScene>(&moved.scene());
  ASSERT_NE(scene, nullptr);
  EXPECT_EQ(scene->spawns[0].position_mm[0], -8000);
}
}  // namespace
}  // namespace blackflower::content
