import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { backend as port, gateway } from "./ports";

const root = resolve(import.meta.dirname, "..", "..", "..");

function devEnv(): Record<string, string> {
  const env: Record<string, string> = {};
  for (const line of readFileSync(join(root, "deploy/dev.env"), "utf8").split("\n")) {
    const m = /^([A-Z0-9_]+)=(.*)$/.exec(line.trim());
    if (m) env[m[1]] = m[2];
  }
  env.PERMISSION__SERVER__BIND = `127.0.0.1:${port}`;
  env.BASE = `http://127.0.0.1:${port}`;
  if (process.env.E2E_DATABASE_URL) env.PERMISSION__DATABASE__URL = process.env.E2E_DATABASE_URL;
  return env;
}

function run(cmd: string, args: string[], tolerant = false) {
  try {
    execFileSync(cmd, args, { stdio: "ignore" });
  } catch (e) {
    if (!tolerant) throw e;
  }
}

async function waitFor(url: string, ms = 60_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`${url} ne répond pas`);
}

export default async function setup() {
  const env = { ...process.env, ...devEnv() };
  const docker = !process.env.E2E_DATABASE_URL;
  const compose = ["compose", "-f", join(root, "deploy/docker-compose.dev.yml")];
  if (docker) {
    run("docker", [...compose, "rm", "-sf", "postgres"], true);
    run("docker", [...compose, "up", "-d", "--wait", "postgres"]);
  }
  if (gateway) {
    run(join(root, "scripts/security/dev.sh"), ["down"], true);
    run(join(root, "scripts/security/dev.sh"), ["up"]);
    run(join(root, "scripts/security/dev.sh"), ["start"]);
  }

  execFileSync("cargo", ["build", "-p", "permission"], { cwd: root, stdio: "inherit" });
  const target = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
  const backend: ChildProcess = spawn(join(target, "debug/permission"), [], {
    cwd: join(root, "api/permission"),
    env,
    stdio: "inherit",
  });
  await waitFor(`http://127.0.0.1:${port}/health`);
  execFileSync("python3", [join(root, "scripts/permission/seed.py")], { env, stdio: "ignore" });

  return () => {
    backend.kill();
    if (gateway) run(join(root, "scripts/security/dev.sh"), ["down"], true);
    if (docker) run("docker", [...compose, "rm", "-sf", "postgres"], true);
  };
}
