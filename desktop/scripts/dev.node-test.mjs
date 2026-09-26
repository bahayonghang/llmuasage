import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer, Server } from "node:net";
import { test } from "node:test";
import { runDesktopDev, startDevServer } from "./dev.mjs";

async function occupyPort() {
  const server = createServer();
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  return server;
}

async function assertApp(url) {
  const response = await fetch(url, { signal: AbortSignal.timeout(10000) });
  assert.equal(response.status, 200);
  assert.ok((await response.text()).includes('<div id="root">'));
}

test("uses another port when the requested port is occupied", async (t) => {
  const blocker = await occupyPort();
  t.after(() => new Promise((resolve) => blocker.close(resolve)));
  const port = blocker.address().port;
  const server = await startDevServer({ server: { port }, logLevel: "silent" });
  t.after(() => server.close());
  assert.notEqual(server.httpServer.address().port, port);
  await assertApp(server.resolvedUrls.local[0]);
});

test("skips denied ports after an occupied port without accumulating listeners", async (t) => {
  const blocker = await occupyPort();
  t.after(() => new Promise((resolve) => blocker.close(resolve)));
  const port = blocker.address().port;
  const listen = Server.prototype.listen;
  let denied = 0;
  const warnings = [];
  const onWarning = (warning) => warnings.push(warning.name);
  process.on("warning", onWarning);
  t.after(() => process.off("warning", onWarning));
  t.mock.method(Server.prototype, "listen", function (...args) {
    if (args[0] > port && args[0] <= port + 12) {
      denied += 1;
      const onListening = args.at(-1);
      if (typeof onListening === "function") this.once("listening", onListening);
      queueMicrotask(() => this.emit("error", Object.assign(new Error("permission denied"), {
        code: "EACCES",
        port: args[0],
      })));
      return this;
    }
    return listen.apply(this, args);
  });
  const server = await startDevServer({ server: { port }, logLevel: "silent" });
  t.after(() => server.close());
  assert.equal(denied, 12);
  assert.ok(server.httpServer.address().port > port + 12);
  await assertApp(server.resolvedUrls.local[0]);
  assert.ok(!warnings.includes("MaxListenersExceededWarning"));
});

test("passes the fallback URL to Tauri and closes Vite after Tauri exits", async (t) => {
  const listen = Server.prototype.listen;
  t.mock.method(Server.prototype, "listen", function (...args) {
    if (args[0] === 43173) {
      const onListening = args.at(-1);
      if (typeof onListening === "function") this.once("listening", onListening);
      queueMicrotask(() => this.emit("error", Object.assign(new Error("address in use"), {
        code: "EADDRINUSE",
        port: 43173,
      })));
      return this;
    }
    return listen.apply(this, args);
  });
  let devUrl;
  await runDesktopDev(["--no-watch"], async (args) => {
    assert.equal(args[0], "dev");
    assert.ok(args.includes("--no-watch"));
    const config = JSON.parse(args[args.indexOf("--config") + 1]);
    assert.equal(config.build.beforeDevCommand, "");
    devUrl = config.build.devUrl;
    assert.equal(new URL(devUrl).hostname, "127.0.0.1");
    assert.ok(Number(new URL(devUrl).port) > 43173);
    await assertApp(devUrl);
  });
  await assert.rejects(fetch(devUrl, { signal: AbortSignal.timeout(1000) }));
});

test("closes Vite when Tauri fails", async () => {
  let devUrl;
  const failure = new Error("Tauri startup failed");
  await assert.rejects(runDesktopDev([], async (args) => {
    devUrl = JSON.parse(args[args.indexOf("--config") + 1]).build.devUrl;
    await assertApp(devUrl);
    throw failure;
  }), failure);
  await assert.rejects(fetch(devUrl, { signal: AbortSignal.timeout(1000) }));
});
