# When this shell script exits, kill all child jobs.
trap 'kill $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

(cd client && wasm-pack build --release)
cd www
npm run start &
cd ..
cargo install cargo-watch
cargo watch -x 'run -p server --bin dev' &
(cd client && cargo watch -s 'wasm-pack build --release' -w src/ -w Cargo.toml -w ../core/src/ -w ../core/Cargo.toml)
