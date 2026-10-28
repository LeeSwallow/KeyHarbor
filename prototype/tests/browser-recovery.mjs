// Kills only a verified supervisor created by this test, using synthetic data.
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
const exec=promisify(execFile);
const root=await fs.mkdtemp(path.join(os.tmpdir(),'keyharbor-crash-test-'));
const data=path.join(root,'data'), wrapper=path.join(root,'browser'), pidfile=path.join(root,'supervisor-pid');
const quote=value=>`'${value.replaceAll("'", "'\\''")}'`;
await fs.writeFile(wrapper,`#!/bin/sh\nprintf '%s %s' "$PPID" "$$" > ${quote(pidfile)}\nexec ${quote(KEYHARBOR_TEST_CHROME)} --headless --disable-gpu "$@"\n`,{mode:0o700});
const site=http.createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end('test');});
site.listen(0,'127.0.0.1');await once(site,'listening');
const origin=`http://127.0.0.1:${site.address().port}`;
const cli=async(...args)=>(await exec(KEYHARBOR_BINARY,['--data-dir',data,...args])).stdout;
async function wait(fn,why){const end=Date.now()+20000;while(Date.now()<end){const value=await fn();if(value)return value;await new Promise(r=>setTimeout(r,100));}throw new Error(`timeout: ${why}`);}
async function endpoint(id){return wait(async()=>{try{return (await cli('endpoint',id)).trim();}catch{return null;}},'endpoint');}
async function close(browser){const cdp=await browser.newBrowserCDPSession();await cdp.send('Browser.close').catch(()=>{});}
let browser;
try {
  const id=(await cli('create','crash-fixture')).split('\t')[0];
  await cli('open',id,'--browser',wrapper,'--cdp');
  browser=await chromium.connectOverCDP(await endpoint(id));
  const page=await browser.contexts()[0].newPage();await page.goto(origin);
  await page.evaluate(()=>localStorage.setItem('fixture','survives'));
  await wait(()=>fs.access(pidfile).then(()=>true,()=>false),'supervisor pid');
  const [pid,browserPid]=(await fs.readFile(pidfile,'utf8')).split(' ').map(Number);
  assert.ok(Number.isInteger(pid) && pid>1);
  const command=(await exec('ps',['-p',String(pid),'-o','command='])).stdout;
  assert.ok(command.includes('supervise') && command.includes(id) && command.includes(data),'unexpected process identity');
  process.kill(pid,'SIGKILL');
  await wait(async()=>{try{await cli('endpoint',id);return false;}catch{return true;}},'supervisor lock release');
  assert.ok((await cli('list')).includes('\trecovery_required'));
  await assert.rejects(()=>cli('wait-auth',id));
  await assert.rejects(()=>cli('open',id,'--browser',wrapper,'--cdp'));
  await assert.rejects(()=>cli('recover',id));
  assert.equal(await page.evaluate(()=>localStorage.getItem('fixture')),'survives');
  await close(browser);browser=null;
  await wait(()=>{try{process.kill(browserPid,0);return false;}catch(err){if(err.code==='ESRCH')return true;throw err;}},'browser process exit');
  // The real user must confirm browser termination before using this switch.
  await cli('recover',id,'--confirm-browser-closed');
  await cli('open',id,'--browser',wrapper,'--cdp');
  browser=await chromium.connectOverCDP(await endpoint(id));
  const again=await browser.contexts()[0].newPage();await again.goto(origin);
  assert.equal(await again.evaluate(()=>localStorage.getItem('fixture')),'survives');
  await close(browser);browser=null;
  await wait(async()=>(await cli('list')).includes('\tstopped'),'normal stop');
  console.log('PASS: supervisor crash, blocked reuse, manual recovery, persistent profile');
} finally {
  if(browser){try{await close(browser);}catch{}}
  site.close();
}
