import assert from "node:assert/strict";
import { test } from "node:test";

import { createStillFollower, shouldReportStillFailure } from "./still-follow.ts";

function setup() {
  const timers: Array<{ run: () => void; cancelled: boolean }> = [];
  const asked: Array<{ id: number; startS: number; resolve: (path: string) => void; reject: (failure: unknown) => void }> = [];
  const applied: string[] = [];
  const failed: unknown[] = [];
  const follower = createStillFollower({
    delayMs: 250,
    schedule: (run) => {
      const timer = { run, cancelled: false };
      timers.push(timer);
      return timer;
    },
    cancel: (handle) => {
      (handle as { cancelled: boolean }).cancelled = true;
    },
    ask: (id, startS) =>
      new Promise<string>((resolve, reject) => {
        asked.push({ id, startS, resolve, reject });
      }),
    apply: (path) => applied.push(path),
    fail: (failure) => failed.push(failure),
  });
  const fire = () => timers.filter((timer) => !timer.cancelled).forEach((timer) => ((timer.cancelled = true), timer.run()));
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  return { follower, asked, applied, failed, fire, settle };
}

test("dragging fires one request, for the last start only", () => {
  const { follower, asked, fire } = setup();
  follower.follow(1, 0.5);
  follower.follow(1, 0.8);
  follower.follow(1, 1.2);
  fire();
  assert.deepEqual(asked.map((request) => request.startS), [1.2]);
});

test("the same start is not asked for twice", () => {
  const { follower, asked, fire } = setup();
  follower.follow(1, 0.5);
  fire();
  follower.follow(1, 0.5);
  fire();
  assert.equal(asked.length, 1);
});

test("an old answer is dropped when a newer start was asked for", async () => {
  const { follower, asked, applied, fire, settle } = setup();
  follower.follow(1, 0.5);
  fire();
  follower.follow(1, 2);
  fire();
  asked[1]?.resolve("new");
  asked[0]?.resolve("old");
  await settle();
  assert.deepEqual(applied, ["new"]);
});

test("an answer from before reset (another session) is dropped", async () => {
  const { follower, asked, applied, failed, fire, settle } = setup();
  follower.follow(1, 0.5);
  fire();
  follower.reset();
  asked[0]?.resolve("late");
  await settle();
  follower.follow(2, 0.5);
  fire();
  asked[1]?.reject(new Error("no"));
  follower.reset();
  await settle();
  assert.deepEqual(applied, []);
  assert.deepEqual(failed, []);
});

test("reset cancels the pending request and lets the same start be asked again", () => {
  const { follower, asked, fire } = setup();
  follower.follow(1, 0.5);
  follower.reset();
  fire();
  assert.equal(asked.length, 0);
  follower.follow(1, 0.5);
  fire();
  assert.equal(asked.length, 1);
});

test("a failure is reported and the same start is retried on the next follow", async () => {
  const { follower, asked, applied, failed, fire, settle } = setup();
  follower.follow(1, 0.5);
  fire();
  asked[0]?.reject(new Error("boom"));
  await settle();
  assert.equal(failed.length, 1);
  follower.follow(1, 0.5);
  fire();
  asked[1]?.resolve("ok");
  await settle();
  assert.deepEqual(applied, ["ok"]);
});

test("a failure of an outdated turn is neither reported nor does it block the current one", async () => {
  const { follower, asked, applied, failed, fire, settle } = setup();
  follower.follow(1, 0.5);
  fire();
  follower.follow(1, 1);
  fire();
  asked[0]?.reject(new Error("old"));
  asked[1]?.resolve("current");
  await settle();
  assert.deepEqual(failed, []);
  assert.deepEqual(applied, ["current"]);
  follower.follow(1, 1);
  fire();
  assert.equal(asked.length, 2);
});

test("a cancellation error is not shown to the user, other errors are", () => {
  assert.equal(shouldReportStillFailure({ kind: "cancelled", message: "cancelado" }), false);
  assert.equal(shouldReportStillFailure({ kind: "other", message: "falló" }), true);
  assert.equal(shouldReportStillFailure("rubbish"), true);
});
