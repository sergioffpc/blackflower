# Typed errors and content size policy

Status: accepted.

Error contracts use enums or classes, with diagnostic text separate from error
identity. Standard-library facilities provide typed result transport without an
additional dependency. See the
[error policy](../cpp-guidelines.md#typed-errors).

Content has no policy size caps because no justified size budget exists. Encoded
widths, actual byte ranges and platform representability remain constraints.
This supersedes the size-cap policy in
[ADR-0007](0007-minimal-pack-format-and-trust.md).

The binary layout and existing artifact identities remain compatible. Readers
with size caps may reject larger artifacts. Full-file loading consumes memory
proportional to input size; allocation failure is not recoverable in the current
no-exceptions build.
