#!/usr/bin/env bash
set -euo pipefail

# E2E test: backup to 3 RustFS instances on Kind, then restore and verify
# Prerequisites: 3 RustFS pods running, port-forwards on 19001-19003, buckets created

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ENIGMA="$PROJECT_DIR/target/debug/enigma"
TEST_DIR=$(mktemp -d)
CONFIG_DIR="$TEST_DIR/enigma-config"
SOURCE_DIR="$TEST_DIR/source"
RESTORE_DIR="$TEST_DIR/restored"
PASSPHRASE="test-e2e-passphrase-42"

echo "=== Enigma E2E Test with 3 RustFS instances ==="
echo "Test dir: $TEST_DIR"

# ── Step 1: Create test data ──────────────────────────
echo ""
echo "--- Creating test data ---"
mkdir -p "$SOURCE_DIR/subdir"

# Text files
echo "Hello, Enigma! This is file 1." > "$SOURCE_DIR/file1.txt"
echo "Multi-cloud encrypted backup works!" > "$SOURCE_DIR/file2.txt"
echo "Nested file in a subdirectory" > "$SOURCE_DIR/subdir/nested.txt"

# Binary file (pseudo-random, 100KB)
dd if=/dev/urandom of="$SOURCE_DIR/random.bin" bs=1024 count=100 2>/dev/null

# Duplicate content (for dedup testing)
cp "$SOURCE_DIR/file1.txt" "$SOURCE_DIR/file1_copy.txt"

echo "Created 5 test files"
ls -la "$SOURCE_DIR"
ls -la "$SOURCE_DIR/subdir"

# ── Step 2: Init Enigma ──────────────────────────────
echo ""
echo "--- Initializing Enigma ---"
$ENIGMA --config-dir "$CONFIG_DIR" --passphrase "$PASSPHRASE" init

# ── Step 3: Write config with 3 RustFS providers ─────
echo ""
echo "--- Configuring 3 RustFS providers ---"
cat > "$CONFIG_DIR/enigma.toml" << 'TOML'
[enigma]
db_path = "DB_PATH_PLACEHOLDER"
key_provider = "local"
keyfile_path = "KEYFILE_PLACEHOLDER"
distribution = "RoundRobin"

[enigma.chunk_strategy.Fixed]
size = 32768

[[providers]]
name = "rustfs-1"
type = "S3Compatible"
bucket = "enigma-chunks"
region = "us-east-1"
endpoint_url = "http://127.0.0.1:19001"
path_style = true
access_key = "enigma-key-1"
secret_key = "enigma-secret-1"
weight = 1

[[providers]]
name = "rustfs-2"
type = "S3Compatible"
bucket = "enigma-chunks"
region = "us-east-1"
endpoint_url = "http://127.0.0.1:19002"
path_style = true
access_key = "enigma-key-2"
secret_key = "enigma-secret-2"
weight = 1

[[providers]]
name = "rustfs-3"
type = "S3Compatible"
bucket = "enigma-chunks"
region = "us-east-1"
endpoint_url = "http://127.0.0.1:19003"
path_style = true
access_key = "enigma-key-3"
secret_key = "enigma-secret-3"
weight = 1
TOML

# Fix paths in TOML
sed -i '' "s|DB_PATH_PLACEHOLDER|$CONFIG_DIR/enigma.db|g" "$CONFIG_DIR/enigma.toml"
sed -i '' "s|KEYFILE_PLACEHOLDER|$CONFIG_DIR/keys.enc|g" "$CONFIG_DIR/enigma.toml"

echo "Config written:"
cat "$CONFIG_DIR/enigma.toml"

# ── Step 4: Backup ────────────────────────────────────
echo ""
echo "--- Running backup ---"
$ENIGMA --config-dir "$CONFIG_DIR" --passphrase "$PASSPHRASE" backup "$SOURCE_DIR"

# ── Step 5: List backups ──────────────────────────────
echo ""
echo "--- Listing backups ---"
$ENIGMA --config-dir "$CONFIG_DIR" list

# Get backup ID
BACKUP_ID=$($ENIGMA --config-dir "$CONFIG_DIR" list 2>/dev/null | tail -1 | awk '{print $1}')
echo "Backup ID: $BACKUP_ID"

# ── Step 6: Check chunks on each RustFS ───────────────
echo ""
echo "--- Checking chunk distribution ---"
echo "RustFS-1:" && mc ls --recursive rustfs1/enigma-chunks/enigma/chunks/ 2>/dev/null | wc -l | xargs echo "  chunks:"
echo "RustFS-2:" && mc ls --recursive rustfs2/enigma-chunks/enigma/chunks/ 2>/dev/null | wc -l | xargs echo "  chunks:"
echo "RustFS-3:" && mc ls --recursive rustfs3/enigma-chunks/enigma/chunks/ 2>/dev/null | wc -l | xargs echo "  chunks:"

# ── Step 7: Verify ────────────────────────────────────
echo ""
echo "--- Verifying backup ---"
$ENIGMA --config-dir "$CONFIG_DIR" --passphrase "$PASSPHRASE" verify "$BACKUP_ID"

# ── Step 8: Restore ───────────────────────────────────
echo ""
echo "--- Restoring backup ---"
$ENIGMA --config-dir "$CONFIG_DIR" --passphrase "$PASSPHRASE" restore "$BACKUP_ID" "$RESTORE_DIR"

# ── Step 9: Diff original vs restored ─────────────────
echo ""
echo "--- Comparing original vs restored ---"
if diff -r "$SOURCE_DIR" "$RESTORE_DIR" > /dev/null 2>&1; then
    echo "SUCCESS: Restored files are identical to originals!"
else
    echo "FAILURE: Files differ!"
    diff -r "$SOURCE_DIR" "$RESTORE_DIR"
    exit 1
fi

# ── Step 10: Status ───────────────────────────────────
echo ""
echo "--- Final status ---"
$ENIGMA --config-dir "$CONFIG_DIR" status

echo ""
echo "=== E2E TEST PASSED ==="
echo "Cleanup: rm -rf $TEST_DIR"
