// Synthetic integration test. CDP is explicitly enabled for these temporary profiles.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import {execFile} from 'node:child_process';
import {once} from 'node:events';
import {promisify} from 'node:util';
import {pathToFileURL} from 'node:url';

const {KEYHARBOR_TEST_PLAYWRIGHT, KEYHARBOR_TEST_CHROME, KEYHARBOR_BINARY} = process.env;
assert.ok(KEYHARBOR_TEST_PLAYWRIGHT && KEYHARBOR_TEST_CHROME && KEYHARBOR_BINARY, 'Set KEYHARBOR_TEST_PLAYWRIGHT, KEYHARBOR_TEST_CHROME and KEYHARBOR_BINARY');
const {chromium} = await import(pathToFileURL(KEYHARBOR_TEST_PLAYWRIGHT).href);
const exec = promisify(execFile);
const root = await fs.mkdtemp(path.join(os.tmpdir(), 'keyharbor-isolation-test-'));
const data = path.join(root, 'data');
const wrapper = path.join(root, 'browser');
const quote = value => `'${value.replaceAll("'", "'\\''")}'`;
await fs.writeFile(wrapper, `#!/bin/sh\nexec ${quote(KEYHARBOR_TEST_CHROME)} --headless --disable-gpu "$@"\n`, {mode: 0o700});
const site = http.createServer((req,res) => {
  if(req.url === '/a' || req.url === '/b') res.setHeader('Set-Cookie',`fixture=${req.url.slice(1)}; Max-Age=3600; Path=/; SameSite=Lax`);
  res.setHeader('Content-Type','text/html'); res.end('<title>KeyHarbor fixture</title>');
});
site.listen(0, '127.0.0.1'); await once(site, 'listening');
const origin = `http://127.0.0.1:${site.address().port}`;
const cli = async (...args) => (await exec(KEYHARBOR_BINARY, ['--data-dir', data, ...args])).stdout;
const browsers = new Set();
async function wait(fn, description) {
  const deadline=Date.now()+20000;
  while(Date.now()<deadline) { if(await fn()) return; await new Promise(resolve=>setTimeout(resolve,100)); }
  throw new Error(`Timed out: ${description}`);
}
async function open(id) {
  await cli('open',id,'--browser',wrapper,'--cdp');
  let endpoint;
  await wait(async()=>{try{endpoint=(await cli('endpoint',id)).trim();return true;}catch{return false;}},`CDP ${id}`);
  const browser=await chromium.connectOverCDP(endpoint);
  browsers.add(browser);
  return browser;
}
async function close(id,browser) {
  const cdp=await browser.newBrowserCDPSession();
  await cdp.send('Browser.close').catch(()=>{});
  browsers.delete(browser);
  await wait(async()=>(await cli('list')).split('\n').some(line=>line.startsWith(id+'\t') && line.endsWith('\tstopped')),`stop ${id}`);
  await assert.rejects(()=>cli('endpoint',id));
}
async function write(browser,value) {
  const page=await browser.contexts()[0].newPage(); await page.goto(`${origin}/${value}`);
  await page.evaluate(async value=>{
    localStorage.setItem('fixture',value);
    await new Promise((resolve,reject)=>{
      const request=indexedDB.open('fixture',1);
      request.onupgradeneeded=()=>request.result.createObjectStore('items');
      request.onerror=()=>reject(request.error);
      request.onsuccess=()=>{const tx=request.result.transaction('items','readwrite');tx.objectStore('items').put(value,'key');tx.oncomplete=()=>resolve();tx.onerror=()=>reject(tx.error);};
    });
  },value);
}
async function check(browser,value) {
  const page=await browser.contexts()[0].newPage(); await page.goto(origin);
  const state=await page.evaluate(async()=>({
    local:localStorage.getItem('fixture'), cookie:document.cookie,
    indexed:await new Promise((resolve,reject)=>{const req=indexedDB.open('fixture',1);req.onerror=()=>reject(req.error);req.onsuccess=()=>{const read=req.result.transaction('items').objectStore('items').get('key');read.onsuccess=()=>resolve(read.result);read.onerror=()=>reject(read.error);};})
  }));
  assert.equal(state.local,value); assert.equal(state.indexed,value); assert.equal(state.cookie,`fixture=${value}`);
}
try {
  const a=(await cli('create','profile-a')).split('\t')[0];
  const b=(await cli('create','profile-b')).split('\t')[0];
  const [first,second]=await Promise.all([open(a),open(b)]);
  await Promise.all([write(first,'a'),write(second,'b')]);
  await Promise.all([check(first,'a'),check(second,'b')]);
  await Promise.all([close(a,first),close(b,second)]);
  const [againA,againB]=await Promise.all([open(a),open(b)]);
  await Promise.all([check(againA,'a'),check(againB,'b')]);
  await Promise.all([close(a,againA),close(b,againB)]);
  console.log('PASS: parallel profiles, cookie/localStorage/IndexedDB isolation and restart');
} finally {
  for(const browser of browsers) {try {const cdp=await browser.newBrowserCDPSession();await cdp.send('Browser.close');}catch {}}
  site.close();
}
