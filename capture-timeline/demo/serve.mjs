// SPDX-License-Identifier: Apache-2.0
// Serves the clip list demo to your own computer's browser, and nothing else.
//
// A browser cannot run TypeScript, so this takes the types out of each .ts file as the browser asks for it
// (Node can do that itself). Nothing is built, installed or written to disk. It listens on 127.0.0.1 only,
// so no other computer can reach it, and it hands out only the two folders the demo needs.
//
//   node demo/serve.mjs            start it and print the address
//   node demo/serve.mjs --open     also open the address in your browser
//   node demo/serve.mjs --port 9000

import { spawn } from "node:child_process";
import { readFile, stat } from "node:fs/promises";
import { createServer } from "node:http";
import module from "node:module";
import { dirname, extname, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const SERVED = ["capture-timeline", "capture-edit/src"].map((p) => resolve(repo, p));
const START = "/capture-timeline/demo/index.html";

const strip = module.stripTypeScriptTypes;
if (typeof strip !== "function") {
  console.error("");
  console.error("This copy of Node.js is too old to show the demo.");
  console.error(`It is version ${process.versions.node}. The demo needs version 22.13 or newer.`);
  console.error("The current LTS version from https://nodejs.org is fine.");
  process.exit(1);
}

// Node says once that taking types out is "experimental". That is true and not useful here.
const emitWarning = process.emitWarning;
process.emitWarning = (warning, ...rest) => {
  if (String(warning).includes("stripTypeScriptTypes")) return;
  return emitWarning.call(process, warning, ...rest);
};

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".ts": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
};

/** The file a URL path names, if it is inside one of the served folders. */
function fileFor(urlPath) {
  let path;
  try {
    path = decodeURIComponent(urlPath);
  } catch {
    return undefined;
  }
  if (path.includes("\0")) return undefined;
  const file = resolve(repo, normalize("." + path));
  const inside = SERVED.some((root) => file === root || file.startsWith(root + sep));
  return inside ? file : undefined;
}

const stripped = new Map(); // file -> { mtimeMs, code }

async function send(file) {
  const kind = TYPES[extname(file).toLowerCase()];
  if (kind === undefined) return { status: 404, type: "text/plain; charset=utf-8", body: "Not found." };
  const info = await stat(file);
  if (!info.isFile()) return { status: 404, type: "text/plain; charset=utf-8", body: "Not found." };
  if (extname(file).toLowerCase() === ".ts") {
    const cached = stripped.get(file);
    if (cached !== undefined && cached.mtimeMs === info.mtimeMs) return { status: 200, type: kind, body: cached.code };
    const source = await readFile(file, "utf8");
    const code = strip(source, { mode: "strip" });
    stripped.set(file, { mtimeMs: info.mtimeMs, code });
    return { status: 200, type: kind, body: code };
  }
  return { status: 200, type: kind, body: await readFile(file) };
}

const server = createServer(async (request, response) => {
  const done = (status, type, body) => {
    response.writeHead(status, { "content-type": type, "cache-control": "no-store", "x-content-type-options": "nosniff" });
    response.end(request.method === "HEAD" ? undefined : body);
  };
  try {
    if (request.method !== "GET" && request.method !== "HEAD") return done(405, "text/plain; charset=utf-8", "Only GET is served.");
    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    if (url.pathname === "/") {
      response.writeHead(302, { location: START });
      return response.end();
    }
    const file = fileFor(url.pathname);
    if (file === undefined) return done(404, "text/plain; charset=utf-8", "Not found.");
    const found = await send(file);
    return done(found.status, found.type, found.body);
  } catch (error) {
    if (error && error.code === "ENOENT") return done(404, "text/plain; charset=utf-8", "Not found.");
    console.error(`Could not serve ${request.url}: ${error instanceof Error ? error.message : error}`);
    return done(500, "text/plain; charset=utf-8", `This file could not be prepared: ${error instanceof Error ? error.message : error}`);
  }
});

function openInBrowser(address) {
  const [command, args] =
    process.platform === "win32"
      ? ["cmd", ["/c", "start", "", address]]
      : process.platform === "darwin"
        ? ["open", [address]]
        : ["xdg-open", [address]];
  const child = spawn(command, args, { stdio: "ignore", detached: true });
  child.on("error", () => console.log("The browser could not be opened by itself. Please open the address above."));
  child.unref();
}

const argv = process.argv.slice(2);
const portAt = argv.indexOf("--port");
const first = portAt >= 0 ? Number(argv[portAt + 1]) : 8765;
if (!Number.isInteger(first) || first < 1 || first > 65000) {
  console.error("The port must be a whole number between 1 and 65000.");
  process.exit(1);
}

function listen(port, left) {
  server.once("error", (error) => {
    if (error.code === "EADDRINUSE" && left > 0) return listen(port + 1, left - 1);
    console.error(`The demo could not start: ${error.message}`);
    process.exit(1);
  });
  server.listen(port, "127.0.0.1", () => {
    const address = `http://127.0.0.1:${port}${START}`;
    console.log("");
    console.log("The clip list demo is running. Open this address in your browser:");
    console.log("");
    console.log(`    ${address}`);
    console.log("");
    console.log("It uses made-up data and changes nothing on your computer.");
    console.log("To stop it, close this window or press Ctrl+C.");
    if (argv.includes("--open")) openInBrowser(address);
  });
}

listen(first, 20);
