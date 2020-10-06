# When this shell script exits, kill all child jobs.
trap 'echo $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

echo "Building client..."
(cd client && wasm-pack build --release && cd ../www && npm run build)
cp -r www/dist server/dist
gzip -9 server/dist/*
echo "Client built, building and starting server..."
(cd server && cargo run --bin prod --release)
