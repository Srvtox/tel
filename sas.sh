#!/bin/bash
set -e

DISPLAY_NUM=:1
LOGFILE=stream.log.txt
RTMP_URL="rtmp://live.iranhls.com/live/toGEL85zk6pesEYol47Rg?r=75e11600-1545-448d-b4be-3baca872bc64"
VIDEO_SIZE=1280x720
FRAMERATE=25
BITRATE=2500k

echo "کشتن Xvfb قبلی و پاک کردن قفل‌ها..."
pkill -f "Xvfb $DISPLAY_NUM" || true
rm -f /tmp/.X1-lock /tmp/.X11-unix/X1

echo "شروع Xvfb روی $DISPLAY_NUM ..."
Xvfb $DISPLAY_NUM -screen 0 ${VIDEO_SIZE}x24 &
XVFB_PID=$!

sleep 5

export DISPLAY=$DISPLAY_NUM

echo "شروع FFmpeg جهت کپچر و ارسال به RTMP..."
ffmpeg -f x11grab -framerate $FRAMERATE -video_size $VIDEO_SIZE -i ${DISPLAY_NUM}.0 \
  -c:v libx264 -preset veryfast -b:v $BITRATE -pix_fmt yuv420p \
  -f flv "$RTMP_URL" > $LOGFILE 2>&1 &

FFMPEG_PID=$!

echo "FFmpeg با PID $FFMPEG_PID در حال اجراست. لاگ‌ها در $LOGFILE ذخیره می‌شود."

wait $FFMPEG_PID

kill $XVFB_PID
