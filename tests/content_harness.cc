#include <array>
#include <concepts>
#include <cstddef>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <ios>
#include <iostream>
#include <limits>
#include <string>
#include <string_view>
#include <variant>
#include <vector>

#include "content.h"
#include "scene_exercise.h"

namespace {
std::string Hex(const blackflower::content::Digest& digest) {
  constexpr std::string_view kHex = "0123456789abcdef";
  std::string text;
  for (const auto byte : digest) {
    text += kHex[byte >> 4];
    text += kHex[byte & 15];
  }
  return text;
}

template <typename T, std::size_t N>
void PrintArray(const std::array<T, N>& values) {
  std::cout << '[';
  for (std::size_t i = 0; i < N; ++i) {
    std::cout << (i == 0 ? "" : ",") << values[i];
  }
  std::cout << ']';
}

template <typename T, typename Printer>
void PrintCollection(const std::vector<T>& values, Printer print) {
  std::cout << '[';
  bool first = true;
  for (const auto& value : values) {
    std::cout << (first ? "" : ",");
    print(value);
    first = false;
  }
  std::cout << ']';
}

void PrintCollider(const blackflower::content::ColliderBox& value) {
  std::cout << "{\"center_m\":";
  PrintArray(value.center_m);
  std::cout << ",\"dimensions_m\":";
  PrintArray(value.dimensions_m);
  std::cout << ",\"rotation_xyzw\":";
  PrintArray(value.rotation_xyzw);
  std::cout << '}';
}

void PrintSceneEntityDescription(
    const blackflower::content::SceneEntityDescription& value) {
  // Validated IDs contain no characters requiring JSON escaping.
  std::cout << "{\"id\":\"" << value.id.value << "\",\"position_m\":";
  PrintArray(value.position_m);
  std::cout << ",\"rotation_xyzw\":";
  PrintArray(value.rotation_xyzw);
  std::cout << ",\"scale\":" << value.scale << ",\"colliders\":";
  PrintCollection(value.colliders, PrintCollider);
  std::cout << ",\"collider_asset_id\":\"" << Hex(value.collider_asset_id.bytes)
            << "\"}";
}

template <typename T>
void PrintScene(const T& value) {
  constexpr std::string_view kName =
      std::same_as<T, blackflower::content::ServerScene>  ? "server"
      : std::same_as<T, blackflower::content::AgentScene> ? "agent"
                                                          : "client";
  std::cout << ",\"scene_type\":\"" << kName << "\",\"entities\":";
  PrintCollection(value.entities, PrintSceneEntityDescription);
}

// Emits prepared values consumed by the integration driver.
void PrintContent(const blackflower::content::VerifiedPack& pack) {
  std::cout << "{\"content_build_id\":\"" << Hex(pack.content_build_id())
            << '\"';
  std::cout << ",\"scene_asset_id\":\"" << Hex(pack.asset_id().bytes) << '\"';
  std::visit([](const auto& value) { PrintScene(value); }, pack.scene());
  std::cout << "}\n";
}
}  // namespace

int main(int argc, char** argv) {
  std::cout << std::setprecision(std::numeric_limits<double>::max_digits10);
  if (argc != 3 && argc != 4) {
    std::cerr << "usage: content_harness PACK PUBLIC_KEY\n";
    return 2;
  }
  std::array<blackflower::content::PublicKey, 1> keys{};
  std::ifstream public_key(std::filesystem::path(argv[2]), std::ios::binary);
  public_key.read(reinterpret_cast<char*>(keys[0].data()), keys[0].size());
  if (!public_key || public_key.peek() != std::ifstream::traits_type::eof()) {
    std::cerr << "expected an independently provisioned 32-byte public key\n";
    return 1;
  }
  const auto pack =
      blackflower::content::LoadFile(std::filesystem::path(argv[1]), keys);
  if (!pack) {
    std::cerr << "content rejected: "
              << blackflower::content::PackErrorMessage(pack.error()) << '\n';
    return 1;
  }
  if (argc == 4) {
    return ExerciseScene(*pack) ? 0 : 1;
  }
  PrintContent(*pack);
  return std::cout ? 0 : 1;
}
