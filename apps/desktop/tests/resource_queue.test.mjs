import { test } from "node:test";
import assert from "node:assert/strict";
import { ResourceQueue } from "../src/lib/resourceQueue.ts";

test("queued resource changes use the revision returned by the previous change", async()=>{
  const queue=new ResourceQueue();let release;let revision=1;const seen=[];
  const wait=new Promise(resolve=>{release=resolve;});
  const first=queue.run(async()=>{seen.push(revision);await wait;revision=2;});
  const second=queue.run(async()=>{seen.push(revision);revision=3;});
  await Promise.resolve();assert.deepEqual(seen,[1]);release();await Promise.all([first,second]);
  assert.deepEqual(seen,[1,2]);assert.equal(revision,3);
});
test("a failed resource change does not strand later actions",async()=>{
  const queue=new ResourceQueue();
  const first=queue.run(async()=>{throw new Error("Denied");});
  const second=queue.run(async()=>"next action finished");
  await assert.rejects(first,/Denied/);assert.equal(await second,"next action finished");
});
