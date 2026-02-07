# Cloud-Init VM Deployment

Bootstrap Enigma nodes on cloud VMs (AWS EC2, GCP Compute, Azure VM, etc.) using cloud-init.

## Usage

Pass `cloud-init.yaml` as user-data when creating VMs.

### AWS EC2

```bash
aws ec2 run-instances \
  --image-id ami-0abcdef1234567890 \
  --instance-type t3.medium \
  --count 3 \
  --user-data file://cloud-init.yaml \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=enigma}]'
```

### GCP Compute Engine

```bash
gcloud compute instances create enigma-0 enigma-1 enigma-2 \
  --machine-type e2-medium \
  --image-family ubuntu-2204-lts \
  --image-project ubuntu-os-cloud \
  --metadata-from-file user-data=cloud-init.yaml
```

### Azure VM

```bash
az vm create \
  --resource-group enigma-rg \
  --name enigma-0 \
  --image Ubuntu2204 \
  --size Standard_B2s \
  --custom-data cloud-init.yaml \
  --count 3
```

## Customization

Edit the variables at the top of `cloud-init.yaml`:

- `ENIGMA_NODE_ID`: Set per-node (1, 2, 3)
- `ENIGMA_PEERS`: Comma-separated list of `id:host:port`
- `ENIGMA_PASSPHRASE`: Encryption passphrase
- Storage backend credentials

For production, use instance metadata or a secrets manager instead of hardcoded values.
