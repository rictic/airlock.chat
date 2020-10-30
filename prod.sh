if test -d "www/node_modules"; then
  echo 'skipping npm install'
else
  (cd www && npm ci)
fi

echo "Building client..."
(cd client && wasm-pack build --release --target web)

echo "Client built, building the server..."
cargo build --release -p server --bin prod

# Create server/dist as the path for the prod server to serve the client
# out of.
rm -rf server/dist/
mkdir server/dist
cp -r www/*.html www/*.js www/assets www/pwa_manifest.json server/dist/
cp -r client/pkg server/dist/wasm

mkdir -p site_data/replays

# Copy this version of the client into a directory for serving out historical
# versions of the client for replays.
mkdir -p site_data/client_versions
THIS_SHA=`git rev-parse HEAD`
rm -rf "site_data/client_versions/$THIS_SHA"
cp -r server/dist "site_data/client_versions/$THIS_SHA"

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
  echo "Running sudo setcap to allow prod server to bind to low ports"
  sudo setcap CAP_NET_BIND_SERVICE=+eip target/release/prod
fi

# kill the previous server, if any
kill `ps aux | grep -v grep | grep target/release/prod | tr -s ' ' | cut -d ' ' -f 2`
nohup ./target/release/prod >./nohup.out &
tail -f nohup.out
