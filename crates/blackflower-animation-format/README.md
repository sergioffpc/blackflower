# Blackflower animation format

`blackflower-animation-format` owns the deterministic, native-free containers
used for cooked skeletal animation assets:

- `.bfskel` contains one private ozz runtime skeleton archive.
- `.bfanim` contains one private ozz runtime animation archive, typed clip
  metadata, and an optional root-motion track archive.

The crate validates the complete container before exposing borrowed section
bytes. It does not parse ozz archives; that remains the responsibility of the
pinned native runtime.

Both formats begin with a fixed 64-byte little-endian header containing typed
magic, container schema, header size, required Ozz version, zero reserved
flags, section count, exact file size, and the 32-byte `SkeletonIdentity`.
Typed 24-byte section descriptors follow. Sections are eight-byte aligned,
strictly ordered, non-overlapping, non-empty, and exhaustive: undeclared
padding or trailing bytes are rejected.

`SkeletonIdentity` hashes the complete ordered rig: joint count and, for each
Ozz joint, its name, parent, and rest translation, rotation, and scale.
Negative zero and quaternion sign are canonicalized before hashing.
