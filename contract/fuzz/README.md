# Adversarial Fuzzing Infrastructure for Commit-Reveal Mechanism

This directory contains fuzzing harnesses targeting the cryptographic security of the commit-reveal randomness mechanism.

## Fuzzing Targets

### 1. `fuzz_commit_reveal` - Outcome Prediction
**Goal**: Attempt to predict game outcomes before the reveal phase.

**Attack Vectors**:
- Predict outcome from commitment alone
- Brute-force secret space
- Timing attacks on reveal processing
- Ledger sequence prediction

**Run**:
```bash
cargo +nightly fuzz run fuzz_commit_reveal -- -max_len=100 -timeout=10
```

### 2. `fuzz_commitment_collision` - Hash Collision Resistance
**Goal**: Find two different secrets that produce the same commitment hash.

**Attack Vectors**:
- Birthday attack on SHA-256
- Weak hash function exploitation
- Collision in commitment scheme

**Run**:
```bash
cargo +nightly fuzz run fuzz_commitment_collision -- -max_len=1000 -timeout=10
```

### 3. `fuzz_ledger_sequence` - Ledger Manipulation
**Goal**: Exploit ledger sequence predictability or manipulation.

**Attack Vectors**:
- Predict future ledger sequences
- Manipulate sequence generation
- Force specific contract_random values
- Timing-based sequence control

**Run**:
```bash
cargo +nightly fuzz run fuzz_ledger_sequence -- -max_len=100 -timeout=10
```

## Running All Fuzzers

```bash
# Run all fuzzers for 1 hour each
for target in fuzz_commit_reveal fuzz_commitment_collision fuzz_ledger_sequence; do
  echo "Fuzzing $target..."
  cargo +nightly fuzz run $target -- -max_total_time=3600 -timeout=10
done
```

## Interpreting Results

- **No crashes**: Cryptographic mechanism is robust against tested attack vectors
- **Crashes/panics**: Potential vulnerability found - investigate immediately
- **Slow inputs**: May indicate algorithmic complexity issues

## Coverage

Fuzzing targets are designed to achieve:
- 100% coverage of commitment verification logic
- 100% coverage of outcome generation
- 100% coverage of randomness derivation

## Corpus

Seed corpus files are stored in `corpus/` subdirectories:
- `corpus/fuzz_commit_reveal/` - Known edge cases for outcome prediction
- `corpus/fuzz_commitment_collision/` - Hash collision attempts
- `corpus/fuzz_ledger_sequence/` - Ledger sequence patterns

## Notes

- Fuzzing requires `cargo-fuzz`: `cargo install cargo-fuzz`
- Use `+nightly` toolchain for libfuzzer support
- Results are deterministic with fixed seeds for reproducibility
- Crashes are saved to `artifacts/` for analysis
