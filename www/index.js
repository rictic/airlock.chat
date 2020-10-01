import * as wasm from '../client/pkg/rust_us';

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
  const simError = game.simulate(elapsed);
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
    output.innerText = `${maybeError}`;
    return;
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
requestAnimationFrame(drawOneFrame);

function average(arr) {
  let sum = 0;
  for (const val of arr) {
    sum += val;
  }
  return sum / arr.length;
}

const knownButtons = new Set([
    'w', 'a', 's', 'd', 'q', 'e', 'r', ' ', 'p',
    'arrowup', 'arrowdown', 'arrowleft', 'arrowright'
]);
const heldButtons = {};
function updateInput() {
  const up = heldButtons['w'] || heldButtons['arrowup'];
  const down = heldButtons['s'] || heldButtons['arrowdown'];
  const left = heldButtons['a'] || heldButtons['arrowleft'];
  const right = heldButtons['d'] || heldButtons['arrowright'];
  const kill = heldButtons['q'];
  const report = heldButtons['r'];
  const activate = heldButtons['e'] || heldButtons[' '];
  const play = heldButtons['p'];
  game.set_inputs(up, down, left, right, kill, report, activate, play);
}
document.addEventListener('keydown', (ev) => {
  const key = ev.key.toLowerCase();
  if (!knownButtons.has(key)) {
    return;
  }
  heldButtons[ev.key.toLowerCase()] = true;
  updateInput();
  if (!(ev.ctrlKey || ev.metaKey || ev.altKey)) {
    ev.preventDefault();
  }
});
document.addEventListener('keyup', (ev) => {
  const key = ev.key.toLowerCase();
  if (!knownButtons.has(key)) {
    return;
  }
  heldButtons[key] = false;
  updateInput();
});
