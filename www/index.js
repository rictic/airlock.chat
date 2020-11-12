// @ts-check

import initWasm, * as wasm from './wasm/client.js';
import {GameHandler} from './game_handler.js';

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
  let name = window.localStorage.getItem('name');
  if (typeof name !== 'string') {
    name = await getName();
    window.localStorage.setItem('name', name);
  }
  let game = await wasm.make_game(name);
  if (game) {
    new GameHandler(game);
  }
}

init().catch((e) => {
  console.error(e);
});
