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
const cases = [
  { name: "last-line", input: "alpha\nbeta", caret: 6, expected: "alphabeta" },
  { name: "middle-line", input: "alpha\nbeta\ngamma", caret: 6, expected: "alphabeta\ngamma" },
  { name: "retained-final-newline", input: "alpha\nbeta\n", caret: 6, expected: "alphabeta\n" },
  { name: "empty-first-line", input: "\nbeta", caret: 1, expected: "beta" },
  { name: "empty-final-line", input: "alpha\n", caret: 6, expected: "alpha" },
  { name: "empty-middle-line", input: "alpha\n\nbeta", caret: 7, expected: "alpha\nbeta" },
  { name: "character-at-end", input: "alphabeta", caret: 9, expected: "alphabet" },
  { name: "character-within-line", input: "alpha\nbeta", caret: 8, expected: "alpha\nbta" },
  { name: "unicode-line", input: "alpha\nβeta", caret: 6, expected: "alphaβeta" },
];

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
  page.on("pageerror", (error) => { errors.push(error.message); console.log("BROWSER_PAGE_ERROR " + error.message); });
  await page.goto(url);
  await page.locator(".cm-content").waitFor();
  await wait(async () => await page.locator("#status").innerText() === "Saved", "initial sync");
  return page;
}

try {
  const a = await open("Backspace writer");
  const b = await open("Backspace observer");
  for (const { name, input, caret, expected } of cases) {
  for (const operation of ["keyboard", "native-delete"]) {
    await a.locator(".cm-content").evaluate((el, { input, caret }) => {
      const view = el.cmTile.root.view;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: input },
        selection: { anchor: caret },
      });
      view.focus();
    }, { input, caret });
    await wait(async () => await text(a) === input && await text(b) === input &&
      await a.locator("#status").innerText() === "Saved" &&
      await b.locator("#status").innerText() === "Saved", "seed did not converge and save");
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
    assert.equal(before.anchor, caret);
    assert.equal(before.head, caret);
    let executed = true;
    if (operation === "keyboard") {
      await a.keyboard.press("Backspace");
    } else {
      executed = await a.evaluate(() => document.execCommand("delete"));
    }
    assert.equal(executed, true, operation + " did not execute");
    let settleError = "";
    try {
      await wait(async () => await text(a) !== input && await text(a) === await text(b) &&
        await a.locator("#status").innerText() === "Saved" &&
        await b.locator("#status").innerText() === "Saved", "joined edit did not converge and save");
    } catch (error) {
      if (!(error instanceof Error)) throw error;
      settleError = error.message;
    }
    const after = await a.locator(".cm-content").evaluate((el) => ({
      document: el.cmTile.root.view.state.doc.toString(),
      html: el.innerHTML,
      events: window.backspaceEvents,
    }));
    const remote = await text(b);
    const record = { name, operation, settleError, browser: browser.version(), before, after, remote, expected,
      passed: !settleError && after.document === expected && remote === expected };
    observations.push(record);
    console.log("BACKSPACE_OBSERVATION " + JSON.stringify(record));
    if (!record.passed) await a.screenshot({ path: path.join(evidence, name + "-" + operation + "-failure.png") });
  }
  }
  assert.deepEqual(errors, []);
  assert.equal(observations.length, cases.length * 2);
  assert.ok(observations.filter((record) => record.operation === "native-delete").every((record) => record.after.events.some((event) =>
    event.isTrusted && event.inputType === "deleteContentBackward")), "native delete did not emit trusted backward input");
  console.log("BACKSPACE_PROBE_COMPLETE " + JSON.stringify(observations.map(({ name, operation, passed }) => ({ name, operation, passed }))));
  assert.ok(observations.every((record) => record.passed), "a joined line retained an unexpected trailing newline");
} finally {
  for (const context of contexts) await context.close();
  await browser.close();
}
