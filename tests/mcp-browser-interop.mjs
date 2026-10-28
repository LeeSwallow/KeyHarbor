// Local synthetic MCP/CDP integration; no user credentials or profiles.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import {spawn, execFile} from 'node:child_process';
import {once} from 'node:events';
import {createInterface} from 'node:readline';
import {pathToFileURL} from 'node:url';
import {promisify} from 'node:util';

const {KEYHARBOR_TEST_PLAYWRIGHT, KEYHARBOR_TEST_CHROME, KEYHARBOR_BINARY} = process.env;
assert.ok(KEYHARBOR_TEST_PLAYWRIGHT && KEYHARBOR_TEST_CHROME && KEYHARBOR_BINARY, 'Set KEYHARBOR_TEST_PLAYWRIGHT, KEYHARBOR_TEST_CHROME and KEYHARBOR_BINARY');
const {chromium} = await import(pathToFileURL(KEYHARBOR_TEST_PLAYWRIGHT).href);
const root = await fs.mkdtemp(path.join(os.tmpdir(), 'keyharbor-mcp-browser-test-'));
const wrapper=path.join(root,'browser');
const quote=value=>`'${value.replaceAll("'", "'\\''")}'`;
await fs.writeFile(wrapper, `#!/bin/sh\nfor arg in "$@"; do [ "$arg" = '--remote-debugging-port=0' ] && exec ${quote(KEYHARBOR_TEST_CHROME)} --headless --disable-gpu "$@"; done\n${quote(KEYHARBOR_TEST_CHROME)} --headless --disable-gpu --dump-dom "$@" >/dev/null 2>/dev/null\nstatus=$?\nsleep 1\nexit "$status"\n`, {mode:0o700});
const cookie='kh-synthetic-session-987654321';
const site=http.createServer((req,res)=>{
  if(req.url==='/login')res.setHeader('Set-Cookie',`session=${cookie}; Max-Age=3600; HttpOnly; SameSite=Lax; Path=/`);
  res.setHeader('Content-Type','text/plain');
  res.end(req.url==='/protected' && req.headers.cookie?.includes(`session=${cookie}`) ? 'authenticated' : 'fixture');
});
site.listen(0,'127.0.0.1');await once(site,'listening');
const origin=`http://127.0.0.1:${site.address().port}`;
const server=spawn(KEYHARBOR_BINARY,['--data-dir',path.join(root,'data'),'mcp','--allow-cdp','--browser',wrapper],{stdio:['pipe','pipe','pipe']});
let received='';const pending=new Map();let counter=0;
const lines=createInterface({input:server.stdout});
lines.on('line',line=>{received+=line+'\n';const msg=JSON.parse(line);if(msg.id!==undefined && pending.has(msg.id)){pending.get(msg.id)(msg);pending.delete(msg.id);}});
function request(method,params={}) {
  const id=++counter;
  return new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`MCP timeout: ${method}`));},15000);
    pending.set(id,value=>{clearTimeout(timer);resolve(value);});
    server.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');
  });
}
async function call(name,args={}) {const response=await request('tools/call',{name,arguments:args});assert.ok(response.result,JSON.stringify(response));return response.result;}
async function wait(fn,why) {const end=Date.now()+15000;while(Date.now()<end){const value=await fn();if(value)return value;await new Promise(r=>setTimeout(r,100));}throw new Error(`timeout: ${why}`);}
let browser;
try {
  const initialized=await request('initialize',{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'integration-test',version:'1'}});
  assert.ok(initialized.result);
  server.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
  const created=await call('session_create',{name:'agent-flow'});
  const id=created.content[0].text.split('\t')[0];
  assert.notEqual((await call('session_open',{id,url:origin+'/',cdp:true})).isError,true);
  const endpoint=await wait(async()=>{const r=await call('session_endpoint',{id});return r.isError?null:r.content[0].text;},'endpoint');
  browser=await chromium.connectOverCDP(endpoint);
  const page=await browser.contexts()[0].newPage();
  const handoff=await call('session_wait_auth',{id,origin});assert.notEqual(handoff.isError,true);
  const request_id=handoff.content[0].text.split('\t')[2];
  const inspected=await promisify(execFile)(KEYHARBOR_BINARY,['--data-dir',path.join(root,'data'),'handoff',id]);
  assert.equal(inspected.stdout.trim().split('\t')[3],origin);
  const resumed=call('session_wait_for_resume',{id,request_id,timeout_seconds:10});
  assert.equal((await call('session_endpoint',{id})).isError,true);
  // A pre-existing CDP connection still works during handoff; this is not a privacy boundary.
  await page.goto(origin+'/login');
  await promisify(execFile)(KEYHARBOR_BINARY,['--data-dir',path.join(root,'data'),'resume',id,request_id]);
  assert.notEqual((await resumed).isError,true);
  assert.equal(await (await page.goto(origin+'/protected')).text(),'authenticated');
  assert.equal(received.includes(cookie),false);
  for(const file of ['keyharbor.sqlite3','keyharbor.sqlite3-wal','keyharbor.sqlite3-shm']){
    const bytes=await fs.readFile(path.join(root,'data',file)).catch(err=>{if(err.code==='ENOENT')return Buffer.alloc(0);throw err;});
    assert.equal(bytes.includes(cookie),false);
  }
  const cdp=await browser.newBrowserCDPSession();await cdp.send('Browser.close').catch(()=>{});browser=null;
  await wait(async()=>{const r=await call('sessions_list');return r.content[0].text.includes('\tstopped');},'stop');
  const second=await call('session_create',{name:'no-cdp-login'});
  const secondId=second.content[0].text.split('\t')[0];
  assert.notEqual((await call('session_open',{id:secondId,url:origin+'/login'})).isError,true);
  assert.equal((await call('session_endpoint',{id:secondId})).isError,true);
  await wait(async()=>{const r=await call('sessions_list');return r.content[0].text.split('\n').some(line=>line.startsWith(secondId+'\t') && line.endsWith('\tstopped'));},'no-CDP browser stop');
  assert.notEqual((await call('session_open',{id:secondId,cdp:true})).isError,true);
  const secondEndpoint=await wait(async()=>{const r=await call('session_endpoint',{id:secondId});return r.isError?null:r.content[0].text;},'second endpoint');
  browser=await chromium.connectOverCDP(secondEndpoint);
  const reopened=await browser.contexts()[0].newPage();
  assert.equal(await (await reopened.goto(origin+'/protected')).text(),'authenticated');
  const secondCdp=await browser.newBrowserCDPSession();await secondCdp.send('Browser.close').catch(()=>{});browser=null;
  await wait(async()=>{const r=await call('sessions_list');return r.content[0].text.split('\n').some(line=>line.startsWith(secondId+'\t') && line.endsWith('\tstopped'));},'second stop');
  console.log('PASS: MCP CDP handoff and restart from a non-CDP synthetic login');
} finally {
  if(browser){try{const cdp=await browser.newBrowserCDPSession();await cdp.send('Browser.close');}catch{}}
  server.stdin.end();
  await Promise.race([once(server,'exit'),new Promise(r=>setTimeout(r,3000))]);
  site.close();
}
