# Ansible Deployment

Deploy a 3-node Enigma cluster on VMs using Ansible.

## Prerequisites

- 3 VMs (Ubuntu 22.04+ / Debian 12+) with SSH access
- Ansible 2.14+

## Inventory

Edit `inventory.ini` with your VM IPs:

```ini
[enigma]
enigma-0 ansible_host=10.0.1.10
enigma-1 ansible_host=10.0.1.11
enigma-2 ansible_host=10.0.1.12
```

## Usage

```bash
# Deploy
ansible-playbook -i inventory.ini playbook.yml

# Test
aws --endpoint-url http://10.0.1.10:8333 s3 mb s3://test
aws --endpoint-url http://10.0.1.10:8333 s3 cp /etc/hostname s3://test/
aws --endpoint-url http://10.0.1.10:8333 s3 ls s3://test/
```

## What It Does

1. Installs Docker and Docker Compose on each node
2. Copies the Enigma binary and config
3. Starts a RustFS container on each node (local backend)
4. Creates the `enigma-chunks` bucket
5. Starts the Enigma proxy with Raft peering between nodes
6. Configures systemd for auto-restart
