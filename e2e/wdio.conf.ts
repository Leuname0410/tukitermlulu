import { spawn, type ChildProcess } from "node:child_process";
import { once } from "node:events";
import { existsSync } from "node:fs";
import net from "node:net";
import { fileURLToPath } from "node:url";

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));
const binary = process.platform === "win32" ? "tuki-desktop.exe" : "tuki-desktop";
const application = fileURLToPath(new URL(`../target/release/${binary}`, import.meta.url));

/**Arranca tauri-driver y espera a que acepte conexiones en el puerto fijo del ticket.*/
async function startDriver(): Promise<ChildProcess> {
  if (!existsSync(application)) {
    throw new Error(`Falta el binario release: ${application}`);
  }
  const driver = spawn(binary === "tuki-desktop.exe" ? "tauri-driver.exe" : "tauri-driver", [], {
    stdio: "ignore",
    windowsHide: true,
  });
  driver.on("error", (cause) => console.error("[e2e] No se pudo arrancar tauri-driver", cause));
  driver.on("exit", (code, signal) => {
    if (code !== 0 && signal !== "SIGTERM") {
      console.error(`[e2e] tauri-driver terminó inesperadamente (código ${code})`);
    }
  });

  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const ready = await new Promise<boolean>((resolve) => {
      const socket = net.createConnection({ host: "127.0.0.1", port: 4444 });
      socket.once("connect", () => {
        socket.destroy();
        resolve(true);
      });
      socket.once("error", () => {
        socket.destroy();
        resolve(false);
      });
    });
    if (ready) {
      return driver;
    }
    await delay(150);
  }
  driver.kill();
  throw new Error("tauri-driver no respondió en 127.0.0.1:4444");
}

let driver: ChildProcess | undefined;

export const config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  path: "/",
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application },
    },
  ],
  logLevel: "warn",
  waitforTimeout: 10_000,
  connectionRetryTimeout: 30_000,
  connectionRetryCount: 0,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { timeout: 180_000 },
  async onPrepare(): Promise<void> {
    driver = await startDriver();
  },
  async onComplete(): Promise<void> {
    if (driver !== undefined && driver.exitCode === null) {
      driver.kill();
      await Promise.race([once(driver, "exit"), delay(3_000)]);
      if (driver.exitCode === null) {
        driver.kill("SIGKILL");
      }
    }
  },
};
