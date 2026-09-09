# Map content files

Status: accepted.

Packs can reach gigabytes. File ingestion uses read-only memory mappings to
avoid an additional full-file heap allocation. Shared RAII ownership keeps
mapped bytes alive across pack copies and moves. The existing in-memory loading
interface retains owned bytes for callers that already have them. Scene values
remain separately decoded; authentication still hashes the complete payload. The
binary format is unchanged.

C++23 provides no standard file-mapping API. Use Boost.Iostreams
`mapped_file_source` for its Linux and Windows implementation. An isolated
adapter is compiled with exceptions enabled, catches dependency exceptions and
returns typed `PackError` values. The content parser and callers remain compiled
without exceptions. Mapping allocation failures have their own category; other
open/map failures share the mapping failure category.

The vcpkg baseline pins Boost.Iostreams and Boost.Filesystem 1.91.0 and their
required Boost dependencies. Boost.Filesystem supplies the native-path
interoperability required by the mapping API, preserving wide Windows paths.
Compression features are disabled because mapping does not need compression
libraries. Its
[implementation](https://github.com/boostorg/iostreams/blob/boost-1.91.0/src/mapped_file.cpp)
uses POSIX `mmap` and Windows file mappings and retains the Windows file handle.

Backing files must remain unchanged until the last mapping owner releases them.
Windows retains a file handle that denies writes and deletion. Linux requires
the publisher and deployment process to prevent in-place writes and truncation;
a read-only mapping is not an immutable snapshot. Signature verification cannot
guarantee later bytes if this precondition is violated. Mapped-page I/O faults
and allocations outside the adapter are outside its recovery contract.

This supersedes the full-file ingestion strategy in
[ADR-0009](0009-typed-errors-and-content-size-policy.md), retaining its typed
errors and absence of policy size caps. Streaming and prefetch policies require
measurements of representative workloads.
