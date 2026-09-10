#ifndef BLACKFLOWER_TESTS_SCENE_EXERCISE_H_
#define BLACKFLOWER_TESTS_SCENE_EXERCISE_H_

#include "content.h"

// Exercises public runtime operations using the actual authenticated fixture.
// Emits geometry and lifecycle results for the installed-CLI integration test.
bool ExerciseScene(const blackflower::content::VerifiedPack& pack);

#endif  // BLACKFLOWER_TESTS_SCENE_EXERCISE_H_
