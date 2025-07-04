#!/bin/bash
set -e

echo "🐳 Building RIND DNS Server Docker Images"
echo "========================================"

# Build production image
echo "📦 Building production image..."
docker build -t rind-dns:latest -f Dockerfile .

# Build development image
echo "📦 Building development image..."
docker build -t rind-dns:dev -f Dockerfile.dev .

echo "✅ Docker images built successfully!"
echo ""
echo "Available images:"
docker images | grep rind-dns

echo ""
echo "🚀 Usage:"
echo "  Production: docker run -p 12312:12312/udp -p 8080:8080 rind-dns:latest"
echo "  Development: docker-compose -f docker-compose.dev.yml up"
echo "  Full stack: docker-compose up"