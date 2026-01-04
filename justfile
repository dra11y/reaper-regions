# List all recipes in justfile
@list:
    just --list

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
