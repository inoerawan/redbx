build: pre
    cargo build --all-targets --all-features
    cargo doc

build_all: pre_all
    cargo build --all --all-targets --all-features
    cargo doc --all

pre:
    cargo deny --workspace --all-features check licenses
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features

pre_all:
    cargo deny --workspace --all-features check licenses
    cargo fmt --all -- --check
    cargo clippy --all --all-targets --all-features

release: pre
    cargo build --release

flamegraph:
    cargo flamegraph -p redbx-bench --bench encryption_overhead
    firefox ./flamegraph.svg

publish_py: test_py
    docker pull quay.io/pypa/manylinux2014_x86_64
    MATURIN_PYPI_TOKEN=$(cat ~/.pypi/redbx_token) docker run -it --rm -e "MATURIN_PYPI_TOKEN" -v `pwd`:/redbx-ro:ro quay.io/pypa/manylinux2014_x86_64 /redbx-ro/crates/redbx-python/py_publish.sh

test_py: install_py
    python3 -m unittest discover --start-directory=./crates/redbx-python

install_py: pre
    maturin develop --manifest-path=./crates/redbx-python/Cargo.toml

test: pre
    RUST_BACKTRACE=1 cargo test --all-features

test_all: build_all
    RUST_BACKTRACE=1 cargo test --all --all-features

test_wasi:
    rustup install nightly-2025-07-26 --target wasm32-wasip1-threads
    # Uses cargo pkgid because "redb" is ambiguous with the test dependency on an old version of redb
    cargo +nightly-2025-07-26 test -p $(cargo pkgid) --target=wasm32-wasip1-threads -- --nocapture
    cargo +nightly-2025-07-26 test -p redbx-derive --target=wasm32-wasip1-threads -- --nocapture

bench bench='encryption_overhead': pre
    cargo bench -p redbx-bench --bench {{bench}}

build_bench_container:
    docker build -t redbx-bench:latest -f Dockerfile.bench .

bench_containerized bench='lmdb_benchmark': build_bench_container
    # Exec the binary directly, because at low memory limits there may not be enough to invoke cargo & rustc
    docker run --rm -it --memory=4g redbx-bench:latest bash -c "cd /code/redbx && ./target/release/deps/{{bench}}-*"

watch +args='test':
    cargo watch --clear --exec "{{args}}"

fuzz: pre
    cargo fuzz run --sanitizer=none fuzz_redbx -- -max_len=10000

fuzz_cmin:
    cargo fuzz cmin --sanitizer=none fuzz_redbx -- -max_len=10000

fuzz_ci: pre_all
    cargo fuzz run --sanitizer=none fuzz_redbx -- -max_len=10000 -max_total_time=60

fuzz_coverage: pre
    #!/usr/bin/env bash
    set -euxo pipefail
    rustup component add llvm-tools-preview
    RUST_SYSROOT=`cargo rustc -- --print sysroot 2>/dev/null`
    LLVM_COV=`find $RUST_SYSROOT -name llvm-cov`
    echo $LLVM_COV
    cargo fuzz coverage --sanitizer=none fuzz_redbx
    $LLVM_COV show target/*/coverage/*/release/fuzz_redbx --format html \
          -instr-profile=fuzz/coverage/fuzz_redbx/coverage.profdata \
          -ignore-filename-regex='.*(cargo/registry|redbx/fuzz|rustc).*' > fuzz/coverage/coverage_report.html
    $LLVM_COV report target/*/coverage/*/release/fuzz_redbx \
          -instr-profile=fuzz/coverage/fuzz_redbx/coverage.profdata \
          -ignore-filename-regex='.*(cargo/registry|redbx/fuzz|rustc).*'
    firefox ./fuzz/coverage/coverage_report.html

# ── Mobile (Android) ──────────────────────────────────────────────────────────
#
# iOS was removed pending a macOS build host — see android/README.md.

# Verify the Android build toolchain is present
check_deps:
    bash scripts/check_mobile_deps.sh android

# Run redbx-mobile Rust unit tests on the host (no emulator required)
test_mobile:
    RUST_BACKTRACE=1 cargo test -p redbx-mobile

# Build Android .so libraries and generate the Kotlin UniFFI bindings
# Requires: cargo-ndk, Android NDK (ANDROID_NDK_HOME or NDK_HOME)
build_android: check_deps
    bash scripts/build_mobile.sh android

# Assemble the release AAR
aar_android: build_android
    cd android && ./gradlew :redbx:assembleRelease

# Compile the instrumentation tests without running them (no device needed)
check_android: build_android
    cd android && ./gradlew :redbx:assembleDebugAndroidTest

# Run the on-device tests — requires a connected device or a running emulator
test_android: build_android
    cd android && ./gradlew :redbx:connectedAndroidTest
