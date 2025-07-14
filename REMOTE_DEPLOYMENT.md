# Remote DNS Server Deployment Guide

This guide covers deploying the RIND DNS server on a remote host and configuring it to accept queries from your local machine.

## 🎯 Overview

Your DNS server will run on a remote host and be accessible from your local machine for DNS queries and API management.

## 🔧 Prerequisites

### Remote Host Requirements
- Docker Engine 20.10+
- Git access to your repository
- Open ports: 12312/udp (DNS) and 8080/tcp (API)
- Sufficient resources: 256MB RAM, 1 CPU core minimum

### Local Machine Requirements
- `dig` command for DNS testing
- `curl` for API testing
- SSH access to remote host

## 🚀 Deployment Steps

### 1. Prepare Remote Host

SSH into your remote host and set up the environment:

```bash
# SSH to remote host
ssh user@your-remote-host

# Update system
sudo apt update && sudo apt upgrade -y

# Install Docker if not already installed
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh
sudo usermod -aG docker $USER
newgrp docker

# Verify Docker installation
docker --version
```

### 2. Clone and Deploy

```bash
# Clone your repository
git clone https://github.com/Danton11/RIND.git
cd RIND

# Build the Docker image
docker build -t rind-dns:latest .

# Create production deployment
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

# Verify deployment
docker ps | grep rind-dns-server
docker logs rind-dns-server
```

### 3. Configure Firewall (if needed)

```bash
# For Ubuntu/Debian with ufw
sudo ufw allow 12312/udp comment "DNS Server"
sudo ufw allow 8080/tcp comment "DNS API"

# For CentOS/RHEL with firewalld
sudo firewall-cmd --permanent --add-port=12312/udp
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --reload

# For cloud providers, also configure security groups/firewall rules
```

## 🧪 Testing from Local Machine

Replace `YOUR_REMOTE_HOST_IP` with your actual remote host IP address.

### 1. Basic Connectivity Test

```bash
# Test DNS connectivity
dig @YOUR_REMOTE_HOST_IP -p 12312 localhost +short

# Test API connectivity
curl -s http://YOUR_REMOTE_HOST_IP:8080/ || echo "API endpoint check"
```

### 2. Add DNS Records via API

```bash
# Add a test record
curl -X POST http://YOUR_REMOTE_HOST_IP:8080/update \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test.example.com",
    "ip": "192.168.1.100",
    "ttl": 300,
    "record_type": "A",
    "class": "IN",
    "value": null
  }'

# Query the record
dig @YOUR_REMOTE_HOST_IP -p 12312 test.example.com
```

### 3. Performance Testing

```bash
# Single query timing
time dig @YOUR_REMOTE_HOST_IP -p 12312 google.com

# Multiple concurrent queries
for i in {1..10}; do
  dig @YOUR_REMOTE_HOST_IP -p 12312 localhost +short &
done
wait
```

## 🔄 Management Commands

### Update Deployment

```bash
# On remote host
cd RIND
git pull origin main
docker build -t rind-dns:latest .
docker stop rind-dns-server
docker rm rind-dns-server

# Redeploy with same command as above
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

### Monitor Server

```bash
# Check container status
docker ps | grep rind-dns-server

# View logs
docker logs -f rind-dns-server

# Check resource usage
docker stats rind-dns-server

# Health check
curl -s http://YOUR_REMOTE_HOST_IP:8080/ && echo "API OK"
dig @YOUR_REMOTE_HOST_IP -p 12312 localhost +short && echo "DNS OK"
```

## 🛡️ Security Considerations

### 1. Network Security

```bash
# Restrict API access to specific IPs (optional)
# Use iptables or cloud security groups to limit access to port 8080

# Example iptables rule to allow only your local IP
sudo iptables -A INPUT -p tcp --dport 8080 -s YOUR_LOCAL_IP -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 8080 -j DROP
```

### 2. DNS Security

```bash
# Monitor DNS queries
docker logs rind-dns-server | grep "Received packet"

# Rate limiting (implement in application or use external tools)
# Consider using fail2ban for additional protection
```

## 🚨 Troubleshooting

### Common Issues

#### 1. Connection Refused

```bash
# Check if ports are open on remote host
sudo netstat -tulpn | grep -E "(12312|8080)"

# Test from remote host locally first
dig @127.0.0.1 -p 12312 localhost
curl http://127.0.0.1:8080/
```

#### 2. Firewall Blocking

```bash
# Check firewall status
sudo ufw status  # Ubuntu/Debian
sudo firewall-cmd --list-all  # CentOS/RHEL

# Temporarily disable firewall for testing
sudo ufw disable  # Ubuntu/Debian
sudo systemctl stop firewalld  # CentOS/RHEL
```

#### 3. Docker Issues

```bash
# Check Docker daemon
sudo systemctl status docker

# Check container logs
docker logs rind-dns-server

# Restart container
docker restart rind-dns-server
```

## 📊 Monitoring Setup

### Simple Health Check Script

Create this on your local machine:

```bash
#!/bin/bash
# health_check_remote.sh

REMOTE_HOST="YOUR_REMOTE_HOST_IP"

echo "=== Remote DNS Server Health Check ==="
echo "Host: $REMOTE_HOST"
echo "Timestamp: $(date)"
echo

# DNS Health
echo "DNS Health:"
if dig @$REMOTE_HOST -p 12312 localhost +short > /dev/null 2>&1; then
  echo "✅ DNS is responding"
else
  echo "❌ DNS is not responding"
fi

# API Health
echo "API Health:"
if curl -s -f http://$REMOTE_HOST:8080/ > /dev/null 2>&1; then
  echo "✅ API is responding"
else
  echo "❌ API is not responding"
fi

# Performance test
echo "Performance Test:"
response_time=$(time dig @$REMOTE_HOST -p 12312 localhost +short 2>&1 | grep real | awk '{print $2}')
echo "DNS query response time: $response_time"
```



## 🔗 Quick Reference

### Essential Commands

```bash
# Local testing commands
dig @REMOTE_HOST_IP -p 12312 domain.com
curl -X POST http://REMOTE_HOST_IP:8080/update -H "Content-Type: application/json" -d '{...}'

# Remote management commands
ssh user@remote-host "docker logs rind-dns-server"
ssh user@remote-host "docker restart rind-dns-server"
ssh user@remote-host "docker stats rind-dns-server"
```

### Port Reference

- **12312/udp**: DNS queries
- **8080/tcp**: REST API for record management
- **9090/tcp**: Prometheus metrics endpoint

### Environment Variables

- `DNS_BIND_ADDR`: DNS server bind address (use "0.0.0.0:12312" for remote access)
- `API_BIND_ADDR`: API server bind address (use "0.0.0.0:8080" for remote access)
- `METRICS_PORT`: Metrics server port (default: 9090)
- `SERVER_ID`: Server instance identifier for metrics labels
- `RUST_LOG`: Logging level (debug, info, warn, error)

## 🧹 Cleanup and Removal

### Stop and Remove DNS Server

When you need to stop and clean up your remote DNS server deployment:

#### Manual SSH Cleanup

```bash
# SSH to remote host
ssh user@YOUR_REMOTE_HOST_IP

# Run these commands on the remote host:
docker stop rind-dns-server
docker rm rind-dns-server
docker rmi rind-dns:latest
docker volume rm dns_data
docker system prune -f

# Exit SSH
exit
```

#### One-Line Remote Command

```bash
# From your local machine, run everything remotely:
ssh user@YOUR_REMOTE_HOST_IP "docker stop rind-dns-server && docker rm rind-dns-server && docker rmi rind-dns:latest && docker volume rm dns_data 2>/dev/null || true && docker system prune -f"
```

### Check What's Running

Before cleanup, you can check what's currently deployed:

```bash
# Check running containers
ssh user@YOUR_REMOTE_HOST_IP "docker ps"

# Check all containers (including stopped)
ssh user@YOUR_REMOTE_HOST_IP "docker ps -a"

# Check Docker images
ssh user@YOUR_REMOTE_HOST_IP "docker images"

# Check volumes
ssh user@YOUR_REMOTE_HOST_IP "docker volume ls"
```

### Partial Cleanup Options

If you only want to stop the server temporarily:

```bash
# Stop container (keeps data)
ssh user@YOUR_REMOTE_HOST_IP "docker stop rind-dns-server"

# Start it again later
ssh user@YOUR_REMOTE_HOST_IP "docker start rind-dns-server"

# Restart container
ssh user@YOUR_REMOTE_HOST_IP "docker restart rind-dns-server"
```

Remember to replace `YOUR_REMOTE_HOST_IP` with your actual remote host IP address throughout this guide!