#include "content.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <ios>
#include <iterator>
#include <optional>
#include <string>
#include <system_error>
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
  EXPECT_EQ(scene->entities[0].id, (SceneEntityId{.value = "reference"}));
  const SceneEntityDescription& entity = scene->entities[0];
  EXPECT_EQ(entity.position_m[0], 7.0);
  const auto* collision = entity.collision ? &*entity.collision : nullptr;
  ASSERT_NE(collision, nullptr);
  EXPECT_EQ(collision->boxes[0].dimensions_m[0], 2.0);
  EXPECT_EQ(collision->boxes[0].center_m[0], -3.0);
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
  const auto& entity = scene->entities[0];
  const auto* collision = entity.collision ? &*entity.collision : nullptr;
  ASSERT_NE(collision, nullptr);
  EXPECT_EQ(collision->boxes[0].center_m[0], -3.0);
}

class ContentFileReplacement : public testing::Test {
 protected:
  void SetUp() override {
    std::error_code error;
    const auto root = std::filesystem::temp_directory_path(error);
    ASSERT_FALSE(error) << error.message();
    directory_ =
        root /
        ("blackflower-replacement-" +
         std::to_string(
             std::chrono::steady_clock::now().time_since_epoch().count()));
    owns_directory_ = std::filesystem::create_directory(directory_, error);
    ASSERT_TRUE(owns_directory_) << error.message();
    path_ = directory_ / "scene.pack";
    replacement_ = directory_ / "replacement.pack";
    ASSERT_TRUE(std::filesystem::copy_file("reference.bfserver", path_, error));
    ASSERT_FALSE(error) << error.message();
    // A short, invalid artifact makes a reopened pathname distinguishable.
    ASSERT_TRUE(
        std::filesystem::copy_file("reference.pub", replacement_, error));
    ASSERT_FALSE(error) << error.message();
    std::ifstream key_file("reference.pub", std::ios::binary);
    key_file.read(reinterpret_cast<char*>(keys_[0].data()),
                  static_cast<std::streamsize>(keys_[0].size()));
    ASSERT_TRUE(key_file);
  }

  void TearDown() override {
    if (owns_directory_) {
      std::error_code error;
      std::filesystem::remove_all(directory_, error);
      EXPECT_FALSE(error) << error.message();
    }
  }

  std::filesystem::path directory_;
  std::filesystem::path path_;
  std::filesystem::path replacement_;
  std::array<PublicKey, 1> keys_{};
  bool owns_directory_ = false;
};

TEST_F(ContentFileReplacement, RetainsVerifiedContentAcrossPathReplacement) {
  std::optional<VerifiedPack> survivor;
  {
    const auto loaded = LoadFile(path_, keys_);
    ASSERT_TRUE(loaded) << PackErrorMessage(loaded.error());
    survivor.emplace(*loaded);
  }
  std::error_code error;
  std::filesystem::rename(replacement_, path_, error);
#ifdef _WIN32
  // Windows denies deletion while the surviving pack holds the mapping.
  ASSERT_TRUE(error);
#else
  ASSERT_FALSE(error) << error.message();
  const auto replacement = LoadFile(path_, keys_);
  ASSERT_FALSE(replacement);
  EXPECT_EQ(replacement.error(), PackError::kInvalidLength);
#endif
  const auto* scene = std::get_if<ServerScene>(&survivor->scene());
  ASSERT_NE(scene, nullptr);
  const auto& entity = scene->entities[0];
  const auto* collision = entity.collision ? &*entity.collision : nullptr;
  ASSERT_NE(collision, nullptr);
  EXPECT_EQ(collision->boxes[0].center_m[0], -3.0);
  survivor.reset();
#ifdef _WIN32
  std::filesystem::rename(replacement_, path_, error);
  ASSERT_FALSE(error) << error.message();
  const auto replacement = LoadFile(path_, keys_);
  ASSERT_FALSE(replacement);
  EXPECT_EQ(replacement.error(), PackError::kInvalidLength);
#endif
}
}  // namespace
}  // namespace blackflower::content
