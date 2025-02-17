#!/bin/sh

# Default interval in hours
INTERVAL="${INTERVAL:-1}"

# Start Nginx in the background
nginx &

echo "Running pstatool every $INTERVAL hour..."

while true; do
    /usr/local/bin/pstatool --temp-folder /tmp/pstatool/ --svg-folder /usr/share/nginx/html/
    sleep $((INTERVAL * 60 * 60))
done
