// Synthetic feasibility test; this gate is not a production security boundary.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import fs from 'node:fs/promises';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';

const chrome = process.env.KEYHARBOR_TEST_CHROME;
assert.ok(chrome, 'Set KEYHARBOR_TEST_CHROME');
const root = await fs.mkdtemp(path.join(os.tmpdir(), 'keyharbor-cdp-pipe-'));
let sideEffect;
const effect = new Promise(resolve => { sideEffect = resolve; });
const server = http.createServer((req, res) => {
  if (req.url === '/after-revoke') sideEffect();
  res.end('<title>synthetic fixture</title>');
});
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const origin = `http://127.0.0.1:${server.address().port}`;
const child = spawn(chrome, [
  '--headless', '--disable-gpu', '--remote-debugging-pipe',
  `--user-data-dir=${path.join(root, 'profile')}`, 'about:blank',
], {stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe']});
const pending = new Map();
let nextId = 0;
let buffer = '';
child.stdio[4].setEncoding('utf8');
child.stdio[4].on('data', chunk => {
  buffer += chunk;
  let end;
  while ((end = buffer.indexOf('\0')) !== -1) {
    const message = JSON.parse(buffer.slice(0, end));
    buffer = buffer.slice(end + 1);
    const promise = pending.get(message.id);
    if (!promise) continue;
    pending.delete(message.id);
    if (message.error) promise.reject(new Error(message.error.message));
    else promise.resolve(message.result);
  }
});
child.on('exit', () => {
  for (const promise of pending.values()) promise.reject(new Error('Chrome exited'));
  pending.clear();
});
function command(method, params = {}, sessionId) {
  const id = ++nextId;
  const result = new Promise((resolve, reject) => pending.set(id, {resolve, reject}));
  child.stdio[3].write(JSON.stringify({id, method, params, ...(sessionId && {sessionId})}) + '\0');
  return result;
}
let generation = 1;
function controlled(token, method, params, sessionId) {
  if (token !== generation) throw new Error('control revoked');
  return command(method, params, sessionId);
}
const timeout = ms => new Promise((_, reject) => setTimeout(() => reject(new Error('test timed out')), ms).unref());
try {
  await Promise.race([(async () => {
    const {targetId} = await command('Target.createTarget', {url: origin});
    const {sessionId} = await command('Target.attachToTarget', {targetId, flatten: true});
    await controlled(1, 'Runtime.evaluate', {
      expression: "localStorage.setItem('fixture', 'retained'); setTimeout(() => fetch('/after-revoke'), 500); 'scheduled'",
      returnByValue: true,
    }, sessionId);
    generation++;
    assert.throws(() => controlled(1, 'Runtime.evaluate', {expression: '1'}, sessionId), /revoked/);
    await effect;
    const result = await controlled(2, 'Runtime.evaluate', {
      expression: "localStorage.getItem('fixture')", returnByValue: true,
    }, sessionId);
    assert.equal(result.result.value, 'retained');
    await assert.rejects(fs.access(path.join(root, 'profile', 'DevToolsActivePort')));
    console.log('PASS: pipe gate rejects stale control and preserves storage; previously scheduled page code still executes');
  })(), timeout(20000)]);
} finally {
  if (child.exitCode === null) {
    child.kill('SIGTERM');
    await Promise.race([once(child, 'exit'), new Promise(resolve => setTimeout(resolve, 3000))]);
    if (child.exitCode === null) child.kill('SIGKILL');
  }
  server.close();
  await fs.rm(root, {recursive: true, force: true});
}
