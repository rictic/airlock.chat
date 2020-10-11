# When this shell script exits, kill all child jobs.
trap 'echo $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

(cd client && wasm-pack build --dev && cd ../www && npm run start) &
cargo install cargo-watch
(cd server && cargo watch -x 'run --bin=dev' -w src/ -w Cargo.toml -w ../core/src/ -w ../core/Cargo.toml) &
(cd client && cargo watch -s 'wasm-pack build --dev' -w src/ -w Cargo.toml -w ../core/src/ -w ../core/Cargo.toml)
