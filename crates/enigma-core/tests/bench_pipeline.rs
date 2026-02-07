//! Benchmark test for the Enigma data pipeline.
//!
//! Measures throughput of each stage and the full pipeline.
//! Run: cargo test -p enigma-core --test bench_pipeline -- --nocapture

use std::time::Instant;

use enigma_core::chunk::{CdcChunkEngine, ChunkEngine, FixedSizeChunkEngine};
use enigma_core::compression;
use enigma_core::crypto::{decrypt_chunk, encrypt_chunk};
use enigma_core::dedup::compute_hash;
use enigma_core::types::KeyMaterial;

fn test_key() -> KeyMaterial {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    KeyMaterial {
        id: "bench-key".to_string(),
        key,
    }
}

fn generate_data(size: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut data = vec![0u8; size];
    rand::rngs::OsRng.fill_bytes(&mut data);
    data
}

fn mb_per_sec(bytes: usize, elapsed: std::time::Duration) -> f64 {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    mb / elapsed.as_secs_f64()
}

#[test]
fn bench_sha256_hashing() {
    let sizes = [1_048_576, 4_194_304, 16_777_216]; // 1MB, 4MB, 16MB
    println!("\n=== SHA-256 Hashing ===");
    for size in sizes {
        let data = generate_data(size);
        let iterations = 20;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = compute_hash(&data);
        }
        let elapsed = start.elapsed();
        let total_bytes = size * iterations;
        println!(
            "  {:>4} MB chunk × {iterations}: {:.0} MB/s",
            size / (1024 * 1024),
            mb_per_sec(total_bytes, elapsed)
        );
    }
}

#[test]
fn bench_aes256gcm_encrypt() {
    let key = test_key();
    let sizes = [1_048_576, 4_194_304, 16_777_216];
    println!("\n=== AES-256-GCM Encryption ===");
    for size in sizes {
        let data = generate_data(size);
        let hash = compute_hash(&data);
        let iterations = 20;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = encrypt_chunk(&data, &hash, &key).unwrap();
        }
        let elapsed = start.elapsed();
        let total_bytes = size * iterations;
        println!(
            "  {:>4} MB chunk × {iterations}: {:.0} MB/s",
            size / (1024 * 1024),
            mb_per_sec(total_bytes, elapsed)
        );
    }
}

#[test]
fn bench_aes256gcm_decrypt() {
    let key = test_key();
    let sizes = [1_048_576, 4_194_304, 16_777_216];
    println!("\n=== AES-256-GCM Decryption ===");
    for size in sizes {
        let data = generate_data(size);
        let hash = compute_hash(&data);
        let encrypted = encrypt_chunk(&data, &hash, &key).unwrap();
        let iterations = 20;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = decrypt_chunk(&encrypted, &key).unwrap();
        }
        let elapsed = start.elapsed();
        let total_bytes = size * iterations;
        println!(
            "  {:>4} MB chunk × {iterations}: {:.0} MB/s",
            size / (1024 * 1024),
            mb_per_sec(total_bytes, elapsed)
        );
    }
}

#[test]
fn bench_zstd_compress() {
    let sizes = [1_048_576, 4_194_304, 16_777_216];
    println!("\n=== zstd Compression (level 3) ===");
    for size in sizes {
        let data = generate_data(size);
        let iterations = 10;
        let start = Instant::now();
        let mut compressed_size = 0;
        for _ in 0..iterations {
            let compressed = compression::compress_chunk(&data, 3).unwrap();
            compressed_size = compressed.len();
        }
        let elapsed = start.elapsed();
        let total_bytes = size * iterations;
        let ratio = compressed_size as f64 / size as f64 * 100.0;
        println!(
            "  {:>4} MB chunk × {iterations}: {:.0} MB/s (ratio: {:.1}% — random data)",
            size / (1024 * 1024),
            mb_per_sec(total_bytes, elapsed),
            ratio
        );
    }
}

#[test]
fn bench_zstd_compress_text() {
    let sizes = [1_048_576, 4_194_304];
    println!("\n=== zstd Compression (level 3, text-like data) ===");
    for size in sizes {
        // Generate repetitive text-like data
        let pattern = b"The quick brown fox jumps over the lazy dog. Enigma encrypts everything. ";
        let data: Vec<u8> = pattern.iter().cycle().take(size).copied().collect();
        let iterations = 10;
        let start = Instant::now();
        let mut compressed_size = 0;
        for _ in 0..iterations {
            let compressed = compression::compress_chunk(&data, 3).unwrap();
            compressed_size = compressed.len();
        }
        let elapsed = start.elapsed();
        let total_bytes = size * iterations;
        let ratio = compressed_size as f64 / size as f64 * 100.0;
        println!(
            "  {:>4} MB chunk × {iterations}: {:.0} MB/s (ratio: {:.1}%)",
            size / (1024 * 1024),
            mb_per_sec(total_bytes, elapsed),
            ratio
        );
    }
}

#[test]
fn bench_cdc_chunking() {
    println!("\n=== CDC Chunking (4 MB target) ===");
    let tmp = tempfile::TempDir::new().unwrap();
    let sizes = [4_194_304, 16_777_216, 67_108_864]; // 4MB, 16MB, 64MB
    let engine = CdcChunkEngine::new(4_194_304);

    for size in sizes {
        let data = generate_data(size);
        let path = tmp.path().join(format!("bench_{size}.dat"));
        std::fs::write(&path, &data).unwrap();

        let start = Instant::now();
        let chunks = engine.chunk_file(&path).unwrap();
        let elapsed = start.elapsed();

        println!(
            "  {:>4} MB file: {} chunks in {:.1}ms ({:.0} MB/s)",
            size / (1024 * 1024),
            chunks.len(),
            elapsed.as_secs_f64() * 1000.0,
            mb_per_sec(size, elapsed)
        );
    }
}

#[test]
fn bench_fixed_chunking() {
    println!("\n=== Fixed Chunking (4 MB) ===");
    let tmp = tempfile::TempDir::new().unwrap();
    let sizes = [4_194_304, 16_777_216, 67_108_864];
    let engine = FixedSizeChunkEngine::new(4_194_304);

    for size in sizes {
        let data = generate_data(size);
        let path = tmp.path().join(format!("bench_{size}.dat"));
        std::fs::write(&path, &data).unwrap();

        let start = Instant::now();
        let chunks = engine.chunk_file(&path).unwrap();
        let elapsed = start.elapsed();

        println!(
            "  {:>4} MB file: {} chunks in {:.1}ms ({:.0} MB/s)",
            size / (1024 * 1024),
            chunks.len(),
            elapsed.as_secs_f64() * 1000.0,
            mb_per_sec(size, elapsed)
        );
    }
}

#[test]
fn bench_full_pipeline() {
    println!("\n=== Full Pipeline: Chunk → Hash → Compress → Encrypt ===");
    let key = test_key();
    let tmp = tempfile::TempDir::new().unwrap();
    let sizes = [4_194_304, 16_777_216, 67_108_864]; // 4MB, 16MB, 64MB
    let engine = CdcChunkEngine::new(4_194_304);

    for size in sizes {
        let data = generate_data(size);
        let path = tmp.path().join(format!("bench_{size}.dat"));
        std::fs::write(&path, &data).unwrap();

        let start = Instant::now();

        // 1. Chunk
        let chunks = engine.chunk_file(&path).unwrap();

        // 2. Hash + Compress + Encrypt each chunk
        let mut total_encrypted_bytes = 0usize;
        for chunk in &chunks {
            let hash = compute_hash(&chunk.data);
            let compressed = compression::compress_chunk(&chunk.data, 3).unwrap();
            let encrypted = encrypt_chunk(&compressed, &hash, &key).unwrap();
            total_encrypted_bytes += encrypted.ciphertext.len();
        }

        let elapsed = start.elapsed();

        println!(
            "  {:>4} MB → {} chunks, {:.1} MB encrypted in {:.0}ms ({:.0} MB/s)",
            size / (1024 * 1024),
            chunks.len(),
            total_encrypted_bytes as f64 / (1024.0 * 1024.0),
            elapsed.as_secs_f64() * 1000.0,
            mb_per_sec(size, elapsed)
        );
    }
}
