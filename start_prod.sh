if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

echo "Building client..."
(cd client && wasm-pack build --release && cd ../www && npm run build)
rm -rf server/dist/
cp -r www/dist server/dist
echo "Client built, building the server..."
cargo build --release

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
  echo "Running sudo setcap to allow prod server to bind to low ports"
  sudo setcap CAP_NET_BIND_SERVICE=+eip target/release/prod
fi

# kill the previous server, if any
kill `ps aux | grep -v grep | grep target/release/prod | tr -s ' ' | cut -d ' ' -f 2`
nohup ./target/release/prod &
