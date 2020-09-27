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
document.body.appendChild(output);

const canvas = document.createElement('canvas');
canvas.id = 'canvas';
document.body.appendChild(canvas);

const maybeGame = wasm.make_game();
if (maybeGame.get_error()) {
  throw new Error(maybeGame.get_error());
}
const game = maybeGame.get_game();
if (!game) {
  throw new Error(`Failed to make a Game object`);
}
let previousFrameTime = performance.now();
function drawOneFrame(timestamp) {
  const elapsed = timestamp - previousFrameTime;
  previousFrameTime = timestamp;
  game.simulate(
      elapsed, curInput.up, curInput.down, curInput.left, curInput.right);
  game.draw()
  const maybeError = game.draw(
      curInput.up, curInput.down, curInput.left, curInput.right);
  if (maybeError == null) {
    output.innerText = 'All is well.';
  } else {
    output.innerText = `Failed to draw! ${maybeError}`;
  }
  requestAnimationFrame(drawOneFrame);
}
requestAnimationFrame(drawOneFrame);
