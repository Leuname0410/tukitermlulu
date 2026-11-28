/// <reference types="@wdio/globals/types" />
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { $, browser, expect } from "@wdio/globals";

// La subida solo existe arrastrando desde el SO y WebDriver no puede automatizarla:
// ya está cubierta por tests de integración y queda en el checklist manual T-36.

const run = promisify(execFile);
const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));
const median = (values: readonly number[]): number =>
  [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)] ?? 0;

/**Mide desde que se crea la sesión hasta que se ve el formulario de conexión rápida.*/
async function coldStartMs(): Promise<number> {
  const started = Date.now();
  await browser.reloadSession();
  const form = await $('form[aria-label="Conexión rápida"]');
  await form.waitForExist({ timeout: 20_000 });
  return Date.now() - started;
}

interface RamProcess {
  pid: number;
  name: string;
  type: string;
  mb: number;
}

interface RamSnapshot {
  processes: RamProcess[];
  groups: Record<string, number>;
  totalMb: number;
}

const RAM_GROUPS = ["app", "browser", "renderer", "gpu-process", "utility", "crashpad", "other"];
/**Meta acordada para Windows; en Linux/macOS el KPI queda pendiente de medición.*/
const WINDOWS_IDLE_RAM_TARGET_MB = 185;

/**Clasifica cada proceso para poder comparar la app frente a WebView2.*/
function ramGroup(process: RamProcess): string {
  if (process.type !== "-") {
    return RAM_GROUPS.includes(process.type) ? process.type : "other";
  }
  const name = process.name.toLowerCase();
  if (name.includes("tuki-desktop")) {
    return "app";
  }
  if (name.includes("webview") || name.includes("msedge")) {
    return "browser";
  }
  return "other";
}

function snapshotOf(processes: RamProcess[]): RamSnapshot {
  const groups: Record<string, number> = {};
  for (const group of RAM_GROUPS) {
    groups[group] = 0;
  }
  for (const process of processes) {
    const group = ramGroup(process);
    groups[group] = (groups[group] ?? 0) + process.mb;
  }
  return {
    processes: [...processes].sort((a, b) => b.mb - a.mb),
    groups,
    totalMb: Math.round(processes.reduce((sum, item) => sum + item.mb, 0) * 10) / 10,
  };
}

/**Desglosa las páginas privadas de la app y sus descendientes WebView2 en Windows.*/
async function ramWindowsSnapshot(): Promise<RamSnapshot> {
  const script = [
    "$all = Get-CimInstance Win32_Process",
    "$app = $all | Where-Object { $_.Name -eq 'tuki-desktop.exe' } | Sort-Object CreationDate -Descending | Select-Object -First 1",
    "if (-not $app) { throw 'No se encontró el proceso de la app' }",
    "$tree = @($app)",
    "do {",
    "  $added = $false",
    "  foreach ($p in $all) {",
    "    if ($p.ParentProcessId -in $tree.ProcessId -and $p -notin $tree) { $tree += $p; $added = $true }",
    "  }",
    "} while ($added)",
    "foreach ($p in $tree) {",
    "  $cmd = [string]$p.CommandLine",
    "  $type = '-'",
    "  if ($cmd -match '--type=([\\w-]+)') { $type = $Matches[1] }",
    "  elseif ($p.Name -like 'msedgewebview2*') { $type = 'browser' }",
    "  '{0}|{1}|{2}|{3}' -f $p.ProcessId, $p.Name, $type, [math]::Round($p.PrivatePageCount / 1MB, 1)",
    "}",
  ].join("\n");
  const { stdout } = await run("powershell.exe", ["-NoLogo", "-NoProfile", "-Command", script]);
  const lines = stdout.trim().split(/\r?\n/).filter((line) => line.length > 0);
  const processes = lines.map((line) => {
    const [pid, name, type, mb] = line.split("|");
    const value = Number(mb);
    if (pid === undefined || name === undefined || type === undefined || !Number.isFinite(value)) {
      throw new Error(`RAM no válida: ${line}`);
    }
    return { pid: Number(pid), name, type, mb: value };
  });
  if (processes.length === 0) {
    throw new Error(`RAM sin procesos: ${stdout}`);
  }
  return snapshotOf(processes);
}

/**Desglosa el Pss de la app y sus descendientes recorriendo /proc por PPID en Linux.*/
async function ramLinuxSnapshot(): Promise<RamSnapshot> {
  type Entry = RamProcess & { ppid: number };
  const processes = new Map<number, Entry>();
  for (const entry of await fs.readdir("/proc")) {
    if (!/^\d+$/.test(entry)) {
      continue;
    }
    const pid = Number(entry);
    let status: string;
    let rollup: string;
    let cmdline: string;
    try {
      status = await fs.readFile(`/proc/${pid}/status`, "utf8");
      rollup = await fs.readFile(`/proc/${pid}/smaps_rollup`, "utf8");
      cmdline = await fs.readFile(`/proc/${pid}/cmdline`, "utf8");
    } catch {
      continue; // Los procesos efímeros desaparecen mientras se recorre /proc.
    }
    const name = status.match(/^Name:\s+(.+)$/m)?.[1] ?? "";
    const type = cmdline.match(/--type=([\w-]+)/)?.[1] ?? "-";
    processes.set(pid, {
      pid,
      ppid: Number(status.match(/^PPid:\s+(\d+)$/m)?.[1] ?? 0),
      name,
      type: type !== "-" ? type : name.startsWith("tuki-desktop") ? "app" : "browser",
      mb: Number(rollup.match(/^Pss:\s+(\d+)\s+kB$/m)?.[1] ?? 0) / 1024,
    });
  }
  const candidates = [...processes.values()].filter((item) => item.name.startsWith("tuki-desktop"));
  const root =
    candidates.find((item) => !candidates.some((other) => other.ppid === item.pid)) ??
    candidates.at(0);
  if (root === undefined) {
    throw new Error("No se encontró el proceso de la app");
  }
  const tree: Entry[] = [root];
  let added = true;
  while (added) {
    added = false;
    for (const item of processes.values()) {
      if (!tree.includes(item) && tree.some((parent) => parent.pid === item.ppid)) {
        tree.push(item);
        added = true;
      }
    }
  }
  return snapshotOf(tree);
}

/**Huella visual de la terminal, válida tanto para el renderer DOM como para WebGL.*/
async function terminalVisualSignature(): Promise<string> {
  const region = await $('div[role="region"][aria-label="Terminal"]');
  const png = await region.takeScreenshot();
  return createHash("sha256").update(png).digest("hex").slice(0, 16);
}

/**Mide desde que se envían 200 caracteres hasta que cambia el render de la terminal.*/
async function terminalLatencyMs(): Promise<number> {
  await browser.execute(() => {
    const input = document.querySelector('textarea[aria-label="Terminal input"]');
    if (input instanceof HTMLTextAreaElement) {
      input.focus();
    }
  });
  const baseline = new Set<string>();
  for (let sample = 0; sample < 8; sample += 1) {
    baseline.add(await terminalVisualSignature());
    await delay(120);
  }
  const started = Date.now();
  await browser.keys("latencia".repeat(25));
  await browser.waitUntil(async () => !baseline.has(await terminalVisualSignature()), {
    timeout: 10_000,
    interval: 50,
    timeoutMsg: "La terminal no mostró los 200 caracteres",
  });
  const elapsed = Date.now() - started;
  await browser.keys(["\uE009", "u"]); // Ctrl+U: limpia la línea sin ejecutarla.
  return elapsed;
}

function ramTable(snapshot: RamSnapshot | null): string {
  if (snapshot === null) {
    return "n/d";
  }
  const groups = RAM_GROUPS.filter((group) => (snapshot.groups[group] ?? 0) > 0).map(
    (group) => `| ${group} | ${(snapshot.groups[group] ?? 0).toFixed(1)} MB |`,
  );
  const processes = snapshot.processes.map(
    (item) => `| ${item.name} (${item.pid}) | ${item.type} | ${item.mb.toFixed(1)} MB |`,
  );
  return [
    "| Proceso (PID) | Tipo WebView2 | MB privados |",
    "| --- | --- | ---: |",
    ...processes,
    "",
    "| Grupo | MB privados |",
    "| --- | ---: |",
    ...groups,
  ].join("\n");
}

/**Fila de RAM: Windows compara contra la meta; el resto solo reporta la medición.*/
function idleRamKpiRow(ramMb: number | null): string {
  const measured = ramMb === null ? "n/d" : `${ramMb.toFixed(1)} MB`;
  if (process.platform !== "win32") {
    return `| RAM en reposo (1 sesión) | ${measured} | por medir | por medir |`;
  }
  const passed = ramMb !== null && ramMb <= WINDOWS_IDLE_RAM_TARGET_MB;
  return `| RAM en reposo (1 sesión) | ${measured} | <= ${WINDOWS_IDLE_RAM_TARGET_MB} MB | ${passed ? "PASS" : "FAIL"} |`;
}

function kpiTable(bootMs: number, snapshot: RamSnapshot | null, latencyMs: number | null): string {
  const ramMb = snapshot?.totalMb ?? null;
  return [
    "| KPI | Medición | Meta | Resultado |",
    "| --- | ---: | ---: | :---: |",
    `| Arranque en frío (mediana de 3) | ${bootMs} ms | < 1000 ms | ${bootMs < 1000 ? "PASS" : "FAIL"} |`,
    idleRamKpiRow(ramMb),
    `| Latencia terminal (200 caracteres) | ${latencyMs === null ? "n/d" : `${latencyMs} ms`} | n/d | n/d |`,
    "",
    "### Desglose de RAM (Windows: bytes privados; Linux: PSS)",
    ramTable(snapshot),
  ].join("\n");
}

/**Enfoca el textarea accesible de xterm; los caracteres llegan al PTY por onData.*/
async function sendTerminal(command: string): Promise<void> {
  await browser.execute(() => {
    const input = document.querySelector('textarea[aria-label="Terminal input"]');
    if (input instanceof HTMLTextAreaElement) {
      input.focus();
    }
  });
  await browser.keys(command);
  await browser.keys("Enter");
}

/**Espera a que el archivo aparezca en el sidebar, refrescando si hace falta.*/
async function waitForEntry(fileName: string): Promise<void> {
  const row = await $(`//table[@aria-label="Entradas remotas"]//tr[contains(., "${fileName}")]`);
  await browser.waitUntil(
    async () => {
      if (await row.isExisting()) {
        return true;
      }
      await (await $('//button[@aria-label="Refrescar"]')).click();
      return false;
    },
    { timeout: 8_000, interval: 2_000, timeoutMsg: `El archivo ${fileName} no apareció en el sidebar` },
  );
}

/**Reenvía `cd` si WebView2 perdió el primer envío de teclado; cada intento exige 5 s.*/
async function followTerminalTo(target: string): Promise<void> {
  const route = await $('input[aria-label="Ruta remota"]');
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    await sendTerminal(`cd ${target}`);
    try {
      await browser.waitUntil(async () => (await route.getValue()) === target, {
        timeout: 5_000,
        timeoutMsg: `El sidebar no siguió la terminal hasta ${target}`,
      });
      return;
    } catch (cause) {
      if (attempt === 3) {
        throw cause;
      }
      console.warn(`[e2e] Reintentando cd ${target}`, cause);
      await delay(2_000);
    }
  }
}

describe("flujo e2e [T-31a]", () => {
  let bootMs: number | undefined;
  let ramPromise: Promise<RamSnapshot | null> | undefined;
  let latencyMs: number | undefined;
  let fileName: string | undefined;
  let localFile: string | undefined;

  after(async () => {
    const table = kpiTable(
      bootMs ?? 0,
      ramPromise === undefined ? null : await ramPromise,
      latencyMs ?? null,
    );
    console.log(table);
    const summary = process.env.GITHUB_STEP_SUMMARY;
    if (summary !== undefined) {
      try {
        await fs.appendFile(summary, `## KPIs e2e [T-31a]\n\n${table}\n`);
      } catch (cause) {
        console.warn("[e2e] No se pudo escribir GITHUB_STEP_SUMMARY", cause);
      }
    }
    if (fileName !== undefined) {
      try {
        await sendTerminal(`rm -f /tmp/${fileName}`);
      } catch (cause) {
        console.warn("[e2e] No se pudo borrar el archivo remoto", cause);
      }
    }
    if (localFile !== undefined) {
      try {
        await fs.rm(localFile, { force: true });
      } catch (cause) {
        console.warn("[e2e] No se pudo borrar la descarga", cause);
      }
    }
  });

  it("should connect, follow the terminal folder and download a remote file", async () => {
    const boots = [await coldStartMs(), await coldStartMs(), await coldStartMs()];
    bootMs = median(boots);

    await (
      await $('//form[@aria-label="Conexión rápida"]//label[contains(., "Host")]//input')
    ).setValue("127.0.0.1");
    await (
      await $('//form[@aria-label="Conexión rápida"]//label[contains(., "Puerto")]//input')
    ).setValue("2222");
    await (
      await $('//form[@aria-label="Conexión rápida"]//label[contains(., "Usuario")]//input')
    ).setValue("tuki-bash");
    await (
      await $('//form[@aria-label="Conexión rápida"]//button[normalize-space()="Conectar"]')
    ).click();

    const hostDialog = await $('dialog[aria-label="Huella del host"]');
    const secretDialog = await $('dialog[aria-label="Diálogo de secreto"]');
    await browser.waitUntil(
      async () => (await hostDialog.isExisting()) || (await secretDialog.isExisting()),
      { timeout: 20_000, timeoutMsg: "No apareció la huella ni la petición de contraseña" },
    );
    if (await hostDialog.isExisting()) {
      await (await hostDialog.$('//button[normalize-space()="Aceptar"]')).click();
      await secretDialog.waitForExist({ timeout: 20_000 });
    }
    await (
      await $('//dialog[@aria-label="Diálogo de secreto"]//label[contains(., "Contraseña")]//input[@type="password"]')
    ).setValue("tuki-test");
    await (
      await $('//dialog[@aria-label="Diálogo de secreto"]//button[normalize-space()="Conectar"]')
    ).click();

    const terminal = await $('div[role="region"][aria-label="Terminal"]');
    const sidebar = await $('aside[aria-label="Explorador SFTP"]');
    await browser.waitUntil(async () => (await terminal.isExisting()) && (await sidebar.isExisting()), {
      timeout: 30_000,
      timeoutMsg: "No aparecieron la terminal y el explorador SFTP",
    });
    ramPromise = delay(5_000)
      .then(() => (process.platform === "win32" ? ramWindowsSnapshot() : ramLinuxSnapshot()))
      .catch((cause: unknown) => {
        console.warn("[e2e] No se pudo medir la RAM en reposo", cause);
        return null;
      });

    // La integración OSC 7 puede tardar varios segundos en instalarse tras conectar.
    await delay(12_000);
    await followTerminalTo("/tmp");
    const follow = await $('//label[contains(., "Seguir terminal")]//input[@type="checkbox"]');
    expect(await follow.isSelected()).toBe(true);
    latencyMs = await terminalLatencyMs();

    fileName = `e2e-${Date.now()}.txt`;
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      await sendTerminal(`echo e2e > /tmp/${fileName}`);
      try {
        await waitForEntry(fileName);
        break;
      } catch (cause) {
        if (attempt === 3) {
          throw cause;
        }
        console.warn(`[e2e] Reintentando crear ${fileName}`, cause);
      }
    }
    const row = await $(`//table[@aria-label="Entradas remotas"]//tr[contains(., "${fileName}")]`);

    await row.click({ button: "right" });
    const download = await $('//div[@role="menu"]//button[@role="menuitem" and normalize-space()="Descargar"]');
    await download.waitForExist({ timeout: 5_000 });
    await download.click();
    const target = path.join(os.homedir(), "Downloads", fileName);
    localFile = target;
    await fs.rm(target, { force: true });
    await browser.waitUntil(
      async () => {
        try {
          return (await fs.readFile(target, "utf8")).trim() === "e2e";
        } catch {
          return false; // El archivo todavía no ha terminado de descargarse.
        }
      },
      { timeout: 20_000, timeoutMsg: "El archivo no apareció en Descargas con contenido e2e" },
    );
  });
});
