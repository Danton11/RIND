# Remote Deployment

Deploy RIND on a remote host and query it from your local machine.

## Prerequisites

**Remote host:** Docker Engine 20.10+, ports 12312/udp and 8080/tcp open.
**Local machine:** `dig` and `curl`.

## Deploy

```bash
ssh user@your-remote-host

git clone https://github.com/Danton11/RIND.git
cd RIND

docker build -f docker/Dockerfile -t rind-dns:latest .

docker run -d --name rind-dns-server \
  -p 12312:12312/udp \
  -p 8080:8080/tcp \
  -p 9090:9090/tcp \
  -e DNS_BIND_ADDR="0.0.0.0:12312" \
  -e API_BIND_ADDR="0.0.0.0:8080" \
  -e METRICS_PORT="9090" \
  -e RUST_LOG=info \
  --restart unless-stopped \
  --memory="256m" \
  --cpus="1.0" \
  -v dns_data:/app/data \
  rind-dns:latest
```

## Firewall

```bash
# Ubuntu/Debian
sudo ufw allow 12312/udp
sudo ufw allow 8080/tcp

# CentOS/RHEL
sudo firewall-cmd --permanent --add-port=12312/udp
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --reload
```

## Test from Local Machine

Replace `REMOTE_IP` with your host's IP.

```bash
dig @REMOTE_IP -p 12312 localhost +short

curl -X POST http://REMOTE_IP:8080/update \
  -H "Content-Type: application/json" \
  -d '{"name": "test.example.com", "ip": "192.168.1.100", "ttl": 300, "record_type": "A", "class": "IN", "value": null}'

dig @REMOTE_IP -p 12312 test.example.com
```

## Update

```bash
ssh user@your-remote-host
cd RIND && git pull origin main
docker build -f docker/Dockerfile -t rind-dns:latest .
docker stop rind-dns-server && docker rm rind-dns-server
# Re-run the docker run command from above
```

## Management

```bash
ssh user@your-remote-host "docker logs -f rind-dns-server"
ssh user@your-remote-host "docker stats rind-dns-server"
ssh user@your-remote-host "docker restart rind-dns-server"
```

## Cleanup

```bash
ssh user@your-remote-host "docker stop rind-dns-server && docker rm rind-dns-server && docker rmi rind-dns:latest && docker volume rm dns_data"
```

## Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| 12312 | UDP | DNS queries |
| 8080 | TCP | REST API |
| 9090 | TCP | Prometheus metrics |
