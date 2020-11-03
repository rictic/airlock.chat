// @ts-check

export class GameHandler {
  constructor(gameWrapper) {
    const textOutput = document.createElement('div');
    textOutput.style.position = 'absolute';
    textOutput.style.marginTop = '10px';
    textOutput.style.marginLeft = '10px';
    textOutput.style.fontSize = '12px';
    textOutput.style.color = '#2c2d';
    const perf = document.createElement('div');
    textOutput.appendChild(perf);
    document.body.prepend(textOutput);
    /** @private */
    this.perf = perf;
    /** @private */
    this.simTimes = [];
    /** @private */
    this.drawTimes = [];
    /** @private */
    this.totalTimes = [];
    /** @private */
    this.perfIdx = 0;
    /** @private */
    this.game = gameWrapper;
    /** @private */
    this.previousFrameTime = performance.now();
    /** @private */
    this.running = true;
    /** @private */
    this.displayPerf = window.localStorage.displayPerf === 'true';
    this.initInputHandling();
    requestAnimationFrame(() => this.drawOneFrame());
  }

  /** @private */
  drawOneFrame() {
    const timestamp = performance.now();
    const elapsed = timestamp - this.previousFrameTime;
    this.previousFrameTime = timestamp;
    const finished = this.game.simulate();
    const afterSim = performance.now();
    const simTime = afterSim - timestamp;
    this.game.draw();
    const afterDraw = performance.now();
    const drawTime = afterDraw - afterSim;
    if (this.simTimes.length < 100) {
      this.simTimes.push(simTime);
    } else {
      this.simTimes[this.perfIdx] = simTime;
      this.perfIdx = (this.perfIdx + 1) % 100;
    }
    if (this.drawTimes.length < 100) {
      this.drawTimes.push(drawTime);
    } else {
      this.drawTimes[this.perfIdx] = drawTime;
    }
    if (this.totalTimes.length < 100) {
      this.totalTimes.push(elapsed);
    } else {
      this.totalTimes[this.perfIdx] = elapsed;
    }
    let perfMessage = `${(1000 / average(this.totalTimes)).toFixed(1)}fps [`;
    perfMessage += `${average(this.simTimes).toFixed(1)}ms sim, `;
    perfMessage += ` ${average(this.drawTimes).toFixed(1)}ms draw]`;
    if (this.displayPerf) {
      this.perf.innerText = perfMessage;
    } else {
      this.perf.innerText = '';
    }
    this.running = !finished;
    if (!finished) {
      requestAnimationFrame(() => this.drawOneFrame());
    }
  }

  /** @private */
  initInputHandling() {
    const knownButtons = new Set([
      'w', 'a', 's', 'd', 'q', 'e', 'r', ' ', 'p',
      'arrowup', 'arrowdown', 'arrowleft', 'arrowright',
      'j', 'k', 'l', 'f11'
    ]);
    const heldButtons = {};
    for (const button of knownButtons) {
      heldButtons[button] = false;
    }
    const updateInput = () => {
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
      this.game.set_inputs(
        up, down, left, right, kill, report,
        activate, play, skip_back, skip_forward, pause_playback);
      if (!this.running) {
        this.running = true;
        requestAnimationFrame(() => this.drawOneFrame());
      }
    };
    document.addEventListener('keydown', (ev) => {
      const key = ev.key.toLowerCase();
      if (key == '/') {
        this.displayPerf = !this.displayPerf;
        window.localStorage.displayPerf = this.displayPerf;
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
}

function average(arr) {
  let sum = 0;
  for (const val of arr) {
    sum += val;
  }
  return sum / arr.length;
}
