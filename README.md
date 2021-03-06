![Tests](https://github.com/rictic/rust-us/workflows/Tests/badge.svg)

## Getting started

1. [install the rust toolchain](https://www.rust-lang.org/tools/install)
2. [install npm](https://www.npmjs.com/get-npm)
3. [install wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
4. install some more deps `apt-get install gcc libssl-dev pkg-config lld`
5. clone this repo
6. cd rust-us
7. ./start.sh
8. Once you see `ℹ ｢wdm｣: Compiled successfully.`, open your browser to http://localhost:8080/

This will also automatically watch the rust code in `./src/` and incrementally rebuild on changes. If you're lucky, your web browser will also automatically reload after a successful build.

## Improve Build Performance

This guide has a few pointers. I got ~20% faster compiles by following it: https://bevyengine.org/learn/book/getting-started/setup/#enable-fast-compiles-optional

## Based off of wasm-pack-template

A template for kick starting a Rust and WebAssembly project using <a href="https://github.com/rustwasm/wasm-pack">wasm-pack</a>.

**[Tutorial](https://rustwasm.github.io/docs/wasm-pack/tutorials/npm-browser-packages/index.html)** – [wasm-pack-template discord](https://discordapp.com/channels/442252698964721669/443151097398296587)

## About

[**📚 Read this template tutorial! 📚**][template-docs]

This template is designed for compiling Rust libraries into WebAssembly and
publishing the resulting package to NPM.

Be sure to check out [other `wasm-pack` tutorials online][tutorials] for other
templates and usages of `wasm-pack`.

[tutorials]: https://rustwasm.github.io/docs/wasm-pack/tutorials/index.html
[template-docs]: https://rustwasm.github.io/docs/wasm-pack/tutorials/npm-browser-packages/index.html

### 🛠️ Build with `wasm-pack build`

```
wasm-pack build
```

### 🔬 Test in Headless Browsers with `wasm-pack test`

```
wasm-pack test --headless --firefox
```

### 🎁 Publish to NPM with `wasm-pack publish`

```
wasm-pack publish
```

## 🔋 Batteries Included

* [`wasm-bindgen`](https://github.com/rustwasm/wasm-bindgen) for communicating
  between WebAssembly and JavaScript.
* [`console_error_panic_hook`](https://github.com/rustwasm/console_error_panic_hook)
  for logging panic messages to the developer console.
* [`wee_alloc`](https://github.com/rustwasm/wee_alloc), an allocator optimized
  for small code size.
