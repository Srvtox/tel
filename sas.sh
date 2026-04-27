#!/bin/bash
set -e

# متغیرهای مهم
DISPLAY_NUM=:1
LOGFILE=stream.log.txt
RTMP_URL="rtmp://5.106.7.97/stream/69ef6cc71e16402b99d336a6takmjjwrggedgamooxbyfsmxdvhwvizj?pt=qyybesmjkbtpycattnfujdkowpqdikyh"
VIDEO_SIZE=1280x720
FRAMERATE=25
BITRATE=2500k

echo "شروع اجرای Xvfb روی شماره صفحه $DISPLAY_NUM..."
Xvfb $DISPLAY_NUM -screen 0 ${VIDEO_SIZE}x24 &
XVFB_PID=$!

sleep 5 # صبر کن Xvfb کامل بالا بیاد

echo "اجرای XFCE4..."
startxfce4 &

sleep 10 # صبر کن محیط گرافیکی راه بیفته

echo "شروع FFmpeg جهت کپچر و ارسال به RTMP..."
export DISPLAY=$DISPLAY_NUM

ffmpeg -f x11grab -framerate $FRAMERATE -video_size $VIDEO_SIZE -i ${DISPLAY_NUM}.0 \
  -c:v libx264 -preset veryfast -b:v $BITRATE -pix_fmt yuv420p \
  -f flv "$RTMP_URL" > $LOGFILE 2>&1 &

FFMPEG_PID=$!

echo "استریم در حال اجراست. PID ffmpeg: $FFMPEG_PID"
echo "تمام لاگ‌ها در فایل $LOGFILE ذخیره می‌شود."

# صبر کردن تا ffmpeg تموم کنه (در صورت تمایل)
wait $FFMPEG_PID

# در صورت پایان، Xvfb رو ببند
kill $XVFB_PID
