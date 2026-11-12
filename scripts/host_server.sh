#!/bin/sh
# Start a Tank Yard community server. In an extracted release folder (next to
# tankyard-server) the first run writes server.toml there for you to edit; in
# a source checkout it builds the server and uses configs/server.toml.
# Usage: ./host-server.sh [server options], such as --port 5001 --mode maze
set -eu
cd "$(dirname "$0")"
if [ -x ./tankyard-server ]; then
    server=./tankyard-server
    config=${TANKYARD_CONFIG:-server.toml}
    if [ ! -f "$config" ]; then
        cp configs/server.toml "$config"
        printf 'Wrote %s. Edit its name, password and bots, then run this again.\n' "$config"
        exit 0
    fi
else
    cd ..
    cargo build --locked --release -p tankyard-server --bin tankyard-server
    server=target/release/tankyard-server
    config=${TANKYARD_CONFIG:-configs/server.toml}
fi
exec "$server" --config "$config" "$@"
