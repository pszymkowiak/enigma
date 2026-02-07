# Kind (Kubernetes in Docker) Deployment

Deploy a 3-node Enigma cluster with 3 RustFS backends on a local Kind cluster.

## Prerequisites

- [kind](https://kind.sigs.k8s.io/)
- [kubectl](https://kubernetes.io/docs/tasks/tools/)
- [Docker](https://docs.docker.com/get-docker/)

## Usage

```bash
# 1. Create a Kind cluster
kind create cluster --name enigma --config kind-cluster.yaml

# 2. Build and load the Enigma image
cd ../..
docker build -t enigma-proxy:latest .
kind load docker-image enigma-proxy:latest --name enigma

# 3. Deploy RustFS backends
kubectl apply -f ../../k8s/rustfs.yaml

# 4. Wait for RustFS pods
kubectl -n enigma-test wait --for=condition=ready pod -l app=rustfs --timeout=120s

# 5. Create buckets on each RustFS instance
for i in 1 2 3; do
  kubectl -n enigma-test port-forward svc/rustfs-$i 900$i:9000 &
done
sleep 3
for i in 1 2 3; do
  aws --endpoint-url http://127.0.0.1:900$i s3 mb s3://enigma-chunks 2>/dev/null || true
done
kill %1 %2 %3 2>/dev/null

# 6. Deploy Enigma cluster
kubectl apply -f ../../k8s/enigma-cluster.yaml

# 7. Wait for Enigma pods
kubectl -n enigma-test wait --for=condition=ready pod -l app=enigma --timeout=180s

# 8. Test via port-forward
kubectl -n enigma-test port-forward svc/enigma-s3 8333:8333 &
sleep 2

aws --endpoint-url http://localhost:8333 s3 mb s3://test-bucket
echo "Hello Enigma on Kind!" | aws --endpoint-url http://localhost:8333 s3 cp - s3://test-bucket/hello.txt
aws --endpoint-url http://localhost:8333 s3 cp s3://test-bucket/hello.txt -

# 9. Cleanup
kind delete cluster --name enigma
```
