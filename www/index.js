import * as wasm from "rust-us";

const output = document.createElement('div');
output.innerText = wasm.greet();
document.body.appendChild(output);

const canvas = document.createElement('canvas');
canvas.id = 'canvas';
document.body.appendChild(canvas);
wasm.draw();
