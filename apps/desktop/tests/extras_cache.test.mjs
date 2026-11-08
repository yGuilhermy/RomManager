import { test } from "node:test";
import assert from "node:assert/strict";
import { buildSync } from "esbuild";
import { fileURLToPath } from "node:url";

let sequence=0;
async function moduleFor(name) {
  const js=buildSync({entryPoints:[fileURLToPath(new URL(`../src/lib/${name}.ts`,import.meta.url))],bundle:true,format:"esm",target:"es2022",write:false}).outputFiles[0].text;
  return import(`data:text/javascript;base64,${Buffer.from(js).toString("base64")}#${++sequence}`);
}
function storage() {
  const data=new Map();
  return {getItem:key=>data.get(key)??null,setItem:(key,value)=>data.set(key,value)};
}
test("extras lists deduplicate reads and restore synchronously after a page reload without fetching",async()=>{
  const previous=globalThis.sessionStorage;globalThis.sessionStorage=storage();
  try {
    const cache=await moduleFor("extrasCache");let reads=0;
    const path="/api/extras/items?kind=saves&key=GFQ%3Agame";
    const fetcher=async()=>{reads++;return {version:"2023",items:[{id:"save"}]};};
    const [a,b]=await Promise.all([cache.loadExtras(path,fetcher),cache.loadExtras(path,fetcher)]);
    assert.deepEqual(a,b);assert.equal(reads,1);assert.deepEqual(cache.peekExtras(path),a);
    const reloaded=await moduleFor("extrasCache");assert.deepEqual(reloaded.peekExtras(path),a);
    assert.deepEqual(await reloaded.loadExtras(path,fetcher),a);assert.equal(reads,1);
  } finally {globalThis.sessionStorage=previous;}
});
test("failed explicit updates preserve saved lists and successful updates replace them",async()=>{
  const previous=globalThis.sessionStorage;globalThis.sessionStorage=storage();
  try {
    const cache=await moduleFor("extrasCache");const path="/api/extras/games?kind=saves&source=gamefaqs";
    const before={version:"2023",games:[{key:"old"}]};await cache.loadExtras(path,async()=>before);
    await assert.rejects(cache.loadExtras(path,async()=>{throw new Error("offline");},true),/offline/);
    assert.deepEqual(cache.peekExtras(path),before);
    assert.deepEqual(await cache.loadExtras(path,async()=>{throw new Error("must not fetch");}),before);
    const after={version:"2023",games:[{key:"new"}]};await cache.loadExtras(path,async()=>after,true);
    assert.deepEqual(cache.peekExtras(path),after);
  } finally {globalThis.sessionStorage=previous;}
});
test("extras use the source-read timeout and translate timeout errors without retry storms",async()=>{
  const previousFetch=globalThis.fetch,previousTimeout=AbortSignal.timeout;let chosenTimeout=0,requests=0;
  try {
    AbortSignal.timeout=milliseconds=>{chosenTimeout=milliseconds;return new AbortController().signal;};
    globalThis.fetch=async()=>{requests++;throw new DOMException("signal timed out","TimeoutError");};
    const {apiFetch}=await moduleFor("api");
    await assert.rejects(apiFetch("/api/extras/items?kind=saves&key=GFQ%3Agame"),/A fonte demorou/);
    assert.equal(chosenTimeout,180000);assert.equal(requests,1);
  } finally {globalThis.fetch=previousFetch;AbortSignal.timeout=previousTimeout;}
});
