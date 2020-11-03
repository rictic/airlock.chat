import initWasm, * as wasm from './wasm/client.js';

async function getName() {
  const input = document.createElement('input');
  const label = document.createElement('label');
  label.textContent = 'Nickname: ';
  label.appendChild(input);
  document.body.appendChild(label);
  const name = await new Promise((resolve) => {
    input.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') {
        resolve(input.value);
      }
    });
  });
  document.body.removeChild(label);
  return name;
}

async function init() {
  await initWasm();
  console.log(
      await wasm.load_replay_over_network('0c15b58d38e1bf3de326c2bd5d03397e'));
  let name = window.localStorage.getItem('name');
  if (typeof name !== 'string') {
    name = await getName();
    window.localStorage.setItem('name', name);
  }
  console.log(`Name: ${name}`);

  const textOutput = document.createElement('div');
  textOutput.style.position = 'absolute';
  textOutput.style.marginLeft = '10px';
  textOutput.style.fontSize = '12px';
  textOutput.style.color = '#2c2d';
  document.body.appendChild(textOutput);
  const perf = document.createElement('div');
  textOutput.appendChild(perf);

  const canvas = document.createElement('canvas');
  canvas.id = 'canvas';
  document.body.appendChild(canvas);

  const simTimes = [];
  const drawTimes = [];
  const totalTimes = [];
  let perfIdx = 0;

  const game = wasm.make_game(name);
  let previousFrameTime = performance.now();
  let running = true;
  let displayPerf = window.localStorage.displayPerf === 'true';
  function drawOneFrame() {
    const timestamp = performance.now();
    const elapsed = timestamp - previousFrameTime;
    previousFrameTime = timestamp;
    const finished = game.simulate();
    const afterSim = performance.now();
    const simTime = afterSim - timestamp;
    game.draw();
    const afterDraw = performance.now();
    const drawTime = afterDraw - afterSim;
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
    let perfMessage = `${(1000 / average(totalTimes)).toFixed(1)}fps [`;
    perfMessage += `${average(simTimes).toFixed(1)}ms sim, `;
    perfMessage += ` ${average(drawTimes).toFixed(1)}ms draw]`;
    if (displayPerf) {
      perf.innerText = perfMessage;
    } else {
      perf.innerText = '';
    }
    running = !finished;
    if (!finished) {
      requestAnimationFrame(drawOneFrame);
    }
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
    'arrowup', 'arrowdown', 'arrowleft', 'arrowright',
    'j', 'k', 'l', 'f11'
  ]);
  const heldButtons = {};
  for (const button of knownButtons) {
    heldButtons[button] = false;
  }
  function updateInput() {
    const up = heldButtons['w'] || heldButtons['arrowup'];
    const down = heldButtons['s'] || heldButtons['arrowdown'];
    const left = heldButtons['a'] || heldButtons['arrowleft'];
    const right = heldButtons['d'] || heldButtons['arrowright'];
    const kill = heldButtons['q'];
    const report = heldButtons['r'];
    const activate = heldButtons['e'] || heldButtons[' '];
    const play = heldButtons['p'];
    const skip_back = heldButtons['j'];
    const skip_forward = heldButtons['l'];
    const pause_playback = heldButtons['k'];
    game.set_inputs(
      up, down, left, right, kill, report,
      activate, play, skip_back, skip_forward, pause_playback);
    if (!running) {
      running = true;
      requestAnimationFrame(drawOneFrame);
    }
  }
  document.addEventListener('keydown', (ev) => {
    const key = ev.key.toLowerCase();
    if (key == '/') {
      displayPerf = !displayPerf;
      window.localStorage.displayPerf = displayPerf;
      ev.preventDefault();
      return;
    }
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
}

init().catch((e) => {
  console.error(e);
});
