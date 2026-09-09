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
#include <vector>

namespace blackflower::content {
namespace {
TEST(Content, OwnsVerifiedBytesAfterCallerStorageChanges) {
  const std::filesystem::path fixtures(".");
  std::ifstream file(fixtures / "reference.bfsimulation", std::ios::binary);
  ASSERT_TRUE(file);
  std::vector<unsigned char> input((std::istreambuf_iterator<char>(file)), {});
  ASSERT_FALSE(input.empty());
  const auto original = input;
  std::array<PublicKey, 1> keys{};
  std::ifstream public_key(fixtures / "reference.pub", std::ios::binary);
  public_key.read(reinterpret_cast<char*>(keys[0].data()), keys[0].size());
  ASSERT_TRUE(public_key);
  const auto loaded = VerifiedPack::Load(input, Role::kSimulation, keys);
  ASSERT_TRUE(loaded) << PackErrorMessage(loaded.error());
  std::ranges::fill(input, 0);
  keys[0].fill(0);
  EXPECT_TRUE(std::ranges::equal(loaded->bytes(), original));
  EXPECT_EQ(std::get<Box>(loaded->scene().geometries[0]).size_mm[0], 2000U);
  EXPECT_EQ(loaded->scene().spawns[0].position_mm[0], -8000);
}

TEST(Content, KeepsMappingAliveAcrossPackCopiesAndMoves) {
  std::array<PublicKey, 1> keys{};
  std::ifstream key_file("reference.pub", std::ios::binary);
  key_file.read(reinterpret_cast<char*>(keys[0].data()), keys[0].size());
  ASSERT_TRUE(key_file);
  std::optional<VerifiedPack> survivor;
  std::vector<unsigned char> expected;
  {
    const auto loaded =
        LoadFile("reference.bfsimulation", Role::kSimulation, keys);
    ASSERT_TRUE(loaded) << PackErrorMessage(loaded.error());
    expected.assign(loaded->bytes().begin(), loaded->bytes().end());
    survivor.emplace(*loaded);
  }
  const auto moved = std::move(*survivor);
  survivor.reset();
  EXPECT_TRUE(std::ranges::equal(moved.bytes(), expected));
  EXPECT_EQ(moved.scene().spawns[0].position_mm[0], -8000);
}
}  // namespace
}  // namespace blackflower::content
