#pragma once

// Blast 5.0.6's stress solver uses names from std without including the C++
// math wrapper that introduces them there. Force this header for MSVC while
// leaving the pinned vendor sources unchanged.
#include <cmath>
