# When this shell script exits, kill all child jobs.
trap 'echo $(jobs -p)' EXIT

if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

echo "Building client..."
<<<<<<< HEAD
(cd client && wasm-pack build --release && cd ../www && npm run build)
cp -r www/dist server/dist
gzip -9 server/dist/*
echo "Client built, building and starting server..."
(cd server && cargo run --bin prod --release)
=======
(cd client && wasm-pack build && cd ../www && npm run build)
echo "Client built, building the server..."
(cd server && cargo build)

echo "Actually starting the prod server is still manual because it binds to port 80..."
>>>>>>> main
