// @ts-check

import initWasm, * as wasm from './wasm/client.js';
import {GameHandler} from './game_handler.js';

async function init() {
  if (!window.parent) {
    console.error("Expected replay_player.html to run in an iframe");
  }
  const replayContentsPromise = new Promise((resolve) => {
    window.addEventListener('message', (ev) => {
      resolve(ev.data);
    }, {once: true});
  });
  window.parent.postMessage('readyForReplayAsString', '*');
  await initWasm();
  const game = await wasm.create_replay_game_from_string(
      await replayContentsPromise);
  new GameHandler(game);
}

init().catch((e) => {
  console.error(e);
});
