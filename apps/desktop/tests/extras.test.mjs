import { test } from "node:test";
import assert from "node:assert/strict";
import { localTargets,extraCover,completedOnlineJobs,canApplyExtra } from "../src/lib/extras.ts";

test("GameFAQs saves may choose a card without a catalog serial or installed ROM, while unsupported formats and resources stay blocked",()=>{
  assert.equal(canApplyExtra({kind:"saves",serial:null,installable:true},[]),true);
  assert.equal(canApplyExtra({kind:"saves",serial:null,installable:false},[]),false);
  assert.equal(canApplyExtra({kind:"patches",serial:null,installable:true},[]),false);
  assert.equal(canApplyExtra({kind:"cheats",serial:"SLUS-21355",installable:true},[]),false);
  assert.equal(canApplyExtra({kind:"cheats",serial:"SLUS-21355",installable:true},[{serial:"SLUS-21355"}]),true);
});
test("extras remain downloadable with no installed games",()=>{
  const item={serial:"SLUS-21355"};
  assert.deepEqual(localTargets(item,[]),[]);
  assert.equal(item.serial,"SLUS-21355");
});
test("historical online jobs do not replay mutations at startup",()=>{
  assert.deepEqual(completedOnlineJobs(null,[{id:"old",kind:"extra",status:"succeeded"}]),[]);
});
test("background completion is detected across navigation and only once per state",()=>{
  const jobs=[{id:"new",kind:"extra",status:"succeeded"},{id:"catalog",kind:"catalog-refresh",status:"succeeded"},{id:"other",kind:"other",status:"succeeded"}];
  assert.deepEqual(completedOnlineJobs(new Map([["new","running"]]),jobs).map(j=>j.id),["new","catalog"]);
  assert.deepEqual(completedOnlineJobs(new Map(jobs.map(j=>[j.id,j.status])),jobs),[]);
});
test("extras use edition-specific covers and an encoded fallback without a serial",()=>{
  const representative="https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/SLUS-20946.jpg";
  assert.equal(extraCover({title:"GameFAQs title",serial:null,cover_url:representative}),representative);
  assert.ok(!extraCover({title:"Game",serial:null,cover_url:"https://untrusted.example/image"}).includes("untrusted.example"));
  assert.equal(extraCover({title:"Any title",serial:"SLUS-21355"}),"https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/SLUS-21355.jpg");
  const fallback=extraCover({title:"Game / Other",serial:null});assert.ok(fallback.includes("Game%20_%20Other"));assert.ok(fallback.startsWith("https://cdn.jsdelivr.net/"));
});
test("application targets match serials and never infer compatibility from titles",()=>{
  const installed=[{id:"wrong",title:"Same title",serial:"SLES-54321"},{id:"correct",title:"Modded title",serial:"SLUS-21355"}];
  assert.deepEqual(localTargets({serial:"SLUS-21355"},installed).map(g=>g.id),["correct"]);
  assert.deepEqual(localTargets({serial:null},installed),[]);
});
