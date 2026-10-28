import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import http from 'node:http';
import {spawn, execFileSync} from 'node:child_process';
import {once} from 'node:events';
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';

const repo = process.env.OPENKEYLET_REPO;
const fixturePath = process.env.OPENKEYLET_FIXTURE;
const chrome = process.env.KEYHARBOR_TEST_CHROME;
const kh = process.env.KEYHARBOR_BINARY;
const pipeInstall = process.env.OPENKEYLET_TEST_PIPE_INSTALL === '1';
assert.ok(repo && fixturePath && chrome && kh, 'Set OPENKEYLET_REPO, OPENKEYLET_FIXTURE, KEYHARBOR_TEST_CHROME, and KEYHARBOR_BINARY');
const {chromium} = await import(pathToFileURL(path.join(repo, 'node_modules/playwright/index.mjs')).href);

const root = await fs.mkdtemp(path.join(os.tmpdir(), 'openkeylet-browser-test-'));
const extension = path.join(root, 'extension');
const khroot = path.join(root, 'kh');
const server = http.createServer((req, res) => {res.setHeader('Content-Type','text/html'); res.end('<form><input id="username" autocomplete="username"><input id="password" type="password" autocomplete="current-password"></form>');});
server.listen(0,'127.0.0.1'); await once(server,'listening');
const origin=`http://localhost:${server.address().port}`;
const probe=net.createServer(); probe.listen(0,'127.0.0.1'); await once(probe,'listening'); const port=probe.address().port; await new Promise(r=>probe.close(r));
await fs.cp(path.join(repo,'browser-extension'),extension,{recursive:true});
const sw=path.join(extension,'service-worker.js'); await fs.writeFile(sw,(await fs.readFile(sw,'utf8')).replaceAll('39272',String(port)));
const fixture=spawn(fixturePath,[root,String(port),origin],{stdio:['ignore','pipe','pipe']});
let messages=''; fixture.stdout.on('data',d=>messages+=d); fixture.stderr.on('data',d=>messages+=d);
const run=(...args)=>execFileSync(kh,['--data-dir',khroot,...args],{encoding:'utf8'});
let browser;
async function wait(fn,why) {let until=Date.now()+20000;while(Date.now()<until){if(await fn())return; await new Promise(r=>setTimeout(r,100));}throw new Error('Timeout '+why+' '+messages.slice(-1000));}
try {
 await wait(()=>messages.includes('READY'),'fixture');
 const id=run('create','integration').split('\t')[0];
 run('bind-account',id,'Fixture login');
 assert.equal(run('account',id).trim(),'openkeylet\tFixture login');
 const wrapper=path.join(root,'browser');
 if(pipeInstall) {
  const script=`#!/usr/bin/env node\nconst {spawn}=require('node:child_process');const fs=require('node:fs');\nconst child=spawn(${JSON.stringify(chrome)},['--headless','--disable-gpu','--remote-debugging-port=0','--remote-debugging-pipe','--enable-unsafe-extension-debugging',...process.argv.slice(2)],{stdio:['ignore','ignore','ignore','pipe','pipe']});\nlet buf='';child.stdio[4].on('data',d=>{buf+=d;let i;while((i=buf.indexOf('\\0'))>=0){let m=JSON.parse(buf.slice(0,i));buf=buf.slice(i+1);if(m.id===1){if(m.error)child.kill();else fs.writeFileSync(${JSON.stringify(path.join(root,'extension-loaded'))},m.result.id)}}});\nchild.stdio[3].write(JSON.stringify({id:1,method:'Extensions.loadUnpacked',params:{path:${JSON.stringify(extension)}}})+'\\0');child.on('exit',code=>process.exit(code??1));\n`;
  await fs.writeFile(wrapper,script,{mode:0o700});
 } else {
  await fs.writeFile(wrapper,`#!/bin/sh\nexec "${chrome}" --headless --disable-gpu --remote-debugging-port=0 --disable-extensions-except="${extension}" --load-extension="${extension}" "$@"\n`,{mode:0o700});
 }
 run('open',id,'--browser',wrapper);
 if(pipeInstall) {await wait(async()=>fs.access(path.join(root,'extension-loaded')).then(()=>true,()=>false),'pipe extension load');assert.equal(await fs.readFile(path.join(root,'extension-loaded'),'utf8'),'knkjojeijgalhpdkfapohofobfomenmi');}
 const portfile=path.join(khroot,'profiles',id,'DevToolsActivePort');
 await wait(async()=>fs.access(portfile).then(()=>true,()=>false),'CDP port');
 const debugPort=(await fs.readFile(portfile,'utf8')).split('\n')[0];
 browser=await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
 const context=browser.contexts()[0];
 await wait(()=>context.serviceWorkers().some(w=>w.url().startsWith('chrome-extension://knkjojeijgalhpdkfapohofobfomenmi/')),'extension worker');
 let worker=context.serviceWorkers().find(w=>w.url().startsWith('chrome-extension://knkjojeijgalhpdkfapohofobfomenmi/'));
 const site=await context.newPage(); await site.goto(origin); await site.bringToFront();
 const requestId=run('wait-auth',id).trim().split('\t')[2];
 assert.equal(run('account',id).trim(),'openkeylet\tFixture login');
 const extensionId=new URL(worker.url()).host;
 const popup=await context.newPage(); await popup.goto(`chrome-extension://${extensionId}/popup.html`);
 await wait(async()=>(await popup.locator('#status').textContent()).includes('CONNECTED'),'pair');
 await popup.evaluate(()=>refresh());
 await popup.locator('#pin').fill('123456'); await site.bringToFront(); await popup.locator('#unlock-form button').click();
 await popup.getByRole('button',{name:/Fixture login/}).waitFor();
 await site.bringToFront(); await popup.getByRole('button',{name:/Fixture login/}).click();
 await wait(async()=>await site.locator('#password').inputValue()==='fixture-initial-password','fill');
 run('resume',id,requestId);
 assert.equal(run('list').includes('\trunning'),true);
 for (const name of ['keyharbor.sqlite3', 'keyharbor.sqlite3-wal', 'keyharbor.sqlite3-shm']) {
   const data=await fs.readFile(path.join(khroot,name)).catch(error=>{if(error.code==='ENOENT')return Buffer.alloc(0);throw error});
   assert.equal(data.includes('fixture-initial-password'),false);
 }
 console.log('PASS: account reference, auth handoff, extension pairing, popup fill, SQLite files plaintext check');
 const cdp=await browser.newBrowserCDPSession(); await cdp.send('Browser.close').catch(()=>{});
 browser=null;
 await wait(()=>run('list').includes('\tstopped'),'browser close');
} finally {
 if(browser){try{const cdp=await browser.newBrowserCDPSession();await cdp.send('Browser.close');}catch{}}
 await fs.writeFile(path.join(root,'stop'),'');
 if(fixture.exitCode===null) await Promise.race([once(fixture,'exit'),new Promise(r=>setTimeout(r,3000))]);
 server.close();
}
