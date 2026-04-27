#!/bin/bash
set -e

echo "======================================"
echo "Installing GUI + Streaming tools"
echo "======================================"

sudo apt-get update -y

sudo apt-get install -y \
  xvfb \
  xfce4 \
  xfce4-terminal \
  dbus-x11 \
  ffmpeg \
  x11vnc \
  wget -y

echo "======================================"
echo "Starting virtual display"
echo "======================================"

export DISPLAY=:99

Xvfb :99 -screen 0 1280x720x24 &
sleep 3

echo "======================================"
echo "Starting XFCE desktop"
echo "======================================"

startxfce4 &
sleep 8

echo "======================================"
echo "Starting screen streaming to Aparat (VIDEO ONLY)"
echo "======================================"

STREAM_KEY="ef6d74868f69db63a3c11a33a8f899d8f?s=54d20ca45fba421a"
RTMP_URL="rtmp://rtmp.cdn.asset.aparat.com:443/event/$STREAM_KEY"

echo "RTMP URL: $RTMP_URL"

ffmpeg \
  -f x11grab \
  -video_size 1280x720 \
  -framerate 30 \
  -i :99.0 \
  -c:v libx264 \
  -preset veryfast \
  -b:v 2500k \
  -maxrate 2500k \
  -bufsize 5000k \
  -pix_fmt yuv420p \
  -g 60 \
  -f flv "$RTMP_URL"
