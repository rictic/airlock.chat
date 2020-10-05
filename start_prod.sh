# When this shell script exits, kill all child jobs.
trap 'echo $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

echo "Building client..."
(cd client && wasm-pack build && cd ../www && npm run build)
echo "Client built, building the server..."
(cd server && cargo build)

echo "Actually starting the prod server is still manual because it binds to port 80..."
