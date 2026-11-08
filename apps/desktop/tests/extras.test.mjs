import { test } from "node:test";
import assert from "node:assert/strict";
import { localTargets,extraCover,completedOnlineJobs,canApplyExtra,groupSaveItems } from "../src/lib/extras.ts";

test("GameFAQs saves are grouped by format and region without losing entries",()=>{
  const save=(id,format,region)=>({id,title:`Save #${id} · ${format} · ${region}`,game_key:"GFQ:game",serial:null,source_url:"https://archive.org/details/gamefaqs_savegames"});
  const items=[save("1","CodeBreaker","Europa"),save("2","Max Drive","América do Norte"),save("3","Max Drive","Europa"),save("4","PSV / ZIP","Japão")];
  const groups=groupSaveItems(items);
  assert.deepEqual(groups.map(g=>g.key),["max","zip","cbs"]);
  assert.deepEqual(groups[0].regions.map(r=>r.name),["América do Norte","Europa"]);
  assert.deepEqual(groups.flatMap(g=>g.regions.flatMap(r=>r.items)).map(i=>i.id).sort(),["1","2","3","4"]);
});

test("Apollo saves use the delivered PSU format and keep unknown regions explicit",()=>{
  const items=[{id:"a",title:"Game · 0001",game_key:"SLUS-21488",serial:"SLUS-21488",source_url:"https://github.com/bucanero/apollo-saves"},
    {id:"b",title:"Game · 0002",game_key:"UNKNOWN",serial:null,source_url:"https://github.com/bucanero/apollo-saves"}];
  const groups=groupSaveItems(items);
  assert.equal(groups.length,1);assert.equal(groups[0].key,"psu");
  assert.deepEqual(groups[0].regions.map(r=>r.name),["América do Norte","Região não informada"]);
});

test("unknown save formats stay available in their own section",()=>{
  const groups=groupSaveItems([{id:"x",title:"Unrecognized save",game_key:"GFQ:game",serial:null,source_url:"https://archive.org/details/gamefaqs_savegames"}]);
  assert.equal(groups[0].key,"other");assert.equal(groups[0].regions[0].name,"Região não informada");assert.equal(groups[0].items[0].id,"x");
  assert.deepEqual(groupSaveItems([]),[]);
});

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
