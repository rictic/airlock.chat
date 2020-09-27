# When this shell script exits, kill all child jobs.
trap 'echo $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

(wasm-pack build && cd www && npm run start) &
cargo install cargo-watch
(cd server && cargo watch -x run) &
cargo watch -s 'wasm-pack build'
