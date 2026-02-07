# Docker Compose Deployment

Deploy a 3-node Enigma cluster with 3 RustFS backends using Docker Compose.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) with Compose v2

## Usage

```bash
# 1. Build and start
docker compose up -d --build

# 2. Create buckets on each RustFS
for port in 19001 19002 19003; do
  aws --endpoint-url http://127.0.0.1:$port s3 mb s3://enigma-chunks 2>/dev/null || true
done

# 3. Test via any of the 3 enigma nodes (ports 8333, 8334, 8335)
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp /etc/hostname s3://my-bucket/hostname.txt
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/hostname.txt -

# 4. Verify data is distributed across RustFS instances
for port in 19001 19002 19003; do
  echo "=== RustFS on port $port ==="
  aws --endpoint-url http://127.0.0.1:$port s3 ls s3://enigma-chunks/enigma/chunks/ --recursive | wc -l
done

# 5. Stop
docker compose down -v
```

## Ports

| Service | Port | Description |
|---------|------|-------------|
| enigma-0 | 8333 | S3 gateway (node 1) |
| enigma-1 | 8334 | S3 gateway (node 2) |
| enigma-2 | 8335 | S3 gateway (node 3) |
| enigma-0 | 9100 | Raft gRPC (node 1) |
| enigma-1 | 9101 | Raft gRPC (node 2) |
| enigma-2 | 9102 | Raft gRPC (node 3) |
| rustfs-1 | 19001 | RustFS backend 1 |
| rustfs-2 | 19002 | RustFS backend 2 |
| rustfs-3 | 19003 | RustFS backend 3 |
