#include <gtest/gtest.h>

namespace blackflower {
namespace {

TEST(BuildTest, UsesCpp23) { EXPECT_GE(__cplusplus, 202302L); }

}  // namespace
}  // namespace blackflower
