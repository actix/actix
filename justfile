msrv := ```
    cargo metadata --format-version=1 \
    | jq -r 'first(.packages[] | select(.source == null and .rust_version)) | .rust_version' \
    | sed -E 's/^1\.([0-9]{2})$/1\.\1\.0/'
```
msrv_rustup := "+" + msrv

# Format project.
fmt:
    cargo +nightly fmt
    fd --type=file --hidden --extension=yml --extension=md --extension=js --exec-batch npx -y prettier --write

# Run Clippy over workspace.
clippy:
    cargo clippy --workspace --all-targets --all-features

# Downgrade dev-dependencies necessary to run MSRV checks/tests.
[private]
downgrade-for-msrv:
    @ echo "No MSRV downgrades currently required."
    # cargo update -p=trybuild --precise=1.0.89

# Test workspace using MSRV.
test-msrv: downgrade-for-msrv (test msrv_rustup)

# Test workspace code.
test toolchain="":
    cargo {{ toolchain }} test -p=actix_derive --lib --tests --all-features
    cargo {{ toolchain }} nextest run --workspace --exclude=actix_derive --no-default-features
    cargo {{ toolchain }} nextest run --workspace --exclude=actix_derive --all-features

# Test workspace docs.
test-docs toolchain="": && doc
    cargo {{ toolchain }} test --doc --workspace --all-features --no-fail-fast -- --nocapture

# Run tests on all crates in workspace and produce coverage file (Codecov format).
test-coverage-codecov toolchain="":
    cargo {{ toolchain }} llvm-cov --workspace --all-features --codecov --output-path codecov.json

# Run tests on all crates in workspace and produce coverage file (lcov format).
test-coverage-lcov toolchain="":
    cargo {{ toolchain }} llvm-cov --workspace --all-features --lcov --output-path lcov.info

# Document crates in workspace.
doc *args:
    RUSTDOCFLAGS="--cfg=docsrs -Dwarnings" cargo +nightly doc --no-deps --workspace --all-features {{ args }}

# Test workspace.
test-all toolchain="": (test toolchain) (test-docs toolchain)
