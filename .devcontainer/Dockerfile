FROM ubuntu:26.04@sha256:889d056d5c6c0bfb55789ff3710681d68e50713cb562d2196dc07110599c7a6f

# The image and package lock jointly fix the compiler, libc, libstdc++, Python,
# and transitive system packages. Image builds never resolve package versions.
COPY .devcontainer/system-packages.lock .devcontainer/install-system.sh /opt/blackflower/
RUN bash /opt/blackflower/install-system.sh

COPY tools/style/install-shell-tools.sh /opt/blackflower/tools/style/
COPY tools/cooker/.python-version /opt/blackflower/python-version
COPY .devcontainer/install-tools.sh /opt/blackflower/install-tools.sh
RUN bash /opt/blackflower/install-tools.sh

ENV PATH="/opt/blackflower/build/style/bin:/usr/local/bin:/usr/bin:/bin" \
    VCPKG_ROOT="/home/ubuntu/vcpkg" \
    VCPKG_DISABLE_METRICS="1" \
    VCPKG_DOWNLOADS="/home/ubuntu/.cache/vcpkg/downloads" \
    VCPKG_DEFAULT_BINARY_CACHE="/home/ubuntu/.cache/vcpkg/archives" \
    SCCACHE_DIR="/home/ubuntu/.cache/sccache" \
    SCCACHE_IDLE_TIMEOUT="0" \
    UV_CACHE_DIR="/home/ubuntu/.cache/uv" \
    UV_PYTHON_DOWNLOADS="never" \
    UV_PYTHON_PREFERENCE="only-system" \
    npm_config_cache="/home/ubuntu/.cache/npm" \
    BLACKFLOWER_DEVCONTAINER="1"

COPY vcpkg.json /opt/blackflower/vcpkg.json
RUN mkdir -p "$VCPKG_DOWNLOADS" \
    && git init --initial-branch=main "$VCPKG_ROOT" \
    && git -C "$VCPKG_ROOT" remote add origin https://github.com/microsoft/vcpkg.git \
    && git -C "$VCPKG_ROOT" fetch --depth=1 origin \
    "$(jq -er '."builtin-baseline"' /opt/blackflower/vcpkg.json)" \
    && git -C "$VCPKG_ROOT" checkout --detach FETCH_HEAD \
    && "$VCPKG_ROOT/bootstrap-vcpkg.sh" -disableMetrics \
    && chown -R ubuntu:ubuntu "$VCPKG_ROOT"

# CI executes as root against a runner-owned checkout at this one fixed path.
RUN git config --system --add safe.directory /workspaces/blackflower \
    && git config --system --add safe.directory "$VCPKG_ROOT"

# Seed generated-volume directories; the entrypoint handles later UID changes.
RUN mkdir -p /workspaces/blackflower/build \
    /workspaces/blackflower/tools/cooker/.venv \
    /workspaces/blackflower/tools/style/node_modules \
    /home/ubuntu/.cache/vcpkg/downloads /home/ubuntu/.cache/vcpkg/archives \
    && chown -R ubuntu:ubuntu /workspaces /home/ubuntu/.cache

COPY --chmod=0755 .devcontainer/entrypoint.sh /opt/blackflower/entrypoint.sh
WORKDIR /workspaces/blackflower
ENTRYPOINT ["/opt/blackflower/entrypoint.sh"]
CMD ["sleep", "infinity"]
