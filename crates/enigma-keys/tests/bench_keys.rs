//! Benchmark for key derivation.
//!
//! Run: cargo test --release -p enigma-keys --test bench_keys -- --nocapture

use std::time::Instant;

#[test]
fn bench_key_derivation() {
    println!("\n=== Key Derivation (Argon2id + ML-KEM-768 + HKDF) ===");
    let tmp = tempfile::TempDir::new().unwrap();
    let keyfile = tmp.path().join("bench_keys.enc");

    let start = Instant::now();
    let _provider =
        enigma_keys::local::LocalKeyProvider::create(&keyfile, b"benchmark-passphrase").unwrap();
    let create_elapsed = start.elapsed();

    let start = Instant::now();
    let _provider =
        enigma_keys::local::LocalKeyProvider::open(&keyfile, b"benchmark-passphrase").unwrap();
    let open_elapsed = start.elapsed();

    println!(
        "  Create (keygen + encrypt): {:.0}ms",
        create_elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "  Open (decrypt + derive):   {:.0}ms",
        open_elapsed.as_secs_f64() * 1000.0
    );
}
