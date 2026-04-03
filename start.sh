#!/usr/bin/env bash
set -e

NGROK_BIN="$HOME/.local/bin/ngrok"
TETHER_BIN="./target/release/tether"
PASSWORD="${TETHER_PASSWORD:-tether123}"
PORT="${TETHER_PORT:-8080}"

# Kill any existing ngrok/tether processes
pkill -f "ngrok http" 2>/dev/null || true
pkill -f "tether serve" 2>/dev/null || true
sleep 1

# Start ngrok in background, capture its output
echo "Starting ngrok tunnel on port $PORT..."
"$NGROK_BIN" http "$PORT" --log=stdout --log-format=json 2>&1 | while IFS= read -r line; do
    # Extract the public URL from ngrok log
    url=$(echo "$line" | grep -oP '"url":"https://[^"]+' | head -1 | cut -d'"' -f4)
    if [ -n "$url" ]; then
        echo "$url" > /tmp/tether-ngrok-url.txt
        echo "Public URL: $url"
        break
    fi
done &
NGROK_PID=$!

# Wait for ngrok URL to be available
echo "Waiting for ngrok tunnel..."
for i in $(seq 1 15); do
    if [ -f /tmp/tether-ngrok-url.txt ]; then
        NGROK_URL=$(cat /tmp/tether-ngrok-url.txt)
        break
    fi
    sleep 1
done

if [ -z "$NGROK_URL" ]; then
    echo "ERROR: ngrok tunnel did not start in time"
    kill $NGROK_PID 2>/dev/null
    exit 1
fi

echo "ngrok URL: $NGROK_URL"

# Start tether with the ngrok URL
echo "Starting tether server..."
exec "$TETHER_BIN" serve --password "$PASSWORD" --allow-lan --port "$PORT" --external-url "$NGROK_URL"
