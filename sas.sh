#!/bin/bash

set -e

STREAM_URL="rtmp://5.106.7.97/stream/69ef6cc71e16402b99d336a6takmjjwrggedgamooxbyfsmxdvhwvizj?pt=qyybesmjkbtpycattnfujdkowpqdikyh"

echo "[1] Installing packages..."
sudo apt update
sudo apt install -y ffmpeg xvfb xfce4 xfce4-terminal pulseaudio x11vnc

echo "[2] Starting Xvfb..."
export DISPLAY=:1
Xvfb :1 -screen 0 1280x720x24 &
sleep 2

echo "[3] Starting XFCE desktop..."
xfce4-session &
sleep 5

echo "[4] Starting virtual audio (optional)..."
pulseaudio --start
pactl load-module module-null-sink sink_name=VirtualSink
AUDIO_INPUT="-f pulse -i VirtualSink.monitor"

echo "[5] Starting stream to Aparat..."
ffmpeg \
    -video_size 1280x720 \
    -framerate 30 \
    -f x11grab -i $DISPLAY \
    $AUDIO_INPUT \
    -c:v libx264 -preset veryfast -b:v 2500k \
    -maxrate 2500k -bufsize 5000k \
    -c:a aac -b:a 128k \
    -f flv "$STREAM_URL"
