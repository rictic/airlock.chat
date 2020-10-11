# When this shell script exits, kill all child jobs.
trap 'echo $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

echo "Building client..."
(cd client && wasm-pack build --release && cd ../www && npm run build)
rm -rf server/dist/
cp -r www/dist server/dist
gzip -9 server/dist/*
echo "Client built, building the server..."
(cd server && cargo build --release)

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
  echo "Running sudo setcap to allow prod server to bind to low ports"
  sudo setcap CAP_NET_BIND_SERVICE=+eip server/target/release/prod
fi

(cd server && nohup target/release/prod) &
