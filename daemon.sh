#!/bin/bash

REPO=${GITHUB_REPOSITORY}
RUN_ID=${GITHUB_RUN_ID}

log() {
  echo "[$(date)] $1"
}

execute_command() {

  CMD="$1"

  log "Executing: $CMD"

  echo "$CMD" >> archive/commands.log

  tmux send-keys -t remote "$CMD" C-m
  sleep 2

  tmux capture-pane -t remote -p > session/last_output.log
  tmux capture-pane -t remote -p >> session/session.log

}

update_status() {

  echo "# Terminal Status" > status.md
  echo "" >> status.md
  echo "Last update: $(date)" >> status.md
  echo "" >> status.md

  echo "## Last output" >> status.md
  echo "\`\`\`" >> status.md
  tail -n 20 session/last_output.log >> status.md
  echo "\`\`\`" >> status.md

}

dequeue() {

  if [ -s priority.txt ]; then
    CMD=$(head -n1 priority.txt)
    sed -i '1d' priority.txt
    echo "$CMD"
    return
  fi

  if [ -s commands.txt ]; then
    CMD=$(head -n1 commands.txt)
    sed -i '1d' commands.txt
    echo "$CMD"
    return
  fi

  echo ""
}

admin_commands() {

  case "$1" in

    "!CLEAR")
      > session/session.log
      > session/last_output.log
      ;;

    "!KILL")
      tmux kill-session -t remote
      tmux new-session -d -s remote
      ;;

    "!STATUS")
      update_status
      ;;

  esac

}

self_replicate() {

  NOW=$(date +%s)
  START=$GITHUB_RUN_ATTEMPT
  LIMIT=$((5*3600))

  if [ "$NOW" -gt "$LIMIT" ]; then

    log "Triggering new workflow"

    gh workflow run sys.yml || true

  fi

}

while true
do

  git pull --rebase --autostash --quiet || true

  CMD=$(dequeue)

  if [ -n "$CMD" ]; then

    if [[ "$CMD" == !* ]]; then
      admin_commands "$CMD"
    else
      execute_command "$CMD"
    fi

  fi

  update_status

  git add .
  git commit -m "terminal update [skip ci]" || true
  git push --quiet || true

  self_replicate

  sleep 5

done

