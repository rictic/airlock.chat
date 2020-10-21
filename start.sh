# Starts the dev servers.
# They will rebuild and restart automatically as you make changes to files.
# Once the build is complete, the app will be running on http://localhost:8080/

# When this shell script exits, kill all child jobs.
trap 'kill $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

# Start the web devserver
cd www/
npm run start &
cd ../

# Make sure cargo-watch is installed
cargo install cargo-watch

# Start the websocket server, and rebuild and relaunch it as necessary.
cargo watch -x 'run -p server --bin dev' &

# Rebuild the client wasm binary each time the filesystem is changed.
cd client
cargo watch -s 'wasm-pack build --target web --release && rm -rf ../www/wasm && cp -r ./pkg ../www/wasm' -w src/ -w Cargo.toml -w ../core/src/ -w ../core/Cargo.toml
