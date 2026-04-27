#!/bin/bash
set -e

echo "=============================================="
echo "🚀 Installing Cloudflare WARP..."
echo "=============================================="

# Add Cloudflare WARP repo and key
curl -fsSL https://pkg.cloudflareclient.com/pubkey.gpg \
  | sudo gpg --dearmor -o /usr/share/keyrings/cloudflare-warp.gpg

echo "deb [signed-by=/usr/share/keyrings/cloudflare-warp.gpg] https://pkg.cloudflareclient.com/ $(lsb_release -cs) main" \
  | sudo tee /etc/apt/sources.list.d/cloudflare-client.list

echo "🔄 Updating package list..."
sudo apt-get update -y

echo "⬇️ Installing cloudflare-warp..."
sudo apt-get install -y cloudflare-warp

echo
echo "=============================================="
echo "🔗 Connecting WARP..."
echo "=============================================="

sudo warp-cli --accept-tos registration new || true
sudo warp-cli --accept-tos connect || true

sleep 6

echo
echo "=============================================="
echo "📊 WARP STATUS"
echo "=============================================="
sudo warp-cli status || true

echo
echo "=============================================="
echo "🌐 IP INFO (after WARP connection)"
echo "=============================================="
curl -s https://ipinfo.io
echo
