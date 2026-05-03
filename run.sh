#!/usr/bin/env bash

SESSION="ultraops"
PANE="$SESSION:0.0"

QUEUE="commands.txt"
PRIORITY="priority.txt"

LOG_DIR="archive"
LOG_FILE="session.log"

STATE_DIR=".state"
CWD_FILE="$STATE_DIR/cwd"

STATUS="status.md"

mkdir -p "$LOG_DIR"
mkdir -p "$STATE_DIR"

touch "$QUEUE"
touch "$PRIORITY"
touch "$LOG_DIR/$LOG_FILE"

now(){
date "+%Y-%m-%d %H:%M:%S"
}

log(){
echo "[$(now)] $1" >> "$LOG_DIR/$LOG_FILE"
}

ensure_tmux(){

if ! tmux has-session -t "$SESSION" 2>/dev/null
then

START_DIR=$(cat "$CWD_FILE" 2>/dev/null || echo "$HOME")

tmux new-session -d -s "$SESSION" -c "$START_DIR" bash

tmux pipe-pane -o -t "$PANE" \
"sed 's/\x1b\[[0-9;]*[a-zA-Z]//g' >> $LOG_DIR/$LOG_FILE"

fi

}

run_cmd(){

CMD="$1"

echo "$CMD" >> "$LOG_DIR/commands.log"

case "$CMD" in

"!CLEAR")
> "$LOG_DIR/$LOG_FILE"
log "log cleared"
return
;;

"!KILL")
tmux kill-session -t "$SESSION"
log "session killed"
return
;;

"!STATUS")
log "status requested"
return
;;

esac

tmux send-keys -t "$PANE" "$CMD" C-m

sleep 1

CWD=$(tmux send-keys -t "$PANE" "pwd" C-m ; sleep 1)

pwd > "$CWD_FILE"

}

update_status(){

{
echo "# UltraOps v15"
echo ""
echo "Last update: $(now)"
echo ""
echo "## tail log"
echo ""
echo '
```'
tail -n 200 "$LOG_DIR/$LOG_FILE"
echo '
```'
} > "$STATUS"

}

git_sync(){

git config user.name "ultraops"
git config user.email "bot@ultraops"

git add .

git commit -m "sync $(date +%s)" 2>/dev/null || true

git push 2>/dev/null || true

}

ensure_tmux

log "engine started"

START=$(date +%s)

while true
do

if [ -s "$PRIORITY" ]
then

CMD=$(head -n1 "$PRIORITY")
sed -i '1d' "$PRIORITY"
run_cmd "$CMD"

elif [ -s "$QUEUE" ]
then

CMD=$(head -n1 "$QUEUE")
sed -i '1d' "$QUEUE"
run_cmd "$CMD"

fi

update_status

git_sync

NOW=$(date +%s)

if (( NOW - START > 3000 ))
then
log "self restart trigger"
exit 0
fi

sleep 2

done
