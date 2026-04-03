#!/usr/bin/env bash
# Start tether + ngrok as persistent background services
# Usage: ./run.sh [password] [port]

NGROK="$HOME/.local/bin/ngrok"
TETHER="$HOME/Desktop/Axon/target/release/tether"
PASSWORD="${1:-tether123}"
PORT="${2:-8080}"
LOG_DIR="$HOME/Desktop/Axon/logs"
mkdir -p "$LOG_DIR"

# Kill any existing instances
pkill -9 -f "ngrok http" 2>/dev/null || true
pkill -9 -f "tether serve" 2>/dev/null || true
sleep 1

# 1. Start tether FIRST (it needs the port)
nohup "$TETHER" serve --password "$PASSWORD" --allow-lan --port "$PORT" > "$LOG_DIR/tether.log" 2>&1 &
TETHER_PID=$!
echo "tether started (PID: $TETHER_PID)"

# Wait for tether to be ready
sleep 2

# 2. Start ngrok (it connects to the already-running tether)
nohup "$NGROK" http "$PORT" --log=stdout --log-format=json > "$LOG_DIR/ngrok.log" 2>&1 &
NGROK_PID=$!
echo "ngrok started (PID: $NGROK_PID)"

# Wait for ngrok to get a public URL (up to 15 seconds)
echo "Waiting for ngrok tunnel..."
NGROK_URL=""
for i in $(seq 1 15); do
    NGROK_URL=$(curl -s http://127.0.0.1:4040/api/tunnels 2>/dev/null | grep -oP '"public_url":"https://[^"]+' | head -1 | cut -d'"' -f4)
    if [ -n "$NGROK_URL" ]; then
        break
    fi
    sleep 1
done

if [ -z "$NGROK_URL" ]; then
    echo "ERROR: ngrok did not get a URL in 15 seconds"
    echo "Check logs: cat $LOG_DIR/ngrok.log"
    kill $NGROK_PID $TETHER_PID 2>/dev/null
    exit 1
fi

echo ""
echo "=========================================="
echo "  Tether is running!"
echo "  URL: $NGROK_URL"
echo "  Password: $PASSWORD"
echo "=========================================="
echo ""
echo "To stop: kill $NGROK_PID $TETHER_PID"
echo "Logs:    tail -f $LOG_DIR/ngrok.log"
echo "         tail -f $LOG_DIR/tether.log"
