# When this shell script exits, kill all child jobs.
trap 'echo $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

echo "Building client..."
(cd client && wasm-pack build --release && cd ../www && npm run build)
rm -rf server/dist
cp -r www/dist server/dist
gzip -9 server/dist/*
echo "Client built, building server..."
(cd server && cargo build --release)

echo "Actually starting the prod server is still manual because it binds to port 80..."
