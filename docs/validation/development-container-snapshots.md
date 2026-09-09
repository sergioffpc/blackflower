# Development container snapshot downloads

## Failure and correction

On 2026-09-09, the native build checks for PRs
[#40](https://github.com/sergioffpc/blackflower/pull/40) and
[#41](https://github.com/sergioffpc/blackflower/pull/41) failed during image
construction. `libpython3.14-minimal_3.14.4-1ubuntu0.1_amd64.deb` returned HTTP
404 from archive.ubuntu.com. The US mirror fallback in #40 returned the same
error. The failures occurred before project compilation or tests. The successful
pull-request run for #41 reused its cached toolchain image and did not exercise
a new image build.

The actual `apt-helper download-file` invocation reproduced exit 100 locally
against archive.ubuntu.com, us.archive.ubuntu.com and security.ubuntu.com.
Downloading the identical package from
`https://snapshot.ubuntu.com/ubuntu/20260908T000000Z/` passed its existing
SHA-256 verification. This supplies retained package bytes instead of relying on
agreement between current archive replicas.

All 182 locked URLs now identify that snapshot. Package paths, versions, hashes
and the base image digest are unchanged. The installer still verifies each
download before local installation and never resolves dependencies against live
indexes. Snapshot URLs replace the earlier US mirror fallback. The existing
SHA-256 checks authenticate downloaded bytes against the reviewed lock.

The maintenance resolver requires a snapshot timestamp and uses that snapshot
for both index update and dependency resolution. APT still verifies the Ubuntu
index signatures. Review and validate a new lock before using it.

## HTTPS bootstrap trust

Snapshot HTTP URLs redirect to HTTPS. The base image does not yet have a CA
bundle, so both scripts explicitly use the committed
[ISRG Root X1 certificate](../../.devcontainer/snapshot-ca.pem) through APT's
`Acquire::https::CaInfo` option. It was fetched from the
[official certificate URL](https://letsencrypt.org/certs/isrgrootx1.pem) and
compared byte-for-byte with the installed Ubuntu CA package's copy. Its SHA-256
certificate fingerprint is:

```text
96:BC:EC:06:26:49:76:F3:74:60:77:9A:CF:28:C5:A7:CF:E8:A3:C0:AA:E1:1A:8F:FC:EE:05:C0:BD:DF:08:C6
```

The snapshot service's observed chain reaches this root. See
[Let's Encrypt's chain documentation](https://letsencrypt.org/certificates/).
The certificate is public trust material, not a private key. Hostname, TLS
chain, APT index signature and package hash checks remain enabled. Review the
trust anchor if the service changes its chain; failure must not enable an
unverified download path. The Dockerfile copies the certificate before
downloading packages. The snapshot repair also included it in the then-current
CI image cache key.

## Verification boundary

Compare each old and new lock row's digest and package-relative path; only the
archive prefix changes. Download every locked file into a fresh temporary
directory with `/usr/lib/apt/apt-helper download-file URL DEST SHA256:DIGEST`. A
failed download or digest mismatch must fail validation. All 182 files passed
hash verification in fresh temporary storage. No package-version substitution or
stale download cache was used.

The resolver can be exercised inside an isolated filesystem namespace with
temporary APT state and signed Ubuntu sources. Empty installed-package state
checks snapshot URL generation but produces a larger closure than the pinned
base image; it must not replace the committed lock.

A reduced check ran the actual installer with the failing Python package in a
filesystem namespace. The original live-mirror script exited 100 with HTTP 404.
The snapshot installer reached installation; a changed expected hash exited 100
without reaching installation. With system CA storage hidden, the committed root
still verified HTTPS, while an empty root file caused rejection. Only the final
`apt-get install` operation was stubbed; APT's extra user switch was disabled
inside the isolated user namespace. This validates download and verification
flow, not package installation.

At the snapshot repair revision, the GitHub workflow included the complete lock
and installer in its toolchain image cache key. The changes therefore required
building an image with the new download sources before cached reuse was
possible. The Debug, TSan and Release jobs then prepared dependencies and ran
fresh project builds offline. The subsequent
[GHCR workflow](ghcr-development-image.md) separates image publication from
ordinary validation. Hosted results are recorded on the two pull requests.
Docker is unavailable in the editing container, so local package downloads do
not establish full image construction or offline build success.

Snapshots have finite retention. The
[official service documentation](https://snapshot.ubuntu.com/) describes its
retention intention, not permanent archival storage. Preserve validated images
and dependency assets for long-term recovery, and revalidate availability when
updating the base image or lock.
