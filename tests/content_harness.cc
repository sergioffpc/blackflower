#include <array>
#include <concepts>
#include <cstddef>
#include <filesystem>
#include <fstream>
#include <ios>
#include <iostream>
#include <string>
#include <string_view>
#include <utility>
#include <variant>
#include <vector>

#include "content.h"

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

void PrintCollisionShape(
    const blackflower::content::CollisionShape& collision_shape) {
  std::visit(
      [](const auto& value) {
        std::cout << "{\"id\":" << value.id << ",\"center_mm\":";
        PrintArray(value.center_mm);
        if constexpr (requires { value.size_mm; }) {
          std::cout << ",\"kind\":"
                    << std::to_underlying(
                           blackflower::content::CollisionShapeKind::kBox)
                    << ",\"dimensions_mm\":";
          PrintArray(value.size_mm);
        } else {
          std::cout << ",\"kind\":"
                    << std::to_underlying(
                           blackflower::content::CollisionShapeKind::kSphere)
                    << ",\"dimensions_mm\":[" << value.radius_mm << ']';
        }
        std::cout << '}';
      },
      collision_shape);
}

void PrintLight(const blackflower::content::Light& light) {
  std::visit(
      [](const auto& value) {
        std::cout << "{\"id\":" << value.id;
        if constexpr (requires { value.position_mm; }) {
          std::cout << ",\"kind\":"
                    << std::to_underlying(
                           blackflower::content::LightKind::kPoint)
                    << ",\"position_mm\":";
          PrintArray(value.position_mm);
        } else {
          std::cout << ",\"kind\":"
                    << std::to_underlying(
                           blackflower::content::LightKind::kDirectional)
                    << ",\"direction\":";
          PrintArray(value.direction);
        }
        std::cout << ",\"color\":";
        PrintArray(value.color);
        std::cout << ",\"intensity\":" << value.intensity << '}';
      },
      light);
}

void PrintSpawn(const blackflower::content::Spawn& spawn) {
  std::cout << "{\"id\":" << spawn.id << ",\"position_mm\":";
  PrintArray(spawn.position_mm);
  std::cout << '}';
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

template <typename T>
void PrintScene(const T& value) {
  constexpr std::string_view kName =
      std::same_as<T, blackflower::content::ServerScene>  ? "server"
      : std::same_as<T, blackflower::content::AgentScene> ? "agent"
                                                          : "client";
  std::cout << ",\"scene_type\":\"" << kName << "\",\"collision_shapes\":";
  PrintCollection(value.collision_shapes, PrintCollisionShape);
  std::cout << ",\"lights\":";
  if constexpr (requires { value.lights; }) {
    PrintCollection(value.lights, PrintLight);
  } else {
    std::cout << "[]";
  }
  std::cout << ",\"spawns\":";
  if constexpr (requires { value.spawns; }) {
    PrintCollection(value.spawns, PrintSpawn);
  } else {
    std::cout << "[]";
  }
}

// Emits prepared values consumed by the integration driver.
void PrintContent(const blackflower::content::VerifiedPack& pack) {
  std::cout << "{\"pack_id\":\"" << Hex(pack.pack_id())
            << "\",\"scenario_build_id\":\"" << Hex(pack.scenario_build_id())
            << '\"';
  std::visit([](const auto& value) { PrintScene(value); }, pack.scene());
  std::cout << "}\n";
}
}  // namespace

int main(int argc, char** argv) {
  if (argc != 3) {
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
  PrintContent(*pack);
  return std::cout ? 0 : 1;
}
