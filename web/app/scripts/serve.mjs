// Static files only. No upload, codec, database, or application endpoints.
import { createServer as httpServer } from "node:http";
import { createServer as httpsServer } from "node:https";
import { readFile, realpath, stat } from "node:fs/promises";
import { createReadStream } from "node:fs";
import { resolve, extname, sep } from "node:path";
import { networkInterfaces } from "node:os";
import { parseArgs } from "node:util";
import { fileURLToPath } from "node:url";

const { values } = parseArgs({
  options: {
    host: { type: "string", default: "0.0.0.0" },
    port: { type: "string", default: "8787" },
    cert: { type: "string" },
    key: { type: "string" },
  },
});
if (!!values.cert !== !!values.key)
  throw new Error("Supply both --cert and --key for HTTPS");
const port = Number(values.port);
if (!Number.isInteger(port) || port < 1 || port > 65535)
  throw new Error("Invalid port");
const root = await realpath(fileURLToPath(new URL("../dist", import.meta.url)));
const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".webmanifest": "application/manifest+json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

async function serve(req, res) {
  res.setHeader("X-Content-Type-Options", "nosniff");
  res.setHeader("Referrer-Policy", "no-referrer");
  res.setHeader(
    "Content-Security-Policy",
    "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; media-src 'self' blob:; img-src 'self' blob: data:; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'",
  );
  if (!["GET", "HEAD"].includes(req.method)) {
    res.writeHead(405, { Allow: "GET, HEAD" }).end();
    req.resume();
    return;
  }
  try {
    const pathname = decodeURIComponent(
      new URL(req.url, "http://static.local").pathname,
    );
    const file = await realpath(
      resolve(root, "." + (pathname === "/" ? "/index.html" : pathname)),
    );
    if (!file.startsWith(root + sep)) {
      res.writeHead(404).end();
      return;
    }
    const info = await stat(file);
    if (!info.isFile()) {
      res.writeHead(404).end();
      return;
    }
    const etag = `"${info.size.toString(16)}-${info.mtimeMs.toString(16)}"`;
    const immutable = file.startsWith(resolve(root, "assets") + sep);
    res.setHeader(
      "Cache-Control",
      immutable ? "public, max-age=31536000, immutable" : "no-cache",
    );
    res.setHeader("ETag", etag);
    if (req.headers["if-none-match"] === etag) {
      res.writeHead(304).end();
      return;
    }
    res.setHeader(
      "Content-Type",
      types[extname(file)] || "application/octet-stream",
    );
    res.setHeader("Content-Length", info.size);
    res.writeHead(200);
    if (req.method === "HEAD") {
      res.end();
      return;
    }
    createReadStream(file)
      .on("error", () => res.destroy())
      .pipe(res);
  } catch (err) {
    res.writeHead(err instanceof URIError ? 400 : 404).end();
  }
}

const tls = values.cert
  ? { cert: await readFile(values.cert), key: await readFile(values.key) }
  : null;
const server = tls ? httpsServer(tls, serve) : httpServer(serve);
server.on("error", (err) => {
  console.error(err.message);
  process.exitCode = 1;
});
server.listen(port, values.host, () => {
  const protocol = tls ? "https" : "http";
  console.log(`QRX static site: ${protocol}://localhost:${port}`);
  if (values.host === "0.0.0.0") {
    for (const entries of Object.values(networkInterfaces())) {
      for (const entry of entries || []) {
        if (entry.family === "IPv4" && !entry.internal)
          console.log(`LAN: ${protocol}://${entry.address}:${port}`);
      }
    }
  }
});
