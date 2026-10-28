// Uses two temporary KeyHarbor profiles and a synthetic OpenKeylet vault.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import http from 'node:http';
import {spawn, execFile} from 'node:child_process';
import {once} from 'node:events';
import {promisify} from 'node:util';
import {createInterface} from 'node:readline';
import {pathToFileURL} from 'node:url';

const {OPENKEYLET_REPO, OPENKEYLET_FIXTURE, OPENKEYLET_BINARY, KEYHARBOR_TEST_CHROME, KEYHARBOR_BINARY} = process.env;
assert.ok(OPENKEYLET_REPO && OPENKEYLET_FIXTURE && OPENKEYLET_BINARY && KEYHARBOR_TEST_CHROME && KEYHARBOR_BINARY, 'Set test binary and repository paths');
const {chromium}=await import(pathToFileURL(path.join(OPENKEYLET_REPO,'node_modules/playwright/index.mjs')).href);
const exec=promisify(execFile), root=await fs.mkdtemp(path.join(os.tmpdir(),'openkeylet-browser-test-'));
const data=path.join(root,'kh'), extension=path.join(root,'extension'), vault=path.join(root,'vault');
const quote=v=>`'${v.replaceAll("'", "'\\''")}'`;
const site=http.createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end('<form><input id="password" type="password" autocomplete="current-password"></form>');});
site.listen(0,'127.0.0.1');await once(site,'listening');
const origin=`http://localhost:${site.address().port}`;
const probe=net.createServer();probe.listen(0,'127.0.0.1');await once(probe,'listening');const port=probe.address().port;await new Promise(r=>probe.close(r));
await fs.cp(path.join(OPENKEYLET_REPO,'browser-extension'),extension,{recursive:true});
const sw=path.join(extension,'service-worker.js');await fs.writeFile(sw,(await fs.readFile(sw,'utf8')).replaceAll('39272',String(port)));
const wrapper=path.join(root,'browser');await fs.writeFile(wrapper,`#!/bin/sh\nexec ${quote(KEYHARBOR_TEST_CHROME)} --headless --disable-gpu --disable-extensions-except=${quote(extension)} --load-extension=${quote(extension)} "$@"\n`,{mode:0o700});
const fixture=spawn(OPENKEYLET_FIXTURE,[root,String(port),origin],{stdio:['ignore','pipe','pipe']});
let fixtureOutput='';fixture.stdout.on('data',d=>fixtureOutput+=d);fixture.stderr.on('data',d=>fixtureOutput+=d);
const cli=async(...args)=>(await exec(KEYHARBOR_BINARY,['--data-dir',data,...args])).stdout;
async function wait(fn,why){const end=Date.now()+20000;while(Date.now()<end){const value=await fn();if(value)return value;await new Promise(r=>setTimeout(r,100));}throw new Error(`timeout: ${why}; ${fixtureOutput.slice(-500)}`);}
let mcp, input, readers, requestId=0;const pending=new Map(), browsers=[];
function request(method,params={}){const id=++requestId;return new Promise((resolve,reject)=>{const t=setTimeout(()=>{pending.delete(id);reject(new Error('MCP timeout'));},15000);pending.set(id,v=>{clearTimeout(t);resolve(v);});input.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});}
async function open(name){const id=(await cli('create',name)).split('\t')[0];await cli('open',id,'--browser',wrapper,'--cdp');const endpoint=await wait(async()=>{try{return(await cli('endpoint',id)).trim();}catch{return null;}},'endpoint');const browser=await chromium.connectOverCDP(endpoint);browsers.push(browser);const context=browser.contexts()[0];const worker=await wait(()=>context.serviceWorkers().find(w=>w.url().startsWith('chrome-extension://knkjojeijgalhpdkfapohofobfomenmi/')),'worker');await wait(async()=>await worker.evaluate(async()=>(await chrome.storage.local.get('bridgeStatus')).bridgeStatus==='connected'),'pair');const page=await context.newPage();await page.goto(origin);return {page,worker};}
async function fill(target){await target.page.bringToFront();await target.page.locator('#password').focus();await target.worker.evaluate(()=>announceActive());await new Promise(r=>setTimeout(r,250));const result=await request('tools/call',{name:'credential_fill',arguments:{item:'Fixture login',module:'password',target:'browser'}});assert.notEqual(result.result?.isError,true,JSON.stringify(result));return result;}
try {
  await wait(()=>fixtureOutput.includes('READY'),'fixture');
  const a=await open('profile-a');
  await exec(OPENKEYLET_BINARY,['browser','pair',String(port)],{env:{...process.env,OPENKEYLET_DATA_DIR:vault}});
  const b=await open('profile-b');
  mcp=spawn(OPENKEYLET_BINARY,['mcp','stdio'],{env:{...process.env,OPENKEYLET_DATA_DIR:vault},stdio:['pipe','pipe','pipe']});input=mcp.stdin;
  readers=createInterface({input:mcp.stdout});readers.on('line',line=>{const msg=JSON.parse(line);if(pending.has(msg.id)){pending.get(msg.id)(msg);pending.delete(msg.id);}});
  const init=await request('initialize',{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'keyharbor-test',version:'1'}});assert.ok(init.result);
  input.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  const first=await fill(a);await wait(async()=>await a.page.locator('#password').inputValue()==='fixture-initial-password','fill A');
  assert.equal(await b.page.locator('#password').inputValue(),'');
  await a.page.locator('#password').fill('');
  const second=await fill(b);await wait(async()=>await b.page.locator('#password').inputValue()==='fixture-initial-password','fill B');
  assert.equal(await a.page.locator('#password').inputValue(),'');
  assert.equal(JSON.stringify([first,second]).includes('fixture-initial-password'),false);
  console.log('PASS: OpenKeylet MCP fill follows active extension across two KeyHarbor profiles');
} finally {
  for(const browser of browsers){try{const cdp=await browser.newBrowserCDPSession();await cdp.send('Browser.close');}catch{}}
  if(mcp){input.end();await Promise.race([once(mcp,'exit'),new Promise(r=>setTimeout(r,3000))]);}
  await fs.writeFile(path.join(root,'stop'),'');if(fixture.exitCode===null)await Promise.race([once(fixture,'exit'),new Promise(r=>setTimeout(r,3000))]);site.close();
}
