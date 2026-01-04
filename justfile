# List all recipes in justfile
@list:
    just --list

# Get description from Cargo.toml
@description:
    which toml 1>/dev/null || cargo binstall -y toml-cli
    toml get -r Cargo.toml package.description

# Re-generate README file
readme:
    #!/usr/bin/env bash

    set -euo pipefail
    which cargo-readme 1>/dev/null || cargo binstall -y cargo-readme

    cat <<EOF > README.md
    # Reaper Regions
    ## $(just description)
    This crate includes both a [library](#reaper-regions-library) and a [CLI](#reaper-regions-cli). The CLI dependencies can be excluded with \`default-features = false\`.
    EOF

    echo >> README.md
    cargo readme --no-title >> README.md
    echo >> README.md
    echo '---' >> README.md
    echo >> README.md
    cargo readme --no-title -i src/main.rs >> README.md

# Strip test WAV files in tests/wav (.gitignored) and output to tests/fixtures
strip:
    cargo run -p strip -- tests/wav

# Run cargo test
test:
    cargo test

# To bless new goldens:
# 1. create test WAV files in Reaper.
# 2. drop in tests/wav (.gitignored).
# 3. Run `just bless`.
# 4. Check golden output for correctness.

# Bless goldens
bless:
    just strip
    BLESS=1 just test
