if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

(wasm-pack build && cd www && npm install && npm run start) &
cargo install cargo-watch
cargo watch -s 'wasm-pack build'
