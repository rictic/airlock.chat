import * as wasm from "rust-us";

const curInput = {
  up: false,
  down: false,
  left: false,
  right: false
}

document.addEventListener('keydown', (ev) => {
  switch (ev.key) {
    case 'ArrowUp':
      curInput.up = true;
      break;
    case 'ArrowDown':
      curInput.down = true;
      break;
    case 'ArrowLeft':
      curInput.left = true;
      break;
    case 'ArrowRight':
      curInput.right = true;
      break;
  }
});
document.addEventListener('keyup', (ev) => {
  switch (ev.key) {
    case 'ArrowUp':
      curInput.up = false;
      break;
    case 'ArrowDown':
      curInput.down = false;
      break;
    case 'ArrowLeft':
      curInput.left = false;
      break;
    case 'ArrowRight':
      curInput.right = false;
      break;
  }
});

const output = document.createElement('div');
output.innerText = wasm.greet();
document.body.appendChild(output);

const canvas = document.createElement('canvas');
canvas.id = 'canvas';
document.body.appendChild(canvas);
function main() {
  const result = wasm.draw(
      curInput.up, curInput.down, curInput.left, curInput.right);
  if (result == null) {
    output.innerText = 'Failed to draw!';
  } else {
    output.innerText = 'All is well. (hold up for smile)';
  }
  requestAnimationFrame(main);
}
requestAnimationFrame(main);
