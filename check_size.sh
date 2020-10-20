# Repeatedly rebuild the client and print the size of the
# created .wasm file.
#
# Useful when trying to improve binary size.

(cd client && cargo watch -s 'wasm-pack build --release && ls -a -l -h ./pkg/client_bg.wasm' -w src/ -w Cargo.toml -w ../core/src/ -w ../core/Cargo.toml)
