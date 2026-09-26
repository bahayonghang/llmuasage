import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { run } from "@tauri-apps/cli";
import { createServer } from "vite";

const desktopDir = fileURLToPath(new URL("../", import.meta.url));

export async function startDevServer(options = {}) {
  let config = { root: desktopDir, ...options };
  for (;;) {
    const server = await createServer(config);
    try {
      await server.listen();
      return server;
    } catch (error) {
      await server.close();
      if (error.code !== "EACCES" || !Number.isInteger(error.port) || error.port >= 65535) {
        throw error;
      }
      const port = error.port + 1;
      server.config.logger.info(`Port ${error.port} is unavailable, trying ${port}...`);
      config = { ...config, server: { ...config.server, port } };
    }
  }
}

export async function runDesktopDev(args = [], runTauri = run) {
  const server = await startDevServer();
  try {
    server.printUrls();
    const config = {
      build: {
        beforeDevCommand: "",
        devUrl: server.resolvedUrls.local[0],
      },
    };
    await runTauri(["dev", "--config", JSON.stringify(config), ...args], "tauri");
  } finally {
    await server.close();
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.chdir(desktopDir);
  runDesktopDev(process.argv.slice(2)).catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
