import * as wasm from 'rust-us';

const curInput = {
  up: false,
  down: false,
  left: false,
  right: false,
  q: false,
};

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
    case 'q':
    case 'Q':
      curInput.q = true;
      break;
  }

  // Stop keys from doing things like changing scroll bars
  // and filling out inputs:
  ev.preventDefault();
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
    case 'q':
      curInput.q = false;
      break;
  }
});

const output = document.createElement('div');
document.body.appendChild(output);

const canvas = document.createElement('canvas');
canvas.width = 1024;
canvas.height = 768;
canvas.id = 'canvas';
document.body.appendChild(canvas);

const simTimes = [];
const drawTimes = [];
const totalTimes = [];
let perfIdx = 0;

const game = wasm.make_game();
let previousFrameTime = performance.now();
function drawOneFrame() {
  const timestamp = performance.now();
  const elapsed = timestamp - previousFrameTime;
  previousFrameTime = timestamp;
  const simError = game.simulate(
    elapsed,
    curInput.up,
    curInput.down,
    curInput.left,
    curInput.right,
    curInput.q
  );
  const afterSim = performance.now();
  const simTime = afterSim - timestamp;
  const drawError = game.draw();
  const afterDraw = performance.now();
  const drawTime = afterDraw - afterSim;
  const maybeError = simError || drawError;
  let message;
  if (maybeError == null) {
    message = 'All is well.';
  } else {
    message = `Failed to draw! ${maybeError}`;
  }
  if (simTimes.length < 100) {
    simTimes.push(simTime);
  } else {
    simTimes[perfIdx] = simTime;
    perfIdx = (perfIdx + 1) % 100;
  }
  if (drawTimes.length < 100) {
    drawTimes.push(drawTime);
  } else {
    drawTimes[perfIdx] = drawTime;
  }
  if (totalTimes.length < 100) {
    totalTimes.push(elapsed);
  } else {
    totalTimes[perfIdx] = elapsed;
  }
  message += ` – ${average(simTimes).toFixed(1)}ms sim`;
  message += ` – ${average(drawTimes).toFixed(1)}ms draw`;
  message += ` – ${(1000 / average(totalTimes)).toFixed(1)}fps`;
  output.innerText = message;
  requestAnimationFrame(drawOneFrame);
}
function average(arr) {
  let sum = 0;
  for (const val of arr) {
    sum += val;
  }
  return sum / arr.length;
}

requestAnimationFrame(drawOneFrame);
