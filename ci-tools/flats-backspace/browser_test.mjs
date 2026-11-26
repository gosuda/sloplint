'use strict';

import assert from "node:assert/strict";
import path from "node:path";
import { pathToFileURL } from "node:url";

const { chromium } = await import(pathToFileURL(process.env.FLATS_PLAYWRIGHT_MODULE).href);
const [url, evidence] = process.argv.slice(2);
const browser = await chromium.launch({ headless: true });
const contexts = [];
const errors = [];
const observations = [];
const input = "alpha\nbeta";
const expected = "alphabeta";

async function wait(check, message) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(message);
}

async function text(page) {
  return page.locator(".cm-content").evaluate((el) => el.cmTile.root.view.state.doc.toString());
}

async function open(name) {
  const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
  contexts.push(context);
  await context.addCookies([{ name: "flats-test-private", value: "1", url }]);
  await context.addInitScript((value) => localStorage.setItem("flats-docs-name", value), name);
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator(".cm-content").waitFor();
  await wait(async () => await page.locator("#status").innerText() === "Saved", "initial sync");
  return page;
}

try {
  const a = await open("Backspace writer");
  const b = await open("Backspace observer");
  for (const operation of ["keyboard", "native-delete"]) {
    await a.locator(".cm-content").focus();
    await a.keyboard.press("ControlOrMeta+a");
    await a.keyboard.insertText(input);
    await wait(async () => await text(a) === input && await text(b) === input, "two-line seed did not converge");
    await a.keyboard.press("ControlOrMeta+End");
    await a.keyboard.press("Home");
    const before = await a.locator(".cm-content").evaluate((el) => {
      const view = el.cmTile.root.view;
      const main = view.state.selection.main;
      window.backspaceEvents = [];
      el.addEventListener("input", (event) => window.backspaceEvents.push({
        inputType: event.inputType,
        isTrusted: event.isTrusted,
        data: event.data,
      }), { once: true });
      return { document: view.state.doc.toString(), anchor: main.anchor, head: main.head, html: el.innerHTML };
    });
    assert.equal(before.document, input);
    assert.equal(before.anchor, 6);
    assert.equal(before.head, 6);
    let executed = true;
    if (operation === "keyboard") {
      await a.keyboard.press("Backspace");
    } else {
      executed = await a.evaluate(() => document.execCommand("delete"));
    }
    assert.equal(executed, true, operation + " did not execute");
    await wait(async () => await text(a) === await text(b) &&
      await a.locator("#status").innerText() === "Saved" &&
      await b.locator("#status").innerText() === "Saved", "joined edit did not converge and save");
    const after = await a.locator(".cm-content").evaluate((el) => ({
      document: el.cmTile.root.view.state.doc.toString(),
      html: el.innerHTML,
      events: window.backspaceEvents,
    }));
    const remote = await text(b);
    const record = { operation, browser: browser.version(), before, after, remote, expected,
      passed: after.document === expected && remote === expected };
    observations.push(record);
    console.log("BACKSPACE_OBSERVATION " + JSON.stringify(record));
    if (!record.passed) await a.screenshot({ path: path.join(evidence, operation + "-failure.png") });
  }
  assert.deepEqual(errors, []);
  assert.equal(observations.length, 2);
  assert.ok(observations[1].after.events.some((event) =>
    event.isTrusted && event.inputType === "deleteContentBackward"), "native delete did not emit trusted backward input");
  console.log("BACKSPACE_PROBE_COMPLETE " + JSON.stringify(observations.map(({ operation, passed }) => ({ operation, passed }))));
  assert.ok(observations.every((record) => record.passed), "a joined line retained an unexpected trailing newline");
} finally {
  for (const context of contexts) await context.close();
  await browser.close();
}
