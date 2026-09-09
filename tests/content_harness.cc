#include <array>
#include <filesystem>
#include <fstream>
#include <ios>
#include <iostream>
#include <string>
#include <string_view>

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
}  // namespace

int main(int argc, char** argv) {
  if (argc != 5) {
    std::cerr << "usage: content_harness PACK client|server windows|linux "
                 "PUBLIC_KEY\n";
    return 2;
  }
  using blackflower::content::Profile;
  using blackflower::content::Role;
  const std::string_view role_name(argv[2]);
  const std::string_view profile_name(argv[3]);
  if ((role_name != "client" && role_name != "server") ||
      (profile_name != "windows" && profile_name != "linux")) {
    return 2;
  }
  std::array<blackflower::content::PublicKey, 1> keys{};
  std::ifstream public_key(std::filesystem::path(argv[4]), std::ios::binary);
  public_key.read(reinterpret_cast<char*>(keys[0].data()), keys[0].size());
  if (!public_key || public_key.peek() != std::ifstream::traits_type::eof()) {
    std::cerr << "expected an independently provisioned 32-byte public key\n";
    return 1;
  }
  const auto pack = blackflower::content::LoadFile(
      std::filesystem::path(argv[1]),
      role_name == "client" ? Role::kClient : Role::kServer,
      profile_name == "windows" ? Profile::kWindowsPrimitives
                                : Profile::kLinuxPrimitives,
      keys);
  if (!pack) {
    std::cerr << "content rejected: "
              << blackflower::content::PackErrorMessage(pack.error()) << '\n';
    return 1;
  }
  const auto& scene = pack->scene();
  std::cout << "{\"pack_id\":\"" << Hex(pack->pack_id())
            << "\",\"scenario_build_id\":\"" << Hex(pack->scenario_build_id())
            << "\",\"interior_mm\":[" << scene.interior_mm[0] << ','
            << scene.interior_mm[1] << "],\"capsule_mm\":["
            << scene.capsule_mm[0] << ',' << scene.capsule_mm[1]
            << "],\"box_count\":" << scene.boxes.size() << ",\"spawns_mm\":[";
  bool first = true;
  for (const auto& spawn : scene.spawns) {
    std::cout << (first ? "" : ",") << '[' << spawn.foot_mm[0] << ','
              << spawn.foot_mm[1] << ',' << spawn.foot_mm[2] << ']';
    first = false;
  }
  std::cout << "]}\n";
  return std::cout ? 0 : 1;
}
